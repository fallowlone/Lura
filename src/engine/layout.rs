/// Phase 2: StyledTree → LayoutTree
///
/// Takes `DocumentArena` (styled boxes), builds a parallel taffy tree,
/// runs layout, and extracts X/Y/W/H coordinates.
///
/// Units: pt (points). 1mm = 2.8346pt. A4 = 595.28pt × 841.89pt.

use taffy::prelude::*;
use super::arena::{DocumentArena, NodeId as ArenaNodeId};
use super::grid_tracks::tracks_to_taffy_components;
use super::styles::{BoxContent, BoxKind, Display, InlineRun};

// --- Constants ---

pub const MM_TO_PT: f32 = 2.8346;

pub const A4_WIDTH_MM: f32  = 210.0;
pub const A4_HEIGHT_MM: f32 = 297.0;
pub const A4_WIDTH_PT: f32  = A4_WIDTH_MM  * MM_TO_PT;  // ≈ 595.3
pub const A4_HEIGHT_PT: f32 = A4_HEIGHT_MM * MM_TO_PT;  // ≈ 841.9

pub const PAGE_MARGIN_MM: f32 = 15.0;
pub const PAGE_MARGIN_PT: f32 = PAGE_MARGIN_MM * MM_TO_PT;

/// A4 content area width in pt
pub const CONTENT_WIDTH_PT: f32 = (A4_WIDTH_MM - PAGE_MARGIN_MM * 2.0) * MM_TO_PT; // ≈ 481.0

// --- Result structures ---

/// Index into the flat `LayoutTree.nodes` array
pub type LayoutNodeIdx = usize;

#[derive(Debug, Clone)]
pub enum LayoutContent {
    Text(String),
    Inline(Vec<InlineRun>),
    Children(Vec<LayoutNodeIdx>),
    Empty,
}

/// Laid-out box with computed absolute coordinates.
/// Coordinates are in pt from the page top-left corner.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Back-reference to the styled-tree node
    pub arena_id: ArenaNodeId,
    pub kind: BoxKind,

    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,

    pub content: LayoutContent,
}

/// Flat list of all layout nodes in the document.
/// Roots are stored in `roots` (indices into `nodes`).
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

// --- Layout build ---

/// Main entry for phase 2.
/// Takes styled tree (`DocumentArena`), returns `LayoutTree`.
pub fn compute_layout(styled: &DocumentArena) -> LayoutTree {
    let mut taffy: TaffyTree<ArenaNodeId> = TaffyTree::new();
    let mut layout_tree = LayoutTree::new();

    // Build taffy nodes for each root
    let taffy_roots: Vec<NodeId> = styled.roots
        .iter()
        .map(|&arena_id| build_taffy_node(arena_id, styled, &mut taffy))
        .collect();

    // Run layout for each root PAGE.
    // Use CONTENT_WIDTH_PT (width without page margins).
    // Margins (PAGE_MARGIN_PT) are applied in extract_layout via parent_x/parent_y.
    for taffy_root in &taffy_roots {
        let available = Size {
            width:  AvailableSpace::Definite(CONTENT_WIDTH_PT),
            height: AvailableSpace::MaxContent,
        };
        let _ = taffy.compute_layout(*taffy_root, available);
    }

    // Flatten results into LayoutTree
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

        // Next PAGE starts below
        if let Ok(l) = taffy.layout(taffy_id) {
            offset_y += l.size.height + PAGE_MARGIN_PT;
        }
    }

    layout_tree
}

// --- Taffy tree construction ---

fn build_taffy_node(
    arena_id: ArenaNodeId,
    styled: &DocumentArena,
    taffy: &mut TaffyTree<ArenaNodeId>,
) -> NodeId {
    let node = styled.get(arena_id);
    let style = styled_box_to_taffy_style(&node.styles, &node.kind);

    match &node.content {
        BoxContent::Text(_) | BoxContent::Inline(_) | BoxContent::Empty => {
            // Leaf text node.
            // Height would come from a measure function in compute_layout_with_measure;
            // v2 keeps a fixed height per line for simplicity.
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

/// Map our styles to a taffy `Style`.
fn styled_box_to_taffy_style(styles: &super::styles::ResolvedStyles, kind: &BoxKind) -> Style {
    use taffy::style_helpers::*;

    let display = match styles.display {
        Display::Grid  => taffy::Display::Grid,
        Display::Flex  => taffy::Display::Flex,
        Display::None  => taffy::Display::None,
        Display::Block => match kind {
            BoxKind::Page  => taffy::Display::Flex,   // flex column: stack blocks
            BoxKind::Grid  => taffy::Display::Grid,
            BoxKind::Table => taffy::Display::Block,  // block: stack rows vertically
            BoxKind::Row   => taffy::Display::Flex,   // flex row: cells in a row
            BoxKind::Figure => taffy::Display::Flex, // column stack: asset + caption
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
            // PAGE width is the content area; margins applied outside via parent_x
            BoxKind::Page => Dimension::length(CONTENT_WIDTH_PT),
            _             => Dimension::auto(),
        },
    };

    let height = match styles.height {
        Some(h) => Dimension::length(h * MM_TO_PT),
        None    => Dimension::auto(),
    };

    let min_size = Size {
        width: styles.min_width.map(|v| v * MM_TO_PT).map(Dimension::length).unwrap_or(Dimension::auto()),
        height: styles.min_height.map(|v| v * MM_TO_PT).map(Dimension::length).unwrap_or(Dimension::auto()),
    };
    let max_size = Size {
        width: styles.max_width.map(|v| v * MM_TO_PT).map(Dimension::length).unwrap_or(Dimension::auto()),
        height: styles.max_height.map(|v| v * MM_TO_PT).map(Dimension::length).unwrap_or(Dimension::auto()),
    };

    // Grid blocks: set columns (fr / lengths / auto from columns attr)
    let grid_template_columns = if matches!(display, taffy::Display::Grid) {
        tracks_to_taffy_components(&styles.grid_column_tracks)
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
        min_size,
        max_size,
        margin,
        padding,
        gap,
        grid_template_columns,
        // flex-direction follows block kind
        flex_direction: match kind {
            BoxKind::Page => FlexDirection::Column, // vertical stack of blocks
            BoxKind::Row  => FlexDirection::Row,    // horizontal stack of cells
            BoxKind::Figure => FlexDirection::Column,
            _             => FlexDirection::Row,
        },
        // CELL flex-grow from styles (default 1.0 for equal columns)
        flex_grow: if matches!(kind, BoxKind::Cell) {
            if styles.flex_grow > 0.0 { styles.flex_grow } else { 1.0 }
        } else {
            styles.flex_grow
        },
        ..Default::default()
    }
}

// --- Extract results ---

/// Recursively pull layout results from taffy into `LayoutTree`.
/// Returns the index of the new `LayoutBox` in `layout_tree.nodes`.
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
        BoxContent::Inline(runs) => LayoutContent::Inline(runs.clone()),
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
