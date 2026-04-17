//! Integration checks for Document → layout → pagination.

use crate::engine::arena::{DocumentArena, NodeId};
use crate::engine::grid_tracks::GridColumnTrack;
use crate::engine::layout::{compute_layout, LayoutContent, LayoutNodeIdx, LayoutTree};
use crate::engine::paginate::{paginate, DrawCommand};
use crate::engine::resolver::build_styled_tree;
use crate::engine::{render, ExportFormat, ExportOptions};
use crate::engine::styles::{BoxContent, BoxKind, StyledBox};
use crate::lexer::Lexer;
use crate::parser::{self, Parser};

fn find_first_grid(styled: &DocumentArena, id: NodeId) -> Option<&StyledBox> {
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

/// Narrow fixed column + long `P`: taffy leaf measure must wrap text so `LayoutBox` height > one line.
#[test]
fn grid_narrow_column_multiline_paragraph_measured_height() {
    let fol = r#"
PAGE(
  GRID({columns: "40pt 1fr"}
    P(Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa Lambda Mu)
    P(X)
  )
)
"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);

    let narrow_heights: Vec<f32> = layout
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, BoxKind::Paragraph) && n.width < 55.0)
        .map(|n| n.height)
        .collect();

    assert!(
        narrow_heights.iter().any(|&h| h > 18.0),
        "expected wrapped paragraph height > one line (~13pt), got {:?}",
        narrow_heights
    );

    let _ = paginate(&layout, &styled);
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
    // Sec numbers are rendered as their own per-fragment text elements alongside the body.
    assert!(s.contains(">1 Alpha") || s.contains(">1<") || s.contains(">1 "), "sec=1 missing: {}", &s[..s.len().min(500)]);
    assert!(s.contains(">2 Beta") || s.contains(">2<") || s.contains(">2 "), "sec=2 missing");
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

/// PDF export uses the same layout pipeline as native preview; header must be stable.
#[test]
fn pipeline_pdf_starts_with_magic_bytes() {
    let fol = r#"PAGE(P(Hello PDF))"#;
    let doc = load_fol(fol);
    let bytes = render(
        &doc,
        ExportOptions {
            format: ExportFormat::Pdf,
        },
    );
    assert!(
        bytes.len() >= 5 && &bytes[..5] == b"%PDF-",
        "expected PDF header %%-, got {:?}",
        bytes.get(..12.min(bytes.len()))
    );
}

/// Inline `[text](url)` must produce a PDF link annotation (clickable in viewers), not only blue text.
#[test]
fn pipeline_pdf_inline_link_emits_uri_annotation() {
    let fol = r#"PAGE(P(Visit [site](https://example.com/path) now.))"#;
    let doc = load_fol(fol);
    let bytes = render(
        &doc,
        ExportOptions {
            format: ExportFormat::Pdf,
        },
    );
    assert!(
        bytes.windows(5).any(|w| w == b"/URI "),
        "expected /URI action in PDF, len={}",
        bytes.len()
    );
    assert!(
        bytes.windows(5).any(|w| w == b"/Link"),
        "expected Link annotation subtype"
    );
}

#[test]
fn table_row_cell_boxes_share_top_y() {
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(P(A))
          CELL(P(BB))
          CELL(P(Longer text in middle))
          CELL(P(D))
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    fn collect_cells(layout: &LayoutTree, idx: LayoutNodeIdx, out: &mut Vec<f32>) {
        let n = &layout.nodes[idx];
        if matches!(n.kind, BoxKind::Cell) {
            out.push(n.y);
        }
        if let LayoutContent::Children(ch) = &n.content {
            for &c in ch {
                collect_cells(layout, c, out);
            }
        }
    }
    let mut ys = Vec::new();
    collect_cells(&layout, layout.roots[0], &mut ys);
    assert!(
        ys.len() >= 4,
        "expected4 cells, ys={ys:?}"
    );
    let min = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (max - min).abs() < 0.5,
        "cells in one row should share same layout y; got {ys:?}"
    );
}

