/// PDF Renderer v1 (legacy) — простой курсорный рендерер на printpdf 0.8.
///
/// Этот модуль сохранён для обратной совместимости команды `convert --format pdf`.
/// Для production-quality вывода используй `render` (Engine v2).

use crate::parser::ast::{Block, Content, Document};
use printpdf::{BuiltinFont, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, Point, TextItem};

struct Cursor {
    x: f64,
    y: f64,
}

pub fn render(doc: &Document) -> Result<Vec<u8>, String> {
    let mut pdf = PdfDocument::new("Folio Document");
    let mut ops: Vec<Op> = Vec::new();
    let mut cursor = Cursor { x: 20.0, y: 277.0 };

    // Рендерим все блоки в один поток Op
    for block in &doc.blocks {
        render_block(block, &mut ops, &mut cursor);
    }

    pdf.pages.push(PdfPage::new(
        Mm(210.0),
        Mm(297.0),
        ops,
    ));

    let mut warnings = Vec::new();
    let bytes = pdf.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

fn render_block(block: &Block, ops: &mut Vec<Op>, cursor: &mut Cursor) {
    match block.kind.as_str() {
        "PAGE" => {
            if let Content::Blocks(blocks) = &block.content {
                for child in blocks {
                    render_block(child, ops, cursor);
                }
            }
        }
        "H1" => {
            let text = extract_text(block);
            cursor.y -= 10.0;
            write_text(ops, &text, cursor, BuiltinFont::HelveticaBold, 24.0);
            cursor.y -= 8.0;
        }
        "H2" => {
            let text = extract_text(block);
            cursor.y -= 8.0;
            write_text(ops, &text, cursor, BuiltinFont::HelveticaBold, 18.0);
            cursor.y -= 6.0;
        }
        "TABLE" | "GRID" => {
            if let Content::Blocks(rows) = &block.content {
                cursor.y -= 6.0;
                for row in rows {
                    if let Content::Blocks(cells) = &row.content {
                        let cell_width = 170.0 / (cells.len() as f64).max(1.0);
                        let mut x = cursor.x;
                        for cell in cells {
                            let text = extract_text(cell);
                            let old_x = cursor.x;
                            cursor.x = x;
                            write_text(ops, &text, cursor, BuiltinFont::Helvetica, 10.0);
                            cursor.x = old_x;
                            x += cell_width;
                        }
                        cursor.y -= 6.0;
                    }
                }
                cursor.y -= 4.0;
            }
        }
        "STYLES" => {}
        _ => {
            let text = extract_text(block);
            if text.is_empty() {
                return;
            }
            let lines = textwrap::wrap(&text, 80);
            for line in lines {
                write_text(ops, &line, cursor, BuiltinFont::Helvetica, 12.0);
                cursor.y -= 6.0;
            }
            cursor.y -= 4.0;
        }
    }
}

fn write_text(ops: &mut Vec<Op>, text: &str, cursor: &Cursor, font: BuiltinFont, size: f64) {
    if text.trim().is_empty() {
        return;
    }
    ops.push(Op::StartTextSection);
    ops.push(Op::SetFontSizeBuiltinFont {
        font,
        size: Pt(size as f32),
    });
    ops.push(Op::SetTextCursor {
        pos: Point {
            x: Pt(cursor.x as f32 * 2.8346),
            y: Pt(cursor.y as f32 * 2.8346),
        },
    });
    ops.push(Op::WriteTextBuiltinFont {
        font,
        items: vec![TextItem::Text(text.to_string())],
    });
    ops.push(Op::EndTextSection);
}

fn extract_text(block: &Block) -> String {
    match &block.content {
        Content::Text(s) => s.clone(),
        Content::Empty => String::new(),
        Content::Blocks(b) => {
            if b.len() == 1 && b[0].kind == "CELL" {
                extract_text(&b[0])
            } else {
                String::new()
            }
        }
    }
}
