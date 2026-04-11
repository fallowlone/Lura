/// Конвертирует AST парсера в StyledTree (Arena of StyledBoxes).
///
/// Два прохода:
/// 1. Конвертация узлов в StyledBox с дефолтными стилями + явными attrs
/// 2. Наследование стилей от родителя к ребёнку (font-size, color, etc.)

use crate::parser::ast::{Block, Content, Document, InlineNode, NodeId as AstNodeId, Value};
use super::arena::DocumentArena;
use super::grid_tracks::parse_grid_columns_value;
use super::styles::{
    BoxContent, BoxKind, Color, Display, EdgeInsets, FloatMode, FontStyle, FontWeight, InlineRun,
    ListStyle, ResolvedStyles, StyledBox, TextAlign,
};

/// Основная точка входа.
/// Принимает Document (из парсера), возвращает DocumentArena.
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

/// Рекурсивно конвертирует блок AST → StyledBox в арене.
/// `parent_styles` — уже разрешённые стили родителя (для наследования).
fn convert_block(
    doc: &Document,
    ast_node_id: AstNodeId,
    parent_styles: Option<&ResolvedStyles>,
    arena: &mut DocumentArena,
) -> super::arena::NodeId {
    let block = doc.block(ast_node_id);
    let kind = BoxKind::from_str(&block.kind);

    // Начинаем со стилей по умолчанию для этого вида блока
    let mut styles = ResolvedStyles::for_kind(&kind);

    // Наследуем от родителя (только наследуемые свойства)
    if let Some(parent) = parent_styles {
        inherit_styles(&mut styles, parent);
    }

    // Применяем явные attrs блока (переопределяют дефолты и наследование)
    apply_attrs(&mut styles, block);

    // Конвертируем контент
    let content = match &block.content {
        Content::Text(s) => BoxContent::Text(s.clone()),
        Content::Inline(nodes) => BoxContent::Inline(flatten_inline(nodes, false, false, None)),
        Content::Empty => BoxContent::Empty,
        Content::Children(children) => {
            // Сначала аллоцируем узел без детей
            // Затем рекурсивно аллоцируем детей
            // (нельзя иметь mutable borrow на arena и styles одновременно)
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

/// Копирует наследуемые CSS-свойства от родителя к ребёнку.
/// Только то, что реально наследуется в типографике:
/// font-*, color, line-height, text-align.
fn inherit_styles(child: &mut ResolvedStyles, parent: &ResolvedStyles) {
    child.font_size = parent.font_size;
    child.font_family = parent.font_family.clone();
    child.font_weight = parent.font_weight;
    child.font_style = parent.font_style;
    child.color = parent.color;
    child.line_height = parent.line_height;
    child.text_align = parent.text_align;
}

/// Применяет явные attrs блока поверх уже накопленных стилей.
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