#[test]
fn table_cell_paragraph_on_second_fol_paint_y_is_page_local() {
    // First PAGE: many blocks so `compute_layout` stacks a large `offset_y` before the next root.
    // Regression: table CELL(P(...)) used raw layout `child.y` (document-absolute) as `cursor_y`,
    // placing text far below the page when a prior fol was tall.
    let mut src = String::from("PAGE(");
    for _ in 0..45 {
        src.push_str("P(Line of filler to grow first fol layout height.)\n");
    }
    src.push_str(")\nPAGE(TABLE(ROW(CELL(P(CellA)) CELL(P(CellB)))))");
    let doc = load_fol(&src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let last_page = tree.pages.last().expect("at least one page");
    let mut hits: Vec<f32> = Vec::new();
    for cmd in &last_page.commands {
        if let DrawCommand::Text { y, content, .. } = cmd
            && (content == "CellA" || content == "CellB") {
                hits.push(*y);
            }
    }
    assert!(
        hits.len() >= 2,
        "expected both cell labels painted on last page, got {:?}",
        hits
    );
    for y in &hits {
        assert!(
            *y > 25.0 && *y < 820.0,
            "cell paragraph must paint inside A4 content band, got y={}",
            y
        );
    }
}

/// Across rows, paragraphs in the same column must render at the same x.
/// Regression: taffy runs flex layout per ROW, giving different cell widths per
/// row when content differs. `place_table` normalizes column widths across rows
/// but fed cell Children through `place_node`, which read the per-row taffy x —
/// so col 2 text landed at a different x on each row.
#[test]
fn table_cell_children_x_aligns_across_rows_within_column() {
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(P(A1))
          CELL(P(col-two-row-one-short))
        )
        ROW(
          CELL(P(A2 much longer content to shift flex basis for column zero))
          CELL(P(col-two-row-two-short))
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);

    let page = tree.pages.first().expect("one page");
    let find_x = |needle: &str| -> Option<f32> {
        for cmd in &page.commands {
            if let DrawCommand::Text { content, x, .. } = cmd
                && content.contains(needle) {
                    return Some(*x);
                }
        }
        None
    };

    let row1_col2 = find_x("col-two-row-one-short").expect("row1 col2 text");
    let row2_col2 = find_x("col-two-row-two-short").expect("row2 col2 text");
    assert!(
        (row1_col2 - row2_col2).abs() < 0.5,
        "col 2 x must be identical across rows; row1={row1_col2}, row2={row2_col2}"
    );

    let row1_col1 = find_x("A1").expect("row1 col1 text");
    let row2_col1 = find_x("A2").expect("row2 col1 text");
    assert!(
        (row1_col1 - row2_col1).abs() < 0.5,
        "col 1 x must be identical across rows; row1={row1_col1}, row2={row2_col1}"
    );
}

#[test]
fn table_row_paragraph_boxes_share_top_y_within_row() {
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(P(A))
          CELL(P(BB))
          CELL(P(Longer text in middle))
          CELL(P(D))
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    fn collect_p_in_table(layout: &LayoutTree, idx: LayoutNodeIdx, out: &mut Vec<f32>) {
        let n = &layout.nodes[idx];
        if matches!(n.kind, BoxKind::Paragraph) {
            out.push(n.y);
        }
        if let LayoutContent::Children(ch) = &n.content {
            for &c in ch {
                collect_p_in_table(layout, c, out);
            }
        }
    }
    let mut ys = Vec::new();
    collect_p_in_table(&layout, layout.roots[0], &mut ys);
    assert!(ys.len() >= 4, "expected4 P nodes, ys={ys:?}");
    let min = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (max - min).abs() < 0.5,
        "paragraphs in one table row should share same y; got {ys:?}"
    );
}

/// Collect every `DrawCommand::Text` on the first page as `(content, x, y)`.
fn page1_texts(tree: &crate::engine::paginate::PageTree) -> Vec<(String, f32, f32)> {
    let page = tree.pages.first().expect("one page");
    let mut out = Vec::new();
    for cmd in &page.commands {
        if let DrawCommand::Text { content, x, y, .. } = cmd {
            out.push((content.clone(), *x, *y));
        }
    }
    out
}

