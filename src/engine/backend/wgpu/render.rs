//! Headless WGPU rasterization of [`PaintDocument`] to PNG bytes.

use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Cache, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping, Style,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use image::{ImageBuffer, Rgba};
use wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

use crate::engine::backend::painter::{PaintDocument, PainterCommand};
use crate::engine::styles::Color;

/// Same vertical gap between pages as [`super::super::svg::SvgBackend`].
pub(crate) const PAGE_GAP_PT: f32 = 12.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SolidVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

fn to_ndc(px: f32, py: f32, cw: f32, ch: f32) -> [f32; 2] {
    [px / cw * 2.0 - 1.0, 1.0 - py / ch * 2.0]
}

fn lura_to_linear_rgba(c: Color, opacity: f32) -> [f32; 4] {
    let a = opacity.clamp(0.0, 1.0);
    [c.r, c.g, c.b, a]
}

fn glyphon_color(c: Color, opacity: f32) -> glyphon::Color {
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    glyphon::Color::rgba(
        (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.b * 255.0).round().clamp(0.0, 255.0) as u8,
        a,
    )
}

fn rect_fill_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    c: Color,
    opacity: f32,
    cw: f32,
    ch: f32,
) -> [SolidVertex; 6] {
    let col = lura_to_linear_rgba(c, opacity);
    let v = |px: f32, py: f32| SolidVertex {
        pos: to_ndc(px, py, cw, ch),
        color: col,
    };
    [
        v(x, y),
        v(x + w, y),
        v(x, y + h),
        v(x + w, y),
        v(x + w, y + h),
        v(x, y + h),
    ]
}

fn rect_stroke_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: Color,
    sw: f32,
    opacity: f32,
    cw: f32,
    ch: f32,
) -> Vec<SolidVertex> {
    if sw <= 0.0 {
        return Vec::new();
    }
    let hw = sw * 0.5;
    let mut out = Vec::with_capacity(24);
    // Top, bottom, left, right strips (two tris each)
    let quads = [
        (x - hw, y - hw, w + sw, sw),           // top
        (x - hw, y + h - hw, w + sw, sw),       // bottom
        (x - hw, y - hw, sw, h + sw),            // left
        (x + w - hw, y - hw, sw, h + sw),        // right
    ];
    for (qx, qy, qw, qh) in quads {
        out.extend_from_slice(&rect_fill_vertices(qx, qy, qw, qh, stroke, opacity, cw, ch));
    }
    out
}

fn line_vertices(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
    c: Color,
    opacity: f32,
    cw: f32,
    ch: f32,
) -> [SolidVertex; 6] {
    let hw = width * 0.5;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let nx = (-dy / len) * hw;
    let ny = (dx / len) * hw;
    let col = lura_to_linear_rgba(c, opacity);
    let v = |px: f32, py: f32| SolidVertex {
        pos: to_ndc(px, py, cw, ch),
        color: col,
    };
    [
        v(x1 + nx, y1 + ny),
        v(x1 - nx, y1 - ny),
        v(x2 + nx, y2 + ny),
        v(x1 - nx, y1 - ny),
        v(x2 - nx, y2 - ny),
        v(x2 + nx, y2 + ny),
    ]
}

/// Canvas size in pixels (1 pt ≈ 1 px), matching stacked SVG pages.
pub(crate) fn canvas_dimensions(doc: &PaintDocument) -> (u32, u32) {
    if doc.pages.is_empty() {
        return (1, 1);
    }
    let w = doc
        .pages
        .iter()
        .fold(0.0f32, |acc, p| acc.max(p.width))
        .ceil()
        .max(1.0) as u32;
    let h_sum: f32 = doc.pages.iter().map(|p| p.height).sum::<f32>()
        + PAGE_GAP_PT * (doc.pages.len().saturating_sub(1) as f32);
    let h = h_sum.ceil().max(1.0) as u32;
    (w, h)
}

fn flat_commands<'a>(doc: &'a PaintDocument) -> Vec<(f32, &'a PainterCommand)> {
    let mut y_off = 0.0f32;
    let mut out = Vec::new();
    for (i, page) in doc.pages.iter().enumerate() {
        for cmd in &page.commands {
            out.push((y_off, cmd));
        }
        y_off += page.height;
        if i + 1 < doc.pages.len() {
            y_off += PAGE_GAP_PT;
        }
    }
    out
}

enum Segment {
    Solid(Vec<SolidVertex>),
    Text(Vec<OwnedTextArea>),
}

struct OwnedTextArea {
    buffer: Buffer,
    left: f32,
    top: f32,
    default_color: glyphon::Color,
}

