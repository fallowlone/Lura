/// PDF backend (phase 4)
///
/// Takes a `PageTree` with draw commands and produces PDF file bytes.
/// Uses the `pdf-writer` crate: low-level but fast.
///
/// Supported commands:
/// - `Text`  → PDF BT ... ET (built-in Type1: Helvetica family + Courier family, incl. oblique)
/// - `Rect`  → PDF re + f/S
/// - `Line`  → PDF m + l + S
/// - `PushOpacity` / `PopOpacity` → `q` / `Q` + `ExtGState` (`ca` / `CA`)
/// - `PushClipRect` / `PopClip` → `q` / `Q` + `W n` clip path
///
/// Coordinate systems:
/// pdf-writer: bottom-left = (0,0), Y increases upward.
/// PageTree:   top-left = (0,0), Y increases downward.
/// Conversion: `pdf_y = page_height - our_y`
use pdf_writer::types::{ActionType, AnnotationType};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use crate::engine::paginate::{PageTree, AnchorPosition};
use super::painter::{from_page_tree, PaintDocument, PainterBackend, PainterCommand};
use std::collections::HashMap;

type PageOpacityData = (Vec<f32>, Vec<Ref>, Vec<Vec<u8>>);

#[derive(Debug, Clone)]
struct LinkSpec {
    x: f32,
    y_topdown: f32,
    width: f32,
    font_size: f32,
    uri: String,
}

fn link_specs_from_commands(commands: &[PainterCommand]) -> Vec<LinkSpec> {
    let mut out = Vec::new();
    for cmd in commands {
        let PainterCommand::Text {
            content,
            x,
            y,
            font_size,
            link_uri,
            link_width_pt,
            ..
        } = cmd
        else {
            continue;
        };
        if content.is_empty() {
            continue;
        }
        let (Some(uri), Some(w)) = (link_uri.as_ref(), link_width_pt) else {
            continue;
        };
        if uri.is_empty() || *w <= 1e-3 {
            continue;
        }
        out.push(LinkSpec {
            x: *x,
            y_topdown: *y,
            width: *w,
            font_size: *font_size,
            uri: uri.clone(),
        });
    }
    out
}

pub fn render(page_tree: &PageTree) -> Vec<u8> {
    let doc = from_page_tree(page_tree);
    PdfBackend.render_document_with_anchors(&doc, &page_tree.anchor_positions)
}

pub struct PdfBackend;

impl PainterBackend for PdfBackend {
    /// Trait entry — delegates to the anchor-aware path with no anchors. The
    /// internal-link GoTo branch becomes a no-op when the map is empty, so this
    /// matches the previous behavior of writing every link as a URI action.
    fn render_document(&self, doc: &PaintDocument) -> Vec<u8> {
        self.render_document_with_anchors(doc, &HashMap::new())
    }
}

impl PdfBackend {
    /// Render with anchor support for internal navigation.
    fn render_document_with_anchors(
        &self,
        doc: &PaintDocument,
        anchor_positions: &HashMap<String, AnchorPosition>,
    ) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let mut alloc = RefAlloc::new(1);

        let catalog_id = alloc.next();
        let pages_id = alloc.next();
        let f_helvetica = alloc.next();
        let f_helvetica_bold = alloc.next();
        let f_helvetica_oblique = alloc.next();
        let f_helvetica_bold_oblique = alloc.next();
        let f_courier = alloc.next();
        let f_courier_bold = alloc.next();
        let f_courier_oblique = alloc.next();
        let f_courier_bold_oblique = alloc.next();

        let mut page_ids: Vec<Ref> = Vec::new();
        let mut content_ids: Vec<Ref> = Vec::new();

        for _ in &doc.pages {
            page_ids.push(alloc.next());
            content_ids.push(alloc.next());
        }

        let page_link_specs: Vec<Vec<LinkSpec>> = doc
            .pages
            .iter()
            .map(|p| link_specs_from_commands(&p.commands))
            .collect();
        let mut page_link_annots: Vec<Vec<Ref>> = Vec::with_capacity(doc.pages.len());
        for specs in &page_link_specs {
            let refs: Vec<Ref> = (0..specs.len()).map(|_| alloc.next()).collect();
            page_link_annots.push(refs);
        }