/// `{truncate: true}` on a CELL clips overflowing content to one line with
/// an ellipsis. Inline content is rendered one DrawCommand::Text per fragment,
/// so we detect truncation by (a) every fragment sharing the same baseline and
/// (b) the tail fragment ending in `…`.
#[test]
fn cell_truncate_clips_text_with_ellipsis() {
    // Two-column table → col width ≈ 255pt. Long content guarantees overflow.
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(short)
          CELL({truncate: true} this is a rather long sentence that absolutely must not fit inside a narrow cell)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);
    // Anchor fragment: the first word of the cell guarantees we hit that cell.
    let anchor_y = texts
        .iter()
        .find(|(t, _, _)| t.starts_with("this"))
        .map(|(_, _, y)| *y)
        .expect("anchor fragment present");
    let same_line: Vec<_> = texts
        .iter()
        .filter(|(t, _, y)| (y - anchor_y).abs() < 0.5 && !t.trim().is_empty())
        .collect();
    // All non-empty fragments on the anchor baseline belong to the truncated cell
    // and the sibling "short" cell. Pick the rightmost one; must end with the
    // ellipsis appended by `truncate_inline_line`.
    let rightmost = same_line
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .expect("rightmost fragment");
    assert!(
        rightmost.0.ends_with('…'),
        "truncate should append ellipsis on tail fragment; got {:?} (all={:?})",
        rightmost.0,
        same_line
    );
}

/// `{nowrap: true}` keeps the entire cell content on a single baseline even
/// when its total width would overflow the cell. Fragments are emitted
/// per-word (break_inline_runs), so we assert on distinct y count = 1.
#[test]
fn cell_nowrap_emits_single_line() {
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(short)
          CELL({nowrap: true} this sentence is definitely longer than a single narrow cell can hold at the default font size)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);
    let anchor_y = texts
        .iter()
        .find(|(t, _, _)| t.starts_with("this"))
        .map(|(_, _, y)| *y)
        .expect("anchor fragment");
    // All fragments on a y close to anchor_y belong to the cell baseline. The
    // 'nowrap' cell must contribute its entire content on that single baseline,
    // so we look for a late-content word ("definitely") on the same y.
    let found_late = texts.iter().any(|(t, _, y)| {
        (*y - anchor_y).abs() < 0.5 && t.contains("definitely")
    });
    assert!(
        found_late,
        "nowrap must keep all content on one baseline; texts={:?}",
        texts
    );
    // No fragment starting with 'this' on a lower baseline (a would-be second line).
    let has_wrap = texts.iter().any(|(t, _, y)| {
        *y > anchor_y + 1.0 && t.contains("font")
    });
    assert!(!has_wrap, "nowrap produced additional wrapped line");
}

/// Explicit `{align: center}` on a CELL shifts the text x so that the line is
/// centered inside the cell's inner width.
#[test]
fn cell_center_align_offsets_x() {
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL({align: left} L)
          CELL({align: center} C)
          CELL({align: right} R)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);

    let x_of = |needle: &str| {
        texts
            .iter()
            .find(|(t, _, _)| t == needle)
            .map(|(_, x, _)| *x)
            .expect("cell text")
    };
    let xl = x_of("L");
    let xc = x_of("C");
    let xr = x_of("R");
    // Left-aligned starts closest to margin; right-aligned x largest.
    assert!(xl < xc, "center must be right of left; L={xl} C={xc}");
    assert!(xc < xr, "right must be right of center; C={xc} R={xr}");
}

