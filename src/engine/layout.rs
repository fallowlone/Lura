/// Фаза 2: StyledTree → LayoutTree
///
/// Берёт DocumentArena (StyledBoxes), строит параллельное дерево taffy,
/// запускает вычисление геометрии и извлекает координаты X/Y/W/H.
///
/// Единица измерения: pt (points). 1mm = 2.8346pt. A4 = 595.28pt × 841.89pt.

use taffy::prelude::*;
use super::arena::{DocumentArena, NodeId as ArenaNodeId};
use super::styles::{BoxContent, BoxKind, Display};

// ─── Константы ────────────────────────────────────────────────────────────────

pub const MM_TO_PT: f32 = 2.8346;

pub const A4_WIDTH_MM: f32  = 210.0;
pub const A4_HEIGHT_MM: f32 = 297.0;
pub const A4_WIDTH_PT: f32  = A4_WIDTH_MM  * MM_TO_PT;  // ≈ 595.3
pub const A4_HEIGHT_PT: f32 = A4_HEIGHT_MM * MM_TO_PT;  // ≈ 841.9

pub const PAGE_MARGIN_MM: f32 = 20.0;
pub const PAGE_MARGIN_PT: f32 = PAGE_MARGIN_MM * MM_TO_PT;

/// Ширина контентной области A4 в pt
pub const CONTENT_WIDTH_PT: f32 = (A4_WIDTH_MM - PAGE_MARGIN_MM * 2.0) * MM_TO_PT; // ≈ 481.0

// ─── Структуры результата ─────────────────────────────────────────────────────

/// Индекс в плоском массиве LayoutTree.nodes
pub type LayoutNodeIdx = usize;

#[derive(Debug, Clone)]
pub enum LayoutContent {
    Text(String),
    Children(Vec<LayoutNodeIdx>),
    Empty,
}

/// Готовый блок с вычисленными абсолютными координатами.
/// Координаты — в pt, относительно левого верхнего угла страницы.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Обратная ссылка на узел в StyledTree
    pub arena_id: ArenaNodeId,
    pub kind: BoxKind,

    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,

    pub content: LayoutContent,
}

/// Плоский список всех layout-узлов документа.
/// Корни хранятся в `roots` (индексы в nodes).
pub struct LayoutTree {
    pub nodes: Vec<LayoutBox>,
    pub roots: Vec<LayoutNodeIdx>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), roots: Vec::new() }
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Сборка Layout ────────────────────────────────────────────────────────────

/// Основная функция фазы 2.
/// Принимает StyledTree (DocumentArena), возвращает LayoutTree.
pub fn compute_layout(styled: &DocumentArena) -> LayoutTree {
    let mut taffy: TaffyTree<ArenaNodeId> = TaffyTree::new();
    let mut layout_tree = LayoutTree::new();

    // Строим taffy-узлы для каждого root
    let taffy_roots: Vec<NodeId> = styled.roots
        .iter()
        .map(|&arena_id| build_taffy_node(arena_id, styled, &mut taffy))
        .collect();

    // Запускаем layout для каждого root-PAGE.
    // Используем CONTENT_WIDTH_PT — ширину без полей страницы.
    // Поля (PAGE_MARGIN_PT) добавляются в extract_layout через parent_x/parent_y.
    for taffy_root in &taffy_roots {
        let available = Size {
            width:  AvailableSpace::Definite(CONTENT_WIDTH_PT),
            height: AvailableSpace::MaxContent,
        };
        let _ = taffy.compute_layout(*taffy_root, available);
    }

    // Извлекаем результаты в плоский LayoutTree
    let mut offset_y = 0.0f32;
    for (&arena_id, &taffy_id) in styled.roots.iter().zip(taffy_roots.iter()) {
        let root_idx = extract_layout(
            taffy_id,
            arena_id,
            styled,
            &taffy,
            PAGE_MARGIN_PT,
            offset_y + PAGE_MARGIN_PT,
            &mut layout_tree,
        );
        layout_tree.roots.push(root_idx);

        // Следующая PAGE начинается ниже
        if let Ok(l) = taffy.layout(taffy_id) {
            offset_y += l.size.height + PAGE_MARGIN_PT;
        }
    }

    layout_tree
}

// ─── Построение taffy-дерева ─────────────────────────────────────────────────

fn build_taffy_node(
    arena_id: ArenaNodeId,
    styled: &DocumentArena,
    taffy: &mut TaffyTree<ArenaNodeId>,
) -> NodeId {
    let node = styled.get(arena_id);
    let style = styled_box_to_taffy_style(&node.styles, &node.kind);

    match &node.content {
        BoxContent::Text(_) | BoxContent::Empty => {
            // Листовой текстовый узел.
            // Высоту считаем через measure-function в compute_layout_with_measure;
            // для простоты v2 используем фиксированную высоту на строку.
            taffy.new_leaf_with_context(style, arena_id).unwrap()
        }
        BoxContent::Children(children) => {
            let child_ids: Vec<NodeId> = children
                .iter()
                .map(|&child_arena_id| build_taffy_node(child_arena_id, styled, taffy))
                .collect();
            taffy.new_with_children(style, &child_ids).unwrap()
        }
    }
}

