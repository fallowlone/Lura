/// Converts parser AST into a StyledTree (arena of StyledBoxes).
///
/// Per node: `ResolvedStyles::default()`, then inheritable fields from the parent,
/// then `apply_kind_defaults` (so H1/CODE defaults are not overwritten by inherited font),
/// then explicit block attrs.

use crate::parser::ast::{Block, Content, Document, InlineNode, NodeId as AstNodeId, Value};
use super::arena::DocumentArena;
use super::grid_tracks::parse_grid_columns_value;
use super::styles::{
    BoxContent, BoxKind, Color, Display, EdgeInsets, FloatMode, FontStyle, FontWeight, InlineRun,
    ListStyle, ResolvedStyles, StyledBox, TextAlign,
};

/// Main entry point.
/// Takes `Document` from the parser, returns `DocumentArena`.
pub fn build_styled_tree(doc: &Document) -> DocumentArena {
    let mut arena = DocumentArena::new();

    for (root, block) in doc.root_blocks() {
        if block.kind == "STYLES" {
            continue;
        }
        let node_id = convert_block(doc, root, None, &mut arena);
        arena.add_root(node_id);
    }

    arena
}

/// Recursively converts an AST block to a StyledBox in the arena.
/// `parent_styles` are the parent's resolved styles (for inheritance).
fn convert_block(
    doc: &Document,
    ast_node_id: AstNodeId,
    parent_styles: Option<&ResolvedStyles>,
    arena: &mut DocumentArena,
) -> super::arena::NodeId {
    let block = doc.block(ast_node_id);
    let kind = BoxKind::from_str(&block.kind);

    // Base + inheritable from parent (font-*, color, line-height, text-align).
    // Then kind defaults (H1 size/weight, CODE monospace, …) so inheritance does not erase them.
    let mut styles = ResolvedStyles::default();
    if let Some(parent) = parent_styles {
        inherit_styles(&mut styles, parent);
    }
    styles.apply_kind_defaults(&kind);

    // Apply explicit block attrs (override defaults and inheritance)
    apply_attrs(&mut styles, block);

    // Convert content
    let content = match &block.content {
        Content::Text(s) => BoxContent::Text(s.clone()),
        Content::Inline(nodes) => BoxContent::Inline(flatten_inline(nodes, false, false, None)),
        Content::Empty => BoxContent::Empty,
        Content::Children(children) => {
            // Allocate the node without children first
            // Then recursively allocate children
            // (cannot hold mutable borrows of arena and styles at the same time)
            let child_ids: Vec<_> = children
                .iter()
                .filter(|child_id| doc.block(**child_id).kind != "STYLES")
                .map(|&child_id| convert_block(doc, child_id, Some(&styles), arena))
                .collect();
            BoxContent::Children(child_ids)
        }
    };

    let node = StyledBox {
        id: block.id.clone(),
        kind,
        styles,
        content,
    };

    arena.alloc(node)
}

/// Copies inheritable layout/text fields from parent to child (`child` is usually
/// `ResolvedStyles::default()` before `apply_kind_defaults`).
/// Only: font-*, color, line-height, text-align.
fn inherit_styles(child: &mut ResolvedStyles, parent: &ResolvedStyles) {
    child.font_size = parent.font_size;
    child.font_family = parent.font_family.clone();
    child.font_weight = parent.font_weight;
    child.font_style = parent.font_style;
    child.color = parent.color;
    child.line_height = parent.line_height;
    child.text_align = parent.text_align;
}