/// TABLE `{align: "left,center,right"}` sets per-column fallbacks. Cells with
/// no explicit `align` inherit the per-column alignment.
#[test]
fn table_col_aligns_fallback_applies_when_cell_has_no_explicit_align() {
    let src = r#"PAGE(
      TABLE({align: "left,center,right"}
        ROW(
          CELL(L)
          CELL(C)
          CELL(R)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);
    let x_of = |needle: &str| {
        texts
            .iter()
            .find(|(t, _, _)| t == needle)
            .map(|(_, x, _)| *x)
            .expect("cell text")
    };
    let xl = x_of("L");
    let xc = x_of("C");
    let xr = x_of("R");
    assert!(xl < xc, "col-align center must shift right; L={xl} C={xc}");
    assert!(xc < xr, "col-align right must be rightmost; C={xc} R={xr}");
}

/// Explicit cell `align` beats TABLE `col_aligns` for that cell.
#[test]
fn cell_align_overrides_table_col_align() {
    let src = r#"PAGE(
      TABLE({align: "right,right"}
        ROW(
          CELL({align: left} X)
          CELL(Y)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);
    let x_of = |needle: &str| texts.iter().find(|(t, _, _)| t == needle).unwrap().1;
    // X is left-aligned in col 0 → smaller x than Y right-aligned in col 1.
    // Also: X must sit near the left edge of col 0, not at the right of col 0.
    let xx = x_of("X");
    let xy = x_of("Y");
    assert!(xx < xy, "explicit left on X must win over col-right; xx={xx} xy={xy}");
}

/// `{span: 2}` consumes two columns. The second painted cell therefore starts
/// at x = col0 + col1, not at x = col0.
#[test]
fn cell_colspan_2_advances_x_by_two_columns() {
    // Three-column row (equal split) used as reference, then a row with span:2.
    let src = r#"PAGE(
      TABLE(
        ROW(
          CELL(a)
          CELL(b)
          CELL(c)
        )
        ROW(
          CELL({span: 2} wide)
          CELL(tail)
        )
      )
    )"#;
    let doc = load_fol(src);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let tree = paginate(&layout, &styled);
    let texts = page1_texts(&tree);
    let x_of = |needle: &str| texts.iter().find(|(t, _, _)| t == needle).unwrap().1;
    let x_c = x_of("c");
    let x_tail = x_of("tail");
    assert!(
        (x_c - x_tail).abs() < 0.5,
        "col 3 (tail after colspan 2) must share x with reference col c; c={x_c} tail={x_tail}"
    );
    let x_a = x_of("a");
    let x_wide = x_of("wide");
    assert!(
        (x_a - x_wide).abs() < 0.5,
        "spanned cell starts at col 0; a={x_a} wide={x_wide}"
    );
}

/// `{valign: bottom}` pushes cell text to the bottom of the row when another
/// cell forces the row to be taller.
#[test]
fn cell_valign_bottom_shifts_text_downward() {
    // Cell A wraps to many lines (inflates row height); cell B has a single line
    // with valign:bottom. Compare against a control with valign top.
    let mk = |valign: &str| -> f32 {
        let src = format!(
            r#"PAGE(
              TABLE(
                ROW(
                  CELL(line one. line two. line three. line four. line five. line six. line seven. line eight. line nine. line ten.)
                  CELL({{valign: {valign}}} B)
                )
              )
            )"#
        );
        let doc = load_fol(&src);
        let styled = build_styled_tree(&doc);
        let layout = compute_layout(&styled);
        let tree = paginate(&layout, &styled);
        let texts = page1_texts(&tree);
        texts
            .iter()
            .find(|(t, _, _)| t == "B")
            .map(|(_, _, y)| *y)
            .expect("B baseline")
    };
    let y_top = mk("top");
    let y_bottom = mk("bottom");
    assert!(
        y_bottom > y_top + 5.0,
        "valign=bottom must be below valign=top by at least a few lines; top={y_top} bottom={y_bottom}"
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
    // Page number is rendered inline with paragraph text (single text element per line)
    assert!(
        s.contains("page 1.") || s.contains(">1.</text>"),
        "expected substituted page digit in SVG, got: {}",
        &s[..s.len().min(1200)]
    );
}

#[test]
fn pipeline_anchor_and_internal_link_renders_goto_and_uri() {
    // Create a document with an internal anchor link and an external link
    let fol = r#"PAGE(H1[start](Heading) P(Go to [section](#start) or [web](https://example.com)))"#;
    let doc = load_fol(fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    let page_tree = paginate(&layout, &styled);

    // Verify that blocks with anchor IDs still record their start page
    // (anchors are mapped via block IDs, not directly in paginate)
    assert!(page_tree.block_start_page.contains_key("start"));

    // Render to PDF
    let bytes = render(&doc, ExportOptions { format: ExportFormat::Pdf });
    assert!(bytes.starts_with(b"%PDF"));

    // Verify link annotations are present
    let pdf_str = String::from_utf8_lossy(&bytes);
    assert!(pdf_str.contains("/Annot"));

    // Verify external links still emit URI actions
    assert!(pdf_str.contains("/URI"));
}