/// Маппинг наших стилей → taffy Style.
fn styled_box_to_taffy_style(styles: &super::styles::ResolvedStyles, kind: &BoxKind) -> Style {
    use taffy::style_helpers::*;

    let display = match styles.display {
        Display::Grid  => taffy::Display::Grid,
        Display::Flex  => taffy::Display::Flex,
        Display::None  => taffy::Display::None,
        Display::Block => match kind {
            BoxKind::Page  => taffy::Display::Flex,   // flex-column: стекирует блоки
            BoxKind::Grid  => taffy::Display::Grid,
            BoxKind::Table => taffy::Display::Block,  // block: стекирует строки вертикально
            BoxKind::Row   => taffy::Display::Flex,   // flex-row: ячейки рядом
            _ => taffy::Display::Block,
        },
    };

    let margin = Rect {
        left:   length(styles.margin.left   * MM_TO_PT),
        right:  length(styles.margin.right  * MM_TO_PT),
        top:    length(styles.margin.top    * MM_TO_PT),
        bottom: length(styles.margin.bottom * MM_TO_PT),
    };
    let padding = Rect {
        left:   length(styles.padding.left   * MM_TO_PT),
        right:  length(styles.padding.right  * MM_TO_PT),
        top:    length(styles.padding.top    * MM_TO_PT),
        bottom: length(styles.padding.bottom * MM_TO_PT),
    };

    let width = match styles.width {
        Some(w) => Dimension::length(w * MM_TO_PT),
        None    => match kind {
            // PAGE задаётся шириной контентной области — поля добавляются снаружи через parent_x
            BoxKind::Page => Dimension::length(CONTENT_WIDTH_PT),
            _             => Dimension::auto(),
        },
    };

    let height = match styles.height {
        Some(h) => Dimension::length(h * MM_TO_PT),
        None    => Dimension::auto(),
    };

    // Для grid-блоков задаём колонки
    let grid_template_columns = if matches!(display, taffy::Display::Grid) {
        let cols = styles.grid_columns.unwrap_or(1);
        vec![fr(1.0); cols]
    } else {
        vec![]
    };

    let gap = if matches!(display, taffy::Display::Grid | taffy::Display::Flex) {
        Size {
            width:  length(styles.column_gap * MM_TO_PT),
            height: length(styles.row_gap    * MM_TO_PT),
        }
    } else {
        Size::zero()
    };

    Style {
        display,
        size: Size { width, height },
        margin,
        padding,
        gap,
        grid_template_columns,
        // flex-direction зависит от типа блока
        flex_direction: match kind {
            BoxKind::Page => FlexDirection::Column, // вертикальный стек блоков
            BoxKind::Row  => FlexDirection::Row,    // горизонтальный стек ячеек
            _             => FlexDirection::Row,
        },
        // flex-grow: 1 для CELL — занимает равное место в строке
        flex_grow: if matches!(kind, BoxKind::Cell) { 1.0 } else { 0.0 },
        ..Default::default()
    }
}

// ─── Извлечение результатов ───────────────────────────────────────────────────

/// Рекурсивно извлекает layout-результаты из taffy и заполняет LayoutTree.
/// Возвращает индекс созданного LayoutBox в layout_tree.nodes.
fn extract_layout(
    taffy_id: NodeId,
    arena_id: ArenaNodeId,
    styled: &DocumentArena,
    taffy: &TaffyTree<ArenaNodeId>,
    parent_x: f32,
    parent_y: f32,
    layout_tree: &mut LayoutTree,
) -> LayoutNodeIdx {
    let layout = taffy.layout(taffy_id).unwrap();

    let abs_x = parent_x + layout.location.x;
    let abs_y = parent_y + layout.location.y;

    let node = styled.get(arena_id);

    let content = match &node.content {
        BoxContent::Text(text) => LayoutContent::Text(text.clone()),
        BoxContent::Empty => LayoutContent::Empty,
        BoxContent::Children(children) => {
            let taffy_children = taffy.children(taffy_id).unwrap_or_default();
            let child_indices: Vec<LayoutNodeIdx> = children
                .iter()
                .zip(taffy_children.iter())
                .map(|(&child_arena_id, &child_taffy_id)| {
                    extract_layout(
                        child_taffy_id,
                        child_arena_id,
                        styled,
                        taffy,
                        abs_x,
                        abs_y,
                        layout_tree,
                    )
                })
                .collect();
            LayoutContent::Children(child_indices)
        }
    };

    let layout_box = LayoutBox {
        arena_id,
        kind: node.kind.clone(),
        x: abs_x,
        y: abs_y,
        width: layout.size.width,
        height: layout.size.height,
        content,
    };

    let idx = layout_tree.nodes.len();
    layout_tree.nodes.push(layout_box);
    idx
}
