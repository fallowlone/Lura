//! Integration checks for Document → layout → pagination.

use crate::engine::arena::{DocumentArena, NodeId};
use crate::engine::grid_tracks::GridColumnTrack;
use crate::engine::layout::compute_layout;
use crate::engine::paginate::{paginate, DrawCommand};
use crate::engine::resolver::build_styled_tree;
use crate::engine::{render, ExportFormat, ExportOptions};
use crate::engine::styles::{BoxContent, BoxKind, StyledBox};
use crate::lexer::Lexer;
use crate::parser::{self, Parser};

fn find_first_grid<'a>(styled: &'a DocumentArena, id: NodeId) -> Option<&'a StyledBox> {
    let node = styled.get(id);
    if matches!(node.kind, BoxKind::Grid) {
        return Some(node);
    }
    if let BoxContent::Children(children) = &node.content {
        for &cid in children {
            if let Some(g) = find_first_grid(styled, cid) {
                return Some(g);
            }
        }
    }
    None
}

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

/// GRID with `columns: "1fr 2fr"` yields cell widths in roughly a 1:2 ratio (taffy + extract_layout).
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

/// Unquoted `columns: 2fr` is one `2fr` track, not two `1fr` columns.
#[test]
fn grid_unquoted_2fr_single_column_track() {
    let fol = r#"
PAGE(
  GRID({columns: 2fr}
    P(Solo)
  )
)
"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let root = *styled.roots.first().expect("root");
    let grid = find_first_grid(&styled, root).expect("GRID node");
    assert_eq!(
        grid.styles.grid_column_tracks,
        vec![GridColumnTrack::Fr(2.0)],
        "unquoted 2fr must be one 2fr track"
    );
    let layout = compute_layout(&styled);
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

/// `{{sec}}` in heading content is expanded to the outline number (`docs/SPEC.md`).
/// Leading `{` must be escaped as `\{` so the lexer does not parse attrs.
#[test]
fn pipeline_heading_sec_placeholder_svg() {
    let fol = r#"PAGE(H1(\{{sec}} Alpha) H1(\{{sec}} Beta))"#;
    let doc = load_fol(fol);
    let bytes = render(
        &doc,
        ExportOptions {
            format: ExportFormat::Svg,
        },
    );
    let s = String::from_utf8(bytes).expect("utf8 svg");
    assert!(s.contains("Alpha"), "heading body missing: {}", &s[..s.len().min(500)]);
    assert!(s.contains("Beta"));
    assert!(s.contains(">1</text>") || s.contains(">1 <"));
    assert!(s.contains(">2</text>") || s.contains(">2 <"));
}

/// `{{page:id}}` resolves to the 1-based start page of the target block.
#[test]
fn page_map_records_explicit_heading_id() {
    let fol = r#"PAGE(H1[target](Title) P(x))"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    assert!(
        tree.block_start_page.contains_key("target"),
        "map={:?}",
        tree.block_start_page
    );
}

#[test]
fn pipeline_page_placeholder_svg() {
    let fol = r#"PAGE(H1[target](Title) P(On page \{{page:target}}.))"#;
    let doc = load_fol(fol);
    let bytes = render(
        &doc,
        ExportOptions {
            format: ExportFormat::Svg,
        },
    );
    let s = String::from_utf8(bytes).expect("utf8 svg");
    assert!(
        s.contains(">1.<") || s.contains(">1</text>"),
        "expected substituted page digit in SVG, got: {}",
        &s[..s.len().min(1200)]
    );
}
