/// PDF backend (phase 4)
///
/// Takes a `PageTree` with draw commands and produces PDF file bytes.
/// Uses the `pdf-writer` crate: low-level but fast.
///
/// Supported commands:
/// - `Text`  → PDF BT ... ET (built-in fonts: Helvetica / Helvetica-Bold)
/// - `Rect`  → PDF re + f/S
/// - `Line`  → PDF m + l + S
/// - `PushOpacity` / `PopOpacity` → `q` / `Q` + `ExtGState` (`ca` / `CA`)
/// - `PushClipRect` / `PopClip` → `q` / `Q` + `W n` clip path
///
/// Coordinate systems:
/// pdf-writer: bottom-left = (0,0), Y increases upward.
/// PageTree:   top-left = (0,0), Y increases downward.
/// Conversion: `pdf_y = page_height - our_y`

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};
use crate::engine::paginate::PageTree;
use super::painter::{from_page_tree, PaintDocument, PainterBackend, PainterCommand};

pub fn render(page_tree: &PageTree) -> Vec<u8> {
    let doc = from_page_tree(page_tree);
    PdfBackend.render_document(&doc)
}

pub struct PdfBackend;

impl PainterBackend for PdfBackend {
    fn render_document(&self, doc: &PaintDocument) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let mut alloc = RefAlloc::new(1);

        let catalog_id = alloc.next();
        let pages_id = alloc.next();
        let font_regular_id = alloc.next();
        let font_bold_id = alloc.next();

        let mut page_ids: Vec<Ref> = Vec::new();
        let mut content_ids: Vec<Ref> = Vec::new();

        for _ in &doc.pages {
            page_ids.push(alloc.next());
            content_ids.push(alloc.next());
        }

        // Per-page opacity → ExtGState objects (refs allocated before page bodies).
        let mut page_opacity: Vec<(Vec<f32>, Vec<Ref>, Vec<Vec<u8>>)> = Vec::with_capacity(doc.pages.len());
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

        // --- Built-in Type1 fonts ---
        pdf.type1_font(font_regular_id)
            .base_font(Name(b"Helvetica"))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
        pdf.type1_font(font_bold_id)
            .base_font(Name(b"Helvetica-Bold"))
            .encoding_predefined(Name(b"WinAnsiEncoding"));

        // --- Pages ---
        for (i, page) in doc.pages.iter().enumerate() {
            let page_id = page_ids[i];
            let content_id = content_ids[i];
            let (alphas, gs_refs, gs_names) = &page_opacity[i];

            {
                let mut p = pdf.page(page_id);
                p.media_box(Rect::new(0.0, 0.0, page.width, page.height));
                p.parent(pages_id);
                p.contents(content_id);
                let mut res = p.resources();
                {
                    let mut fonts = res.fonts();
                    fonts.pair(Name(b"F1"), font_regular_id);
                    fonts.pair(Name(b"F2"), font_bold_id);
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

        pdf.finish()
    }
}

fn collect_unique_opacity_alphas(commands: &[PainterCommand]) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for cmd in commands {
        if let PainterCommand::PushOpacity { alpha } = cmd {
            let a = alpha.clamp(0.0, 1.0);
            if !out.iter().any(|x| (x - a).abs() < 1e-4) {
                out.push(a);
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
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

            PainterCommand::Text { content: text, x, y, font_size, bold, color, .. } => {
                if text.trim().is_empty() {
                    continue;
                }

                // Y: our y is top-down baseline; PDF y is bottom-up baseline
                let pdf_y = page_height - y;
                let font_name: &[u8] = if *bold { b"F2" } else { b"F1" };

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