/// Applies explicit block attrs on top of accumulated styles.
fn apply_attrs(styles: &mut ResolvedStyles, block: &Block) {
    for (key, value) in &block.attrs {
        match key.as_str() {
            "font-size" => {
                if let Some(v) = value_to_f32(value) {
                    styles.font_size = v;
                }
            }
            "font-family" | "font" => {
                if let Value::Str(s) = value {
                    styles.font_family = s.clone();
                }
            }
            "font-weight" => match value {
                Value::Str(s) if s == "bold" => styles.font_weight = FontWeight::Bold,
                Value::Number(n) if *n >= 600.0 => styles.font_weight = FontWeight::Bold,
                _ => styles.font_weight = FontWeight::Normal,
            },
            "font-style" => {
                if let Value::Str(s) = value {
                    styles.font_style = if s == "italic" {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    };
                }
            }
            "color" => {
                if let Some(c) = value_to_color(value) {
                    styles.color = c;
                }
            }
            "background" | "background-color" => {
                styles.background = value_to_color(value);
            }
            "opacity" => {
                if let Some(v) = value_to_f32(value) {
                    styles.opacity = v.clamp(0.0, 1.0);
                }
            }
            "overflow" => {
                if let Value::Str(s) = value {
                    styles.overflow_clip = matches!(s.as_str(), "clip" | "hidden");
                }
            }
            "margin" => {
                if let Some(v) = value_to_f32(value) {
                    styles.margin = EdgeInsets::uniform(v);
                }
            }
            "margin-top"    => { if let Some(v) = value_to_f32(value) { styles.margin.top = v; } }
            "margin-right"  => { if let Some(v) = value_to_f32(value) { styles.margin.right = v; } }
            "margin-bottom" => { if let Some(v) = value_to_f32(value) { styles.margin.bottom = v; } }
            "margin-left"   => { if let Some(v) = value_to_f32(value) { styles.margin.left = v; } }
            "padding" => {
                if let Some(v) = value_to_f32(value) {
                    styles.padding = EdgeInsets::uniform(v);
                }
            }
            "padding-top"    => { if let Some(v) = value_to_f32(value) { styles.padding.top = v; } }
            "padding-right"  => { if let Some(v) = value_to_f32(value) { styles.padding.right = v; } }
            "padding-bottom" => { if let Some(v) = value_to_f32(value) { styles.padding.bottom = v; } }
            "padding-left"   => { if let Some(v) = value_to_f32(value) { styles.padding.left = v; } }
            "width"  => { styles.width  = value_to_f32(value); }
            "height" => { styles.height = value_to_f32(value); }
            "min-width" => { styles.min_width = value_to_f32(value); }
            "max-width" => { styles.max_width = value_to_f32(value); }
            "min-height" => { styles.min_height = value_to_f32(value); }
            "max-height" => { styles.max_height = value_to_f32(value); }
            "line-height" => {
                if let Some(v) = value_to_f32(value) {
                    styles.line_height = v;
                }
            }
            "text-align" | "align" => {
                if let Value::Str(s) = value {
                    styles.text_align = match s.as_str() {
                        "center"  => TextAlign::Center,
                        "right"   => TextAlign::Right,
                        "justify" => TextAlign::Justify,
                        _         => TextAlign::Left,
                    };
                }
            }
            "letter-spacing" => {
                if let Some(v) = value_to_f32(value) {
                    styles.letter_spacing = v;
                }
            }
            "word-spacing" => {
                if let Some(v) = value_to_f32(value) {
                    styles.word_spacing = v;
                }
            }
            "justify" => {
                match value {
                    Value::Str(s) => styles.justify = matches!(s.as_str(), "true" | "yes" | "1"),
                    Value::Number(n) => styles.justify = *n > 0.0,
                    _ => {}
                }
            }
            "keep-together" => {
                if let Value::Str(s) = value {
                    styles.keep_together = matches!(s.as_str(), "true" | "yes" | "1");
                }
            }
            "keep-with-next" => {
                if let Value::Str(s) = value {
                    styles.keep_with_next = matches!(s.as_str(), "true" | "yes" | "1");
                }
            }
            "widows" => {
                if let Some(v) = value_to_f32(value) {
                    styles.widows = v.max(1.0) as usize;
                }
            }
            "orphans" => {
                if let Some(v) = value_to_f32(value) {
                    styles.orphans = v.max(1.0) as usize;
                }
            }
            "allow-row-split" => {
                if let Value::Str(s) = value {
                    styles.allow_row_split = matches!(s.as_str(), "true" | "yes" | "1");
                }
            }
            "display" => {
                if let Value::Str(s) = value {
                    styles.display = match s.as_str() {
                        "grid"  => Display::Grid,
                        "flex"  => Display::Flex,
                        "none"  => Display::None,
                        _       => Display::Block,
                    };
                }
            }
            "flex-grow" | "grow" => {
                if let Some(v) = value_to_f32(value) {
                    styles.flex_grow = v;
                }
            }
            "type" | "list-type" => {
                if let Value::Str(s) = value {
                    styles.list_style = match s.as_str() {
                        "ordered" | "ol" | "numbered" => ListStyle::Ordered,
                        _ => ListStyle::Bullet,
                    };
                }
            }
            "columns" | "grid-columns" => {
                if let Some(tracks) = parse_grid_columns_value(value) {
                    styles.grid_column_tracks = tracks;
                }
            }
            "column-gap" | "gap" => {
                if let Some(v) = value_to_f32(value) {
                    styles.column_gap = v;
                    styles.row_gap = v;
                }
            }
            "float" => {
                if let Value::Str(s) = value {
                    styles.float = match s.as_str() {
                        "left" => FloatMode::Left,
                        "right" => FloatMode::Right,
                        _ => FloatMode::None,
                    };
                }
            }
            "anchor" => {
                if let Value::Str(s) = value {
                    styles.anchor = Some(s.clone());
                }
            }
            "page-header" => {
                if let Value::Str(s) = value {
                    styles.page_header = Some(s.clone());
                }
            }
            "page-footer" => {
                if let Value::Str(s) = value {
                    styles.page_footer = Some(s.clone());
                }
            }
            _ => {}
        }
    }
}