        // Per-page opacity → ExtGState objects (refs allocated before page bodies).
        let mut page_opacity: Vec<PageOpacityData> = Vec::with_capacity(doc.pages.len());
        for page in &doc.pages {
            let alphas = collect_unique_opacity_alphas(&page.commands);
            let mut refs = Vec::with_capacity(alphas.len());
            let mut names = Vec::with_capacity(alphas.len());
            for (j, &a) in alphas.iter().enumerate() {
                let r = alloc.next();
                pdf.ext_graphics(r)
                    .non_stroking_alpha(a)
                    .stroking_alpha(a);
                refs.push(r);
                names.push(format!("Lo{j}").into_bytes());
            }
            page_opacity.push((alphas, refs, names));
        }

        // ─── Catalog ──────────────────────────────────────────────────────────────
        pdf.catalog(catalog_id).pages(pages_id);

        // ─── Page tree ────────────────────────────────────────────────────────────
        let page_width = doc.pages.first().map(|p| p.width).unwrap_or(595.28);
        let page_height = doc.pages.first().map(|p| p.height).unwrap_or(841.89);
        {
            let mut pages = pdf.pages(pages_id);
            pages.media_box(Rect::new(0.0, 0.0, page_width, page_height));
            pages.kids(page_ids.iter().copied());
            pages.count(page_ids.len() as i32);
        }

        // --- Built-in Type1 fonts (PDF 1.7 standard 14 subset we use) ---
        for (id, name) in [
            (f_helvetica, b"Helvetica" as &[u8]),
            (f_helvetica_bold, b"Helvetica-Bold"),
            (f_helvetica_oblique, b"Helvetica-Oblique"),
            (f_helvetica_bold_oblique, b"Helvetica-BoldOblique"),
            (f_courier, b"Courier"),
            (f_courier_bold, b"Courier-Bold"),
            (f_courier_oblique, b"Courier-Oblique"),
            (f_courier_bold_oblique, b"Courier-BoldOblique"),
        ] {
            pdf.type1_font(id)
                .base_font(Name(name))
                .encoding_predefined(Name(b"WinAnsiEncoding"));
        }

        // --- Pages ---
        for (i, page) in doc.pages.iter().enumerate() {
            let page_id = page_ids[i];
            let content_id = content_ids[i];
            let (alphas, gs_refs, gs_names) = &page_opacity[i];
            let link_refs = &page_link_annots[i];

            {
                let mut p = pdf.page(page_id);
                p.media_box(Rect::new(0.0, 0.0, page.width, page.height));
                p.parent(pages_id);
                p.contents(content_id);
                if !link_refs.is_empty() {
                    p.annotations(link_refs.iter().copied());
                }
                let mut res = p.resources();
                {
                    let mut fonts = res.fonts();
                    fonts.pair(Name(b"F1"), f_helvetica);
                    fonts.pair(Name(b"F2"), f_helvetica_bold);
                    fonts.pair(Name(b"F3"), f_helvetica_oblique);
                    fonts.pair(Name(b"F4"), f_helvetica_bold_oblique);
                    fonts.pair(Name(b"F5"), f_courier);
                    fonts.pair(Name(b"F6"), f_courier_bold);
                    fonts.pair(Name(b"F7"), f_courier_oblique);
                    fonts.pair(Name(b"F8"), f_courier_bold_oblique);
                }
                if !alphas.is_empty() {
                    let mut ext = res.ext_g_states();
                    for (buf, &r) in gs_names.iter().zip(gs_refs.iter()) {
                        ext.pair(Name(buf.as_slice()), r);
                    }
                }
            }

            let buf = build_content_stream(page.height, &page.commands, alphas, gs_names);
            pdf.stream(content_id, buf.as_slice());
        }