struct RenderBatch {
    scissor: (u32, u32, u32, u32),
    segment: Segment,
}

fn full_viewport_scissor(width: u32, height: u32) -> (u32, u32, u32, u32) {
    (0, 0, width.max(1), height.max(1))
}

fn clip_rect_to_scissor(
    x: f32,
    y_top: f32,
    w: f32,
    h: f32,
    canvas_w: u32,
    canvas_h: u32,
) -> (u32, u32, u32, u32) {
    let cw = canvas_w as f32;
    let ch = canvas_h as f32;
    let sx = x.floor().clamp(0.0, cw) as u32;
    let sy = y_top.floor().clamp(0.0, ch) as u32;
    let sw = w.ceil().clamp(1.0, cw) as u32;
    let sh = h.ceil().clamp(1.0, ch) as u32;
    // Empty extent (sw/sh = 0) when the rect is past the edge; batches skip these scissors.
    let sw = sw.min(canvas_w.saturating_sub(sx));
    let sh = sh.min(canvas_h.saturating_sub(sy));
    (sx, sy, sw, sh)
}

fn scissor_intersect(
    a: (u32, u32, u32, u32),
    b: (u32, u32, u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    let w = x2.saturating_sub(x1);
    let h = y2.saturating_sub(y1);
    if w == 0 || h == 0 {
        None
    } else {
        Some((x1, y1, w, h))
    }
}

fn build_segments(
    doc: &PaintDocument,
    font_system: &mut FontSystem,
    cw: f32,
    ch: f32,
) -> Vec<RenderBatch> {
    let canvas_w = cw as u32;
    let canvas_h = ch as u32;
    let full_sci = full_viewport_scissor(canvas_w, canvas_h);

    let flat = flat_commands(doc);
    let mut batches: Vec<RenderBatch> = Vec::new();
    let mut solid: Vec<SolidVertex> = Vec::new();
    let mut texts: Vec<OwnedTextArea> = Vec::new();

    let mut op_stack: Vec<f32> = vec![1.0];
    let mut clip_stack: Vec<(u32, u32, u32, u32)> = Vec::new();

    let flush_solid =
        |batches: &mut Vec<RenderBatch>, solid: &mut Vec<SolidVertex>, sci: (u32, u32, u32, u32)| {
            if solid.is_empty() {
                return;
            }
            batches.push(RenderBatch {
                scissor: sci,
                segment: Segment::Solid(std::mem::take(solid)),
            });
        };
    let flush_text =
        |batches: &mut Vec<RenderBatch>, texts: &mut Vec<OwnedTextArea>, sci: (u32, u32, u32, u32)| {
            if texts.is_empty() {
                return;
            }
            batches.push(RenderBatch {
                scissor: sci,
                segment: Segment::Text(std::mem::take(texts)),
            });
        };

    let flush_all = |batches: &mut Vec<RenderBatch>,
                     solid: &mut Vec<SolidVertex>,
                     texts: &mut Vec<OwnedTextArea>,
                     sci: (u32, u32, u32, u32)| {
        flush_solid(batches, solid, sci);
        flush_text(batches, texts, sci);
    };

    for (y_off, cmd) in flat {
        match cmd {
            PainterCommand::PushOpacity { alpha } => {
                let sci_now = clip_stack.last().copied().unwrap_or(full_sci);
                flush_all(&mut batches, &mut solid, &mut texts, sci_now);
                let a = alpha.clamp(0.0, 1.0);
                let cur = *op_stack.last().unwrap_or(&1.0);
                op_stack.push(cur * a);
            }
            PainterCommand::PopOpacity => {
                let sci_now = clip_stack.last().copied().unwrap_or(full_sci);
                flush_all(&mut batches, &mut solid, &mut texts, sci_now);
                op_stack.pop();
                if op_stack.is_empty() {
                    op_stack.push(1.0);
                }
            }
            PainterCommand::PushClipRect { x, y, w, h } => {
                let sci_now = clip_stack.last().copied().unwrap_or(full_sci);
                flush_all(&mut batches, &mut solid, &mut texts, sci_now);
                let r = clip_rect_to_scissor(*x, y_off + y, *w, *h, canvas_w, canvas_h);
                let next = match clip_stack.last().copied() {
                    None => r,
                    Some(base) => scissor_intersect(base, r).unwrap_or((0, 0, 0, 0)),
                };
                clip_stack.push(next);
            }
            PainterCommand::PopClip => {
                let sci_now = clip_stack.last().copied().unwrap_or(full_sci);
                flush_all(&mut batches, &mut solid, &mut texts, sci_now);
                clip_stack.pop();
            }
            PainterCommand::Rect {
                x,
                y,
                w,
                h,
                fill,
                stroke,
                stroke_width,
            } => {
                let sci = clip_stack.last().copied().unwrap_or(full_sci);
                flush_text(&mut batches, &mut texts, sci);
                let op = *op_stack.last().unwrap_or(&1.0);
                if let Some(c) = fill {
                    solid.extend_from_slice(&rect_fill_vertices(
                        *x,
                        y_off + y,
                        *w,
                        *h,
                        *c,
                        op,
                        cw,
                        ch,
                    ));
                }
                if let Some(st) = stroke {
                    solid.extend(rect_stroke_vertices(
                        *x,
                        y_off + y,
                        *w,
                        *h,
                        *st,
                        *stroke_width,
                        op,
                        cw,
                        ch,
                    ));
                }
            }
            PainterCommand::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => {
                let sci = clip_stack.last().copied().unwrap_or(full_sci);
                flush_text(&mut batches, &mut texts, sci);
                let op = *op_stack.last().unwrap_or(&1.0);
                solid.extend_from_slice(&line_vertices(
                    *x1,
                    y_off + y1,
                    *x2,
                    y_off + y2,
                    *width,
                    *color,
                    op,
                    cw,
                    ch,
                ));
            }
            PainterCommand::Text {
                content,
                x,
                y,
                font_size,
                font_family,
                bold,
                italic,
                color,
            } => {
                let sci = clip_stack.last().copied().unwrap_or(full_sci);
                flush_solid(&mut batches, &mut solid, sci);
                if content.is_empty() {
                    continue;
                }
                let op = *op_stack.last().unwrap_or(&1.0);
                let family = if font_family.eq_ignore_ascii_case("helvetica")
                    || font_family.eq_ignore_ascii_case("arial")
                {
                    Family::SansSerif
                } else {
                    Family::Name(font_family.as_str().into())
                };
                let weight = if *bold {
                    Weight::BOLD
                } else {
                    Weight::NORMAL
                };
                let style = if *italic {
                    Style::Italic
                } else {
                    Style::Normal
                };
                let attrs = Attrs::new().family(family).weight(weight).style(style);
                let mut buffer = Buffer::new(
                    font_system,
                    Metrics::new(*font_size, *font_size * 1.25),
                );
                buffer.set_size(font_system, Some(cw), None);
                buffer.set_text(font_system, content, &attrs, Shaping::Advanced, None);
                buffer.shape_until_scroll(font_system, false);
                let top = y_off + y - font_size * 0.85;
                texts.push(OwnedTextArea {
                    buffer,
                    left: *x,
                    top,
                    default_color: glyphon_color(*color, op),
                });
            }
        }
    }
    let sci = clip_stack.last().copied().unwrap_or(full_sci);
    flush_all(&mut batches, &mut solid, &mut texts, sci);
    batches
}

const SOLID_SHADER: &str = r#"
struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

fn create_solid_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("lura solid"),
        source: wgpu::ShaderSource::Wgsl(SOLID_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("lura solid pl"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("lura solid pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SolidVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn read_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let unpadded_bytes_per_row = width * 4;
    let align = COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) / align * align;
    let buffer_size = (padded_bytes_per_row * height) as u64;
    let read_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lura readback"),
        size: buffer_size.max(align as u64),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("lura copy tex"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = read_buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    rx.recv().expect("map channel").expect("map buffer");

    let data = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        rgba.extend_from_slice(&data[start..end]);
    }
    drop(data);
    read_buffer.unmap();
    rgba
}

fn rgba_to_png(width: u32, height: u32, rgba: Vec<u8>) -> Vec<u8> {
    let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, rgba).expect("rgba size");
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let mut out = Vec::new();
    dyn_img
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("png encode");
    out
}

fn fallback_white_png() -> Vec<u8> {
    rgba_to_png(1, 1, vec![255, 255, 255, 255])
}

/// Renders `doc` to PNG bytes. Returns a 1×1 white PNG if no GPU adapter is available.
pub(crate) fn render_to_png(doc: &PaintDocument) -> Vec<u8> {
    pollster::block_on(render_to_png_async(doc)).unwrap_or_else(|_| fallback_white_png())
}

async fn render_to_png_async(doc: &PaintDocument) -> Result<Vec<u8>, ()> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .map_err(|_| ())?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("lura wgpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await
        .map_err(|_| ())?;

    let (width, height) = canvas_dimensions(doc);
    let cw = width as f32;
    let ch = height as f32;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lura target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let format = wgpu::TextureFormat::Rgba8Unorm;
    let solid_pipeline = create_solid_pipeline(&device, format);

    let mut font_system = FontSystem::new();
    let segments = build_segments(doc, &mut font_system, cw, ch);

    let glyph_cache = Cache::new(&device);
    let mut text_atlas = TextAtlas::with_color_mode(
        &device,
        &queue,
        &glyph_cache,
        format,
        ColorMode::Web,
    );
    let mut viewport = Viewport::new(&device, &glyph_cache);
    viewport.update(
        &queue,
        Resolution {
            width,
            height,
        },
    );
    let mut text_renderer = TextRenderer::new(
        &mut text_atlas,
        &device,
        wgpu::MultisampleState::default(),
        None,
    );
    let mut swash_cache = SwashCache::new();

    let mut solid_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lura solid vb"),
        size: 65536,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut first_pass = true;
    let clear = wgpu::Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    if segments.is_empty() {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lura clear only"),
        });
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lura clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            drop(pass);
        }
        queue.submit(Some(encoder.finish()));
        first_pass = false;
    }

    for batch in segments {
        let (sx, sy, sw, sh) = batch.scissor;
        if sw == 0 || sh == 0 {
            continue;
        }
        match batch.segment {
            Segment::Solid(verts) => {
                if verts.is_empty() {
                    continue;
                }
                let need = (verts.len() * std::mem::size_of::<SolidVertex>()) as u64;
                if need > solid_vertex_buffer.size() {
                    solid_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("lura solid vb grown"),
                        size: need.next_power_of_two().max(65536),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                queue.write_buffer(&solid_vertex_buffer, 0, bytemuck::cast_slice(&verts));

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("lura solid"),
                });
                {
                    let load = if first_pass {
                        first_pass = false;
                        wgpu::LoadOp::Clear(clear)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("lura solid pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&solid_pipeline);
                    pass.set_scissor_rect(sx, sy, sw, sh);
                    pass.set_vertex_buffer(0, solid_vertex_buffer.slice(..));
                    pass.draw(0..verts.len() as u32, 0..1);
                }
                queue.submit(Some(encoder.finish()));
            }
            Segment::Text(owned) => {
                let areas: Vec<TextArea<'_>> = owned
                    .iter()
                    .map(|t| TextArea {
                        buffer: &t.buffer,
                        left: t.left,
                        top: t.top,
                        scale: 1.0,
                        bounds: TextBounds::default(),
                        default_color: t.default_color,
                        custom_glyphs: &[],
                    })
                    .collect();
                text_renderer
                    .prepare(
                        &device,
                        &queue,
                        &mut font_system,
                        &mut text_atlas,
                        &viewport,
                        areas,
                        &mut swash_cache,
                    )
                    .map_err(|_| ())?;

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("lura text"),
                });
                {
                    let load = if first_pass {
                        first_pass = false;
                        wgpu::LoadOp::Clear(clear)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("lura text pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_scissor_rect(sx, sy, sw, sh);
                    text_renderer
                        .render(&text_atlas, &viewport, &mut pass)
                        .map_err(|_| ())?;
                }
                queue.submit(Some(encoder.finish()));
            }
        }
    }

    if first_pass {
        // No draw commands: still clear once.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lura clear empty cmds"),
        });
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lura clear empty"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            drop(pass);
        }
        queue.submit(Some(encoder.finish()));
    }

    let rgba = read_rgba_texture(&device, &queue, &texture, width, height);
    Ok(rgba_to_png(width, height, rgba))
}

#[cfg(test)]
mod clip_rect_to_scissor_tests {
    use super::clip_rect_to_scissor;

    #[test]
    fn clip_past_right_edge_yields_zero_width_within_target() {
        let canvas_w = 100u32;
        let canvas_h = 50u32;
        let (sx, sy, sw, sh) = clip_rect_to_scissor(100.0, 0.0, 10.0, 10.0, canvas_w, canvas_h);
        assert_eq!((sx, sy, sw, sh), (100, 0, 0, 10));
        assert!(sx.saturating_add(sw) <= canvas_w);
        assert!(sy.saturating_add(sh) <= canvas_h);
    }

    #[test]
    fn clip_past_bottom_edge_yields_zero_height_within_target() {
        let canvas_w = 80u32;
        let canvas_h = 60u32;
        let (sx, sy, sw, sh) = clip_rect_to_scissor(0.0, 60.0, 80.0, 4.0, canvas_w, canvas_h);
        assert_eq!((sx, sy, sw, sh), (0, 60, 80, 0));
        assert!(sx.saturating_add(sw) <= canvas_w);
        assert!(sy.saturating_add(sh) <= canvas_h);
    }
}
