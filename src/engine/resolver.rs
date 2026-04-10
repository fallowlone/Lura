/// Конвертирует AST парсера в StyledTree (Arena of StyledBoxes).
///
/// Два прохода:
/// 1. Конвертация узлов в StyledBox с дефолтными стилями + явными attrs
/// 2. Наследование стилей от родителя к ребёнку (font-size, color, etc.)

use crate::parser::ast::{Block, Content, Document, Value};
use super::arena::DocumentArena;
use super::styles::{
    BoxContent, BoxKind, Color, Display, EdgeInsets, FontStyle, FontWeight,
    ListStyle, ResolvedStyles, StyledBox, TextAlign,
};

/// Основная точка входа.
/// Принимает Document (из парсера), возвращает DocumentArena.
pub fn build_styled_tree(doc: &Document) -> DocumentArena {
    let mut arena = DocumentArena::new();

    for block in &doc.blocks {
        if block.kind == "STYLES" {
            continue;
        }
        let node_id = convert_block(block, None, &mut arena);
        arena.add_root(node_id);
    }

    arena
}

/// Рекурсивно конвертирует блок AST → StyledBox в арене.
/// `parent_styles` — уже разрешённые стили родителя (для наследования).
fn convert_block(
    block: &Block,
    parent_styles: Option<&ResolvedStyles>,
    arena: &mut DocumentArena,
) -> super::arena::NodeId {
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
        Content::Empty => BoxContent::Empty,
        Content::Blocks(children) => {
            // Сначала аллоцируем узел без детей
            // Затем рекурсивно аллоцируем детей
            // (нельзя иметь mutable borrow на arena и styles одновременно)
            let child_ids: Vec<_> = children
                .iter()
                .filter(|c| c.kind != "STYLES")
                .map(|child| convert_block(child, Some(&styles), arena))
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
                if let Some(v) = value_to_f32(value) {
                    styles.grid_columns = Some(v as usize);
                }
            }
            "column-gap" | "gap" => {
                if let Some(v) = value_to_f32(value) {
                    styles.column_gap = v;
                    styles.row_gap = v;
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