        // --- Link annotations with GoTo support for internal anchors ---
        for (i, page) in doc.pages.iter().enumerate() {
            let specs = &page_link_specs[i];
            let refs = &page_link_annots[i];
            for (spec, &annot_id) in specs.iter().zip(refs.iter()) {
                let pdf_y = page.height - spec.y_topdown;
                let descent = spec.font_size * 0.22;
                let ascent = spec.font_size * 0.78;
                let rect = Rect::new(
                    spec.x,
                    pdf_y - descent,
                    spec.x + spec.width,
                    pdf_y + ascent,
                );
                let mut ann = pdf.annotation(annot_id);
                ann.subtype(AnnotationType::Link);
                ann.rect(rect);
                ann.contents(TextStr(spec.uri.as_str()));

                // Check if this is an internal anchor link (#anchor_id)
                if spec.uri.starts_with('#') {
                    let anchor_id = &spec.uri[1..];
                    if let Some(pos) = anchor_positions.get(anchor_id) {
                        // Internal link: emit GoTo action targeting the anchor.
                        // Get the target page (0-based for pdf-writer)
                        let target_page_idx = (pos.page_index - 1) as usize;
                        if target_page_idx < page_ids.len() {
                            ann.action()
                                .action_type(ActionType::GoTo)
                                .destination()
                                .page(page_ids[target_page_idx])
                                .xyz(0.0, page_height, None);
                        } else {
                            // Fallback: treat as external URI if anchor not found on a valid page.
                            ann.action()
                                .action_type(ActionType::Uri)
                                .uri(Str(spec.uri.as_bytes()));
                        }
                    } else {
                        // Anchor not resolved: fallback to URI action.
                        ann.action()
                            .action_type(ActionType::Uri)
                            .uri(Str(spec.uri.as_bytes()));
                    }
                } else {
                    // External link: emit URI action as before.
                    ann.action()
                        .action_type(ActionType::Uri)
                        .uri(Str(spec.uri.as_bytes()));
                }
                ann.finish();
            }
        }

        pdf.finish()
    }
}

