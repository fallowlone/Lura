//! GPU preview rasterization (`PaintDocument` → PNG) behind feature `wgpu-preview`.

mod render;

use super::painter::{PaintDocument, PainterBackend};

/// Renders a [`PaintDocument`] to PNG bytes via WGPU + glyphon.
pub struct WgpuBackend;

impl PainterBackend for WgpuBackend {
    fn render_document(&self, doc: &PaintDocument) -> Vec<u8> {
        render::render_to_png(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::render::{canvas_dimensions, PAGE_GAP_PT};
    use crate::engine::backend::painter::{PaintDocument, PaintedPage, PainterBackend, PainterCommand};
    use crate::engine::styles::Color;

    #[test]
    fn canvas_empty_document_is_1x1() {
        let doc = PaintDocument { pages: vec![] };
        assert_eq!(canvas_dimensions(&doc), (1, 1));
    }

    #[test]
    fn canvas_single_page_uses_ceil_dimensions() {
        let doc = PaintDocument {
            pages: vec![PaintedPage {
                width: 100.4,
                height: 200.7,
                commands: vec![],
            }],
        };
        assert_eq!(canvas_dimensions(&doc), (101, 201));
    }

    #[test]
    fn canvas_two_pages_adds_gap() {
        let doc = PaintDocument {
            pages: vec![
                PaintedPage {
                    width: 50.0,
                    height: 100.0,
                    commands: vec![],
                },
                PaintedPage {
                    width: 80.0,
                    height: 30.0,
                    commands: vec![],
                },
            ],
        };
        let (w, h) = canvas_dimensions(&doc);
        assert_eq!(w, 80);
        assert_eq!(h, (100.0 + PAGE_GAP_PT + 30.0).ceil() as u32);
    }

    fn gpu_ready() -> bool {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
            instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .is_ok()
        })
    }

    fn decode_rgba(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let img = image::load_from_memory(png).expect("png").into_rgba8();
        let (w, h) = img.dimensions();
        (w, h, img.into_raw())
    }

    #[test]
    fn wgpu_empty_pages_white_png() {
        if !gpu_ready() {
            return;
        }
        let doc = PaintDocument {
            pages: vec![PaintedPage {
                width: 32.0,
                height: 24.0,
                commands: vec![],
            }],
        };
        let png = super::WgpuBackend.render_document(&doc);
        let (w, h, rgba) = decode_rgba(&png);
        assert_eq!((w, h), (32, 24));
        assert!(rgba.chunks(4).all(|p| p[0] > 250 && p[1] > 250 && p[2] > 250));
    }

    #[test]
    fn wgpu_red_fill_center_pixel() {
        if !gpu_ready() {
            return;
        }
        let doc = PaintDocument {
            pages: vec![PaintedPage {
                width: 64.0,
                height: 64.0,
                commands: vec![PainterCommand::Rect {
                    x: 16.0,
                    y: 16.0,
                    w: 32.0,
                    h: 32.0,
                    fill: Some(Color::from_hex(0xff0000)),
                    stroke: None,
                    stroke_width: 0.0,
                }],
            }],
        };
        let png = super::WgpuBackend.render_document(&doc);
        let (w, _h, rgba) = decode_rgba(&png);
        let idx = ((32 * w + 32) * 4) as usize;
        assert!(rgba[idx] > 200, "R={}", rgba[idx]);
        assert!(rgba[idx + 1] < 80, "G={}", rgba[idx + 1]);
        assert!(rgba[idx + 2] < 80, "B={}", rgba[idx + 2]);
    }

    #[test]
    fn wgpu_diagonal_line_pixels_nonwhite() {
        if !gpu_ready() {
            return;
        }
        let doc = PaintDocument {
            pages: vec![PaintedPage {
                width: 48.0,
                height: 48.0,
                commands: vec![PainterCommand::Line {
                    x1: 4.0,
                    y1: 4.0,
                    x2: 44.0,
                    y2: 44.0,
                    color: Color::BLACK,
                    width: 2.0,
                }],
            }],
        };
        let png = super::WgpuBackend.render_document(&doc);
        let (w, h, rgba) = decode_rgba(&png);
        let mut dark = 0u32;
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let r = rgba[i];
                let g = rgba[i + 1];
                let b = rgba[i + 2];
                if r < 40 && g < 40 && b < 40 {
                    dark += 1;
                }
            }
        }
        assert!(dark >= 8, "expected several dark pixels along line, got {dark}");
    }

    #[test]
    fn wgpu_text_produces_dark_pixels() {
        if !gpu_ready() {
            return;
        }
        let doc = PaintDocument {
            pages: vec![PaintedPage {
                width: 120.0,
                height: 40.0,
                commands: vec![PainterCommand::Text {
                    content: "Hello".to_string(),
                    x: 8.0,
                    y: 24.0,
                    font_size: 18.0,
                    font_family: "Arial".to_string(),
                    bold: false,
                    italic: false,
                    color: Color::BLACK,
                }],
            }],
        };
        let png = super::WgpuBackend.render_document(&doc);
        let (w, h, rgba) = decode_rgba(&png);
        let mut dark = 0u32;
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if rgba[i] < 128 && rgba[i + 1] < 128 && rgba[i + 2] < 128 {
                    dark += 1;
                }
            }
        }
        assert!(dark > 50, "expected glyph coverage, dark pixels={dark}");
    }
}
