/// PDF Backend (Фаза 4)
///
/// Принимает PageTree с DrawCommand-ами и генерирует байты PDF-файла.
/// Использует крейт `pdf-writer` — низкоуровневый, но быстрый.
///
/// Поддерживаемые команды:
/// - DrawCommand::Text  → PDF BT ... ET (встроенные шрифты: Helvetica / Helvetica-Bold)
/// - DrawCommand::Rect  → PDF re + f/S
/// - DrawCommand::Line  → PDF m + l + S
///
/// Система координат:
/// pdf-writer: левый нижний угол = (0,0), Y растёт вверх.
/// PageTree:   левый верхний угол = (0,0), Y растёт вниз.
/// Конвертация: pdf_y = page_height - our_y

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};
use crate::engine::paginate::{DrawCommand, PageTree};

pub fn render(page_tree: &PageTree) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut alloc = RefAlloc::new(1);

    let catalog_id      = alloc.next();
    let pages_id        = alloc.next();
    let font_regular_id = alloc.next();
    let font_bold_id    = alloc.next();

    let mut page_ids:    Vec<Ref> = Vec::new();
    let mut content_ids: Vec<Ref> = Vec::new();

    for _ in &page_tree.pages {
        page_ids.push(alloc.next());
        content_ids.push(alloc.next());
    }

    // ─── Catalog ──────────────────────────────────────────────────────────────
    pdf.catalog(catalog_id).pages(pages_id);

    // ─── Page tree ────────────────────────────────────────────────────────────
    let page_width  = page_tree.pages.first().map(|p| p.width).unwrap_or(595.28);
    let page_height = page_tree.pages.first().map(|p| p.height).unwrap_or(841.89);
    {
        let mut pages = pdf.pages(pages_id);
        pages.media_box(Rect::new(0.0, 0.0, page_width, page_height));
        pages.kids(page_ids.iter().copied());
        pages.count(page_ids.len() as i32);
    }

    // ─── Шрифты (встроенные Type1) ────────────────────────────────────────────
    pdf.type1_font(font_regular_id).base_font(Name(b"Helvetica"));
    pdf.type1_font(font_bold_id).base_font(Name(b"Helvetica-Bold"));

    // ─── Страницы ─────────────────────────────────────────────────────────────
    for (i, page) in page_tree.pages.iter().enumerate() {
        let page_id    = page_ids[i];
        let content_id = content_ids[i];

        {
            let mut p = pdf.page(page_id);
            p.media_box(Rect::new(0.0, 0.0, page.width, page.height));
            p.parent(pages_id);
            p.contents(content_id);
            let mut res = p.resources();
            let mut fonts = res.fonts();
            fonts.pair(Name(b"F1"), font_regular_id);
            fonts.pair(Name(b"F2"), font_bold_id);
        }

        let buf = build_content_stream(page.height, &page.commands);
        pdf.stream(content_id, buf.as_slice());
    }

    pdf.finish()
}

// ─── Content stream ───────────────────────────────────────────────────────────

fn build_content_stream(page_height: f32, commands: &[DrawCommand]) -> pdf_writer::Buf {
    let mut content = Content::new();

    for cmd in commands {
        match cmd {
            DrawCommand::Rect { x, y, w, h, fill, stroke, stroke_width } => {
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

            DrawCommand::Line { x1, y1, x2, y2, color, width } => {
                let pdf_y1 = page_height - y1;
                let pdf_y2 = page_height - y2;

                content.set_stroke_rgb(color.r, color.g, color.b);
                content.set_line_width(*width);
                content.move_to(*x1, pdf_y1);
                content.line_to(*x2, pdf_y2);
                content.stroke();
            }

            DrawCommand::Text { content: text, x, y, font_size, bold, color, .. } => {
                if text.trim().is_empty() {
                    continue;
                }

                // Y: наш y — top-down baseline, PDF y — bottom-up baseline
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

// ─── Утилиты ──────────────────────────────────────────────────────────────────

/// Кодирует UTF-8 → WinAnsiEncoding (cp1252).
/// Стандартный Latin-1 (0x00–0xFF) сохраняется as-is.
/// Символы WinAnsi из диапазона 0x80–0x9F маппятся явно.
/// Всё остальное заменяется на '?'.
/// Ограничение built-in Type1 шрифтов; TrueType + ToUnicode — v3.
fn encode_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| match c as u32 {
        code @ 0..=255 => code as u8,
        0x2022 => 0x95, // • BULLET
        0x2013 => 0x96, // – EN DASH
        0x2014 => 0x97, // — EM DASH
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