fn value_to_f32(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n as f32),
        Value::Unit(n, _unit) => Some(*n as f32),
        Value::Str(s) => s.parse::<f32>().ok(),
        _ => None,
    }
}

fn value_to_color(value: &Value) -> Option<Color> {
    match value {
        Value::Color(s) | Value::Str(s) => Color::from_str(s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::arena::NodeId;
    use crate::lexer::Lexer;
    use crate::parser::{self, id, Parser};

    fn parse_doc(input: &str) -> Document {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let doc = parser.parse().expect("parse");
        let doc = parser::resolver::resolve(doc);
        id::assign_ids(doc)
    }

    fn first_heading<'a>(arena: &'a DocumentArena, id: NodeId) -> Option<&'a StyledBox> {
        let node = arena.get(id);
        if matches!(node.kind, BoxKind::Heading(_)) {
            return Some(node);
        }
        if let BoxContent::Children(children) = &node.content {
            for &cid in children {
                if let Some(h) = first_heading(arena, cid) {
                    return Some(h);
                }
            }
        }
        None
    }

    #[test]
    fn opacity_and_overflow_clip_resolve() {
        let doc = parse_doc(r#"PAGE(P({ opacity: 0.5, overflow: clip } Hi))"#);
        let arena = build_styled_tree(&doc);
        let root = arena.roots[0];
        let page = arena.get(root);
        let p = match &page.content {
            BoxContent::Children(ids) => arena.get(ids[0]),
            _ => panic!("expected page with child"),
        };
        assert!((p.styles.opacity - 0.5).abs() < 1e-4);
        assert!(p.styles.overflow_clip);
    }

    #[test]
    fn h1_nested_in_page_keeps_kind_font_not_parent_body() {
        let doc = parse_doc("PAGE(H1(Title))");
        let arena = build_styled_tree(&doc);
        let root = arena.roots[0];
        let h1 = first_heading(&arena, root).expect("H1");
        assert!((h1.styles.font_size - 14.0).abs() < f32::EPSILON);
        assert_eq!(h1.styles.font_weight, FontWeight::Bold);
    }
}

fn flatten_inline(
    nodes: &[InlineNode],
    bold: bool,
    italic: bool,
    link: Option<&str>,
) -> Vec<InlineRun> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            InlineNode::TextRun(text) => out.push(InlineRun {
                text: text.clone(),
                bold,
                italic,
                code: false,
                link: link.map(|s| s.to_string()),
            }),
            InlineNode::CodeSpan(text) => out.push(InlineRun {
                text: text.clone(),
                bold,
                italic,
                code: true,
                link: link.map(|s| s.to_string()),
            }),
            InlineNode::Emphasis(children) => {
                out.extend(flatten_inline(children, bold, true, link));
            }
            InlineNode::Strong(children) => {
                out.extend(flatten_inline(children, true, italic, link));
            }
            InlineNode::LinkSpan { text, href } => {
                out.extend(flatten_inline(text, bold, italic, Some(href)));
            }
        }
    }
    out
}