fn collect_unique_opacity_alphas(commands: &[PainterCommand]) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for cmd in commands {
        if let PainterCommand::PushOpacity { alpha } = cmd {
            let a = alpha.clamp(0.0, 1.0);
            if a.is_finite() && !out.iter().any(|x| (x - a).abs() < 1e-4) {
                out.push(a);
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn opacity_gs_index(alphas: &[f32], alpha: f32) -> usize {
    let a = alpha.clamp(0.0, 1.0);
    alphas
        .iter()
        .position(|x| (x - a).abs() < 1e-4)
        .expect("PushOpacity alpha must have matching ExtGState")
}

// ─── Content stream ───────────────────────────────────────────────────────────

fn build_content_stream(
    page_height: f32,
    commands: &[PainterCommand],
    opacity_alphas: &[f32],
    opacity_names: &[Vec<u8>],
) -> pdf_writer::Buf {
    let mut content = Content::new();

    for cmd in commands {
        match cmd {
            PainterCommand::PushOpacity { alpha } => {
                let idx = opacity_gs_index(opacity_alphas, *alpha);
                content.save_state();
                content.set_parameters(Name(opacity_names[idx].as_slice()));
            }
            PainterCommand::PopOpacity => {
                content.restore_state();
            }
            PainterCommand::PushClipRect { x, y, w, h } => {
                let pdf_y = page_height - y - h;
                content.save_state();
                content.rect(*x, pdf_y, *w, *h);
                content.clip_nonzero();
                content.end_path();
            }
            PainterCommand::PopClip => {
                content.restore_state();
            }
            PainterCommand::Rect { x, y, w, h, fill, stroke, stroke_width } => {
                let pdf_y = page_height - y - h;

                if let Some(f) = fill {
                    content.set_fill_rgb(f.r, f.g, f.b);
                    content.rect(*x, pdf_y, *w, *h);
                    content.fill_nonzero();
                }
                if let Some(s) = stroke {
                    content.set_stroke_rgb(s.r, s.g, s.b);
                    content.set_line_width(*stroke_width);
                    content.rect(*x, pdf_y, *w, *h);
                    content.stroke();
                }
            }

            PainterCommand::Line { x1, y1, x2, y2, color, width } => {
                let pdf_y1 = page_height - y1;
                let pdf_y2 = page_height - y2;

                content.set_stroke_rgb(color.r, color.g, color.b);
                content.set_line_width(*width);
                content.move_to(*x1, pdf_y1);
                content.line_to(*x2, pdf_y2);
                content.stroke();
            }

            PainterCommand::Text {
                content: text,
                x,
                y,
                font_size,
                font_family,
                bold,
                italic,
                color,
                link_uri: _,
                link_width_pt: _,
            } => {
                // Must emit space-only runs: layout positions words using explicit x, but skipping
                // whitespace here removed all space glyphs and made adjacent words touch.
                if text.is_empty() {
                    continue;
                }

                // Y: our y is top-down baseline; PDF y is bottom-up baseline
                let pdf_y = page_height - y;
                let font_name = pdf_font_resource_name(*bold, *italic, font_family);

                content.set_fill_rgb(color.r, color.g, color.b);
                content.begin_text();
                content.set_font(Name(font_name), *font_size);
                content.next_line(*x, pdf_y);
                content.show(Str(encode_latin1(text).as_slice()));
                content.end_text();
            }
        }
    }

    content.finish()
}

// --- Utilities ---

fn is_monospace_family(family: &str) -> bool {
    let l = family.to_ascii_lowercase();
    l.contains("courier") || l.contains("mono")
}

/// Resource name embedded in the content stream (F1…F8).
fn pdf_font_resource_name(bold: bool, italic: bool, font_family: &str) -> &'static [u8] {
    match (is_monospace_family(font_family), bold, italic) {
        (false, false, false) => b"F1",
        (false, true, false) => b"F2",
        (false, false, true) => b"F3",
        (false, true, true) => b"F4",
        (true, false, false) => b"F5",
        (true, true, false) => b"F6",
        (true, false, true) => b"F7",
        (true, true, true) => b"F8",
    }
}

/// Encode UTF-8 → WinAnsiEncoding (cp1252).
/// Standard Latin-1 (0x00–0xFF) is preserved as-is.
/// WinAnsi characters in 0x80–0x9F are mapped explicitly.
/// Everything else becomes '?'.
/// Limitation of built-in Type1 fonts; TrueType + ToUnicode is planned for v3.
fn encode_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| match c as u32 {
        code @ (0x00..=0x7F | 0xA0..=0xFF) => code as u8,
        0x20AC => 0x80, // € EURO SIGN
        0x201A => 0x82, // ‚ SINGLE LOW-9 QUOTATION MARK
        0x0192 => 0x83, // ƒ LATIN SMALL LETTER F WITH HOOK
        0x201E => 0x84, // „ DOUBLE LOW-9 QUOTATION MARK
        0x2026 => 0x85, // … HORIZONTAL ELLIPSIS
        0x2020 => 0x86, // † DAGGER
        0x2021 => 0x87, // ‡ DOUBLE DAGGER
        0x02C6 => 0x88, // ˆ MODIFIER LETTER CIRCUMFLEX ACCENT
        0x2030 => 0x89, // ‰ PER MILLE SIGN
        0x0160 => 0x8A, // Š LATIN CAPITAL LETTER S WITH CARON
        0x2039 => 0x8B, // ‹ SINGLE LEFT-POINTING ANGLE QUOTATION MARK
        0x0152 => 0x8C, // Œ LATIN CAPITAL LIGATURE OE
        0x017D => 0x8E, // Ž LATIN CAPITAL LETTER Z WITH CARON
        0x2022 => 0x95, // • BULLET
        0x2013 => 0x96, // – EN DASH
        0x2014 => 0x97, // — EM DASH
        0x02DC => 0x98, // ˜ SMALL TILDE
        0x2122 => 0x99, // ™ TRADE MARK SIGN
        0x0161 => 0x9A, // š LATIN SMALL LETTER S WITH CARON
        0x203A => 0x9B, // › SINGLE RIGHT-POINTING ANGLE QUOTATION MARK
        0x0153 => 0x9C, // œ LATIN SMALL LIGATURE OE
        0x017E => 0x9E, // ž LATIN SMALL LETTER Z WITH CARON
        0x0178 => 0x9F, // Ÿ LATIN CAPITAL LETTER Y WITH DIAERESIS
        0x201C => 0x93, // " LEFT DOUBLE QUOTATION MARK
        0x201D => 0x94, // " RIGHT DOUBLE QUOTATION MARK
        0x2018 => 0x91, // ' LEFT SINGLE QUOTATION MARK
        0x2019 => 0x92, // ' RIGHT SINGLE QUOTATION MARK
        _ => b'?',
    }).collect()
}

struct RefAlloc(i32);

impl RefAlloc {
    fn new(start: i32) -> Self { Self(start) }
    fn next(&mut self) -> Ref {
        let r = Ref::new(self.0);
        self.0 += 1;
        r
    }
}
