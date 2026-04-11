//! Integration checks for Document → layout → pagination.

use crate::engine::layout::compute_layout;
use crate::engine::paginate::{paginate, DrawCommand};
use crate::engine::resolver::build_styled_tree;
use crate::engine::styles::BoxKind;
use crate::lexer::Lexer;
use crate::parser::{self, Parser};

fn load_fol(src: &str) -> crate::parser::ast::Document {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let doc = parser.parse().expect("parse");
    let doc = parser::resolver::resolve(doc);
    parser::id::assign_ids(doc)
}

/// Each top-level `PAGE` must begin on a new physical page when the prior
/// page still has room (short content). Regression: merged PAGE blocks on one PDF page.
#[test]
fn five_page_blocks_yield_five_physical_pages() {
    let mut fol = String::new();
    for i in 0..5 {
        fol.push_str(&format!("PAGE(P(Page {i} short.))\n"));
    }
    let doc = load_fol(&fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    assert_eq!(
        layout.roots.len(),
        5,
        "fixture must produce one layout root per PAGE block"
    );
    let pages = paginate(&layout, &styled);
    assert_eq!(
        pages.pages.len(),
        5,
        "each PAGE block must start a new physical page"
    );
}

/// GRID с `columns: "1fr 2fr"` даёт ширины ячеек в соотношении ~1:2 (taffy + extract_layout).
#[test]
fn grid_1fr_2fr_column_width_ratio() {
    let fol = r#"
PAGE(
  GRID({columns: "1fr 2fr"}
    P(Left)
    P(Right)
  )
)
"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);

    let mut paragraphs: Vec<(f32, f32)> = layout
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, BoxKind::Paragraph))
        .map(|n| (n.width, n.x))
        .collect();
    assert_eq!(paragraphs.len(), 2, "expected two P children");
    paragraphs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let w_left = paragraphs[0].0;
    let w_right = paragraphs[1].0;
    assert!(
        w_left > 10.0 && w_right > 10.0,
        "sane widths: left={w_left} right={w_right}"
    );
    let ratio = w_left / w_right;
    assert!(
        (ratio - 0.5).abs() < 0.08,
        "expected width ratio w_left/w_right ≈ 0.5 for 1fr:2fr, got {ratio} (left={w_left} right={w_right})"
    );

    let _ = paginate(&layout, &styled);
}

/// Empty `IMAGE` / `FIGURE` leaf: engine draws a visible placeholder until raster decode exists.
#[test]
fn empty_image_yields_placeholder_rect_in_page_tree() {
    let fol = r#"PAGE(IMAGE({width: 40}))"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let pages = paginate(&layout, &styled);
    let placeholders = pages
        .pages
        .iter()
        .flat_map(|p| &p.commands)
        .filter(|c| matches!(c, DrawCommand::Rect { fill: Some(_), stroke: Some(_), .. }))
        .count();
    assert!(
        placeholders >= 1,
        "expected at least one stroked filled rect as image placeholder"
    );
}
