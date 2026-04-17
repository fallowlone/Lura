/// Phase 4a: LayoutTree → PageTree
///
/// Sequential `cursor_y` drives vertical flow.
/// X position and width come from taffy.
/// For GRID: `cursor_y` is saved and restored per cell in each row.
use std::collections::HashMap;

use super::grid_tracks::GridColumnTrack;
use super::layout::{
    text_container_width_pt, LayoutBox, LayoutContent, LayoutNodeIdx, LayoutTree, A4_HEIGHT_PT,
    A4_WIDTH_PT, CONTENT_WIDTH_PT, MM_TO_PT, PAGE_MARGIN_PT,
};
use super::styles::{
    BoxKind, Color, FloatMode, FontStyle, FontWeight, InlineRun, ListStyle, TextAlign,
    VerticalAlign,
};
use super::text::{
    break_inline_runs, break_text, inline_lines_block_height, inline_runs_block_height,
    text_block_height, text_width_pt_with_spacing, InlineLine, TextLine, TextLayoutOpts,
};

// --- Draw commands ---

#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Multiply alpha for subsequent primitives until matching [`DrawCommand::PopOpacity`].
    PushOpacity { alpha: f32 },
    PopOpacity,
    /// Clip to axis-aligned rect (page coords, top-left origin) until [`DrawCommand::PopClip`].
    PushClipRect { x: f32, y: f32, w: f32, h: f32 },
    PopClip,
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        width: f32,
    },
    Text {
        content: String,
        x: f32,
        y: f32,
        font_size: f32,
        font_family: String,
        bold: bool,
        italic: bool,
        color: Color,
        /// When set, PDF/SVG backends emit a clickable link to this URI.
        link_uri: Option<String>,
        /// Horizontal extent in pt for link hit area (fragment advance); set with `link_uri`.
        link_width_pt: Option<f32>,
    },
}

// ─── PageTree ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub commands: Vec<DrawCommand>,
}

impl Page {
    fn new() -> Self {
        Self {
            width: A4_WIDTH_PT,
            height: A4_HEIGHT_PT,
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchorPosition {
    /// 1-based page index
    pub page_index: u32,
    /// Y coordinate (top-down) on the page
    pub y: f32,
}

/// Searchable text unit — one per painted line. Coordinates are in PDF points
/// with bottom-origin (y=0 at page bottom) so Swift can hand the rect to
/// PDFKit annotations without axis flipping.
#[derive(Debug, Clone)]
pub struct TextUnit {
    /// 0-based page index.
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub block_id: String,
}

#[derive(Debug)]
pub struct PageTree {
    pub pages: Vec<Page>,
    /// Stable block `id` → 1-based page index where the block first enters pagination.
    pub block_start_page: HashMap<String, u32>,
    /// Anchor ID → position (page index, Y coordinate). Collected during painting.
    pub anchor_positions: HashMap<String, AnchorPosition>,
    /// Searchable text index: one entry per painted line.
    pub text_units: Vec<TextUnit>,
}

impl PageTree {
    pub fn new() -> Self {
        Self {
            pages: vec![Page::new()],
            block_start_page: HashMap::new(),
            anchor_positions: HashMap::new(),
            text_units: Vec::new(),
        }
    }
}

impl Default for PageTree {
    fn default() -> Self {
        Self::new()
    }
}

// --- Constants ---

const CONTENT_TOP: f32 = PAGE_MARGIN_PT;
const CONTENT_BOTTOM: f32 = A4_HEIGHT_PT - PAGE_MARGIN_PT;

// ─── Paginator ────────────────────────────────────────────────────────────────

struct Paginator<'a> {
    layout: &'a LayoutTree,
    styled: &'a super::arena::DocumentArena,
    pages: Vec<Page>,
    cursor_y: f32,
    block_start_page: HashMap<String, u32>,
    /// Anchor ID → position (page index, Y coordinate). Collected during painting.
    anchor_positions: HashMap<String, AnchorPosition>,
    /// Counter for the current ordered list (`None` = bulleted)
    list_item_counter: Option<usize>,
    page_header: Option<String>,
    page_footer: Option<String>,
    /// Pending block IDs awaiting first draw. Flushed to `block_start_page`
    /// on the next `push_cmd`, so a block records the page where its first
    /// paint lands (not the page where `place_node` entered).
    pending_block_starts: Vec<String>,
    /// Opacity stack for cross-page rebalancing inside `new_page`.
    active_opacity: Vec<f32>,
    /// Clip rect stack for cross-page rebalancing inside `new_page`.
    /// Stores (x, y, w, h) of each active clip in order of nesting.
    active_clip: Vec<(f32, f32, f32, f32)>,
    /// Per-layout-node (x, width) overrides. Used by `place_table` to snap cell
    /// children to the normalized column rect (taffy computes flex per-row, so
    /// cell widths can differ across rows in the same column).
    rect_override: HashMap<LayoutNodeIdx, (f32, f32)>,
    /// Accumulator for `PageTree::text_units`.
    text_units: Vec<TextUnit>,
    /// Block id of the most-recent `queue_block_start` call, threaded into each
    /// TextUnit so Swift can jump back to the source block for a match.
    current_block_id: String,
}

impl<'a> Paginator<'a> {
    fn new(layout: &'a LayoutTree, styled: &'a super::arena::DocumentArena) -> Self {
        Self {
            layout,
            styled,
            pages: vec![Page::new()],
            cursor_y: CONTENT_TOP,
            block_start_page: HashMap::new(),
            anchor_positions: HashMap::new(),
            list_item_counter: None,
            page_header: None,
            page_footer: None,
            pending_block_starts: Vec::new(),
            active_opacity: Vec::new(),
            active_clip: Vec::new(),
            rect_override: HashMap::new(),
            text_units: Vec::new(),
            current_block_id: String::new(),
        }
    }

    /// Read node with any pending (x, width) override applied.
    fn node_with_override(&self, idx: LayoutNodeIdx) -> LayoutBox {
        let mut n = self.layout.nodes[idx].clone();
        if let Some(&(x, w)) = self.rect_override.get(&idx) {
            n.x = x;
            n.width = w;
        }
        n
    }

    /// Queue a block to record its start page and anchor position (if applicable) when its first draw lands.
    fn queue_block_start(&mut self, arena_id: super::arena::NodeId) {
        let id = self.styled.get(arena_id).id.clone();
        if id.is_empty() {
            return;
        }
        self.current_block_id = id.clone();
        if self.block_start_page.contains_key(&id) {
            return;
        }
        self.pending_block_starts.push(id);
    }

    fn flush_pending_block_starts(&mut self) {
        if self.pending_block_starts.is_empty() {
            return;
        }
        let page_1based = self.pages.len() as u32;
        let cursor_y = self.cursor_y;
        let styled_roots: Vec<_> = self.styled.roots.to_vec();
        let ids_to_process: Vec<_> = self.pending_block_starts.drain(..).collect();

        for id in ids_to_process {
            self.block_start_page.entry(id.clone()).or_insert(page_1based);

            // Record anchor position if this block has an anchor attribute.
            let styled_node = styled_roots.iter().find_map(|&root_id| {
                self.find_styled_node_by_id(root_id, &id)
            });
            if let Some(styled_node) = styled_node
                && let Some(anchor_id) = styled_node.styles.anchor.as_ref()
                    && !anchor_id.is_empty() && !self.anchor_positions.contains_key(anchor_id) {
                        self.anchor_positions.insert(
                            anchor_id.clone(),
                            AnchorPosition {
                                page_index: page_1based,
                                y: cursor_y,
                            },
                        );
                    }
        }
    }

    /// Find a styled node by its block ID (used for anchor tracking).
    fn find_styled_node_by_id(&self, node_id: super::arena::NodeId, block_id: &str) -> Option<&super::styles::StyledBox> {
        let node = self.styled.get(node_id);
        if node.id == block_id {
            return Some(node);
        }
        if let super::styles::BoxContent::Children(children) = &node.content {
            for &child_id in children {
                if let Some(found) = self.find_styled_node_by_id(child_id, block_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn new_page(&mut self) {
        // Close cross-page wraps on old page.
        for _ in &self.active_clip {
            self.pages
                .last_mut()
                .expect("at least one page exists")
                .commands
                .push(DrawCommand::PopClip);
        }
        for _ in &self.active_opacity {
            self.pages
                .last_mut()
                .expect("at least one page exists")
                .commands
                .push(DrawCommand::PopOpacity);
        }
        self.pages.push(Page::new());
        self.cursor_y = CONTENT_TOP;
        self.draw_page_chrome();
        // Re-open wraps on new page.
        let clips = self.active_clip.clone();
        for (clip_x, _clip_y, clip_w, _clip_h) in clips {
            // Continuation-page clip rect uses the new page's full content area.
            // This keeps the clip effective but constrains it to the new page's bounds.
            self.pages
                .last_mut()
                .expect("at least one page exists")
                .commands
                .push(DrawCommand::PushClipRect {
                    x: clip_x,
                    y: CONTENT_TOP,
                    w: clip_w,
                    h: CONTENT_BOTTOM - CONTENT_TOP,
                });
        }
        let alphas = self.active_opacity.clone();
        for alpha in alphas {
            self.pages
                .last_mut()
                .expect("at least one page exists")
                .commands
                .push(DrawCommand::PushOpacity { alpha });
        }
    }

    fn push_cmd(&mut self, cmd: DrawCommand) {
        // Painting command → block's first visible location is now fixed.
        self.flush_pending_block_starts();
        match &cmd {
            DrawCommand::PushOpacity { alpha } => self.active_opacity.push(*alpha),
            DrawCommand::PopOpacity => {
                self.active_opacity.pop();
            }
            DrawCommand::PushClipRect { x, y, w, h } => self.active_clip.push((*x, *y, *w, *h)),
            DrawCommand::PopClip => {
                self.active_clip
                    .pop()
                    .expect("PopClip without matching PushClipRect");
            }
            DrawCommand::Text {
                content,
                x,
                y,
                font_size,
                font_family,
                bold,
                ..
            } if !content.is_empty() => {
                let mono = font_family.eq_ignore_ascii_case("Courier");
                let width = if mono {
                    // Monospace: 0.6 em per char is a close-enough Courier
                    // advance for highlight rects; skip the proportional
                    // measurement path entirely.
                    (content.chars().count() as f32) * font_size * 0.6
                } else {
                    super::text::text_width_pt_with_spacing(
                        content, *font_size, *bold, 0.0, 0.0,
                    )
                };
                // `y` is the painter's top-origin baseline. Convert to
                // PDF bottom-origin for the match rect (baseline sits near the
                // top of the rect; pad down ~20% of font_size for descent).
                let baseline_pdf = A4_HEIGHT_PT - *y;
                let y_pdf_bottom = baseline_pdf - *font_size * 0.2;
                let page_zero = (self.pages.len().saturating_sub(1)) as u32;
                self.text_units.push(TextUnit {
                    page: page_zero,
                    x: *x,
                    y: y_pdf_bottom,
                    w: width,
                    h: *font_size,
                    text: content.clone(),
                    block_id: self.current_block_id.clone(),
                });
            }
            _ => {}
        }
        self.pages
            .last_mut()
            .expect("at least one page exists")
            .commands
            .push(cmd);
    }

    /// Placeholder height for `FIGURE` / `IMAGE` until raster decode exists.
    /// Uses explicit `height` mm, else `width` mm, else 40 mm.
    fn figure_placeholder_height_pt(styles: &super::styles::ResolvedStyles) -> f32 {
        let mm = styles
            .height
            .or(styles.width)
            .unwrap_or(40.0)
            .max(8.0);
        mm * MM_TO_PT
    }

    fn draw_figure_placeholder(&mut self, block_x: f32, node: &super::layout::LayoutBox) {
        let styles = self.styled.get(node.arena_id).styles.clone();
        let w = node.width.max(1.0);
        let h = Self::figure_placeholder_height_pt(&styles);
        if self.cursor_y + h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
            self.new_page();
        }
        self.push_cmd(DrawCommand::Rect {
            x: block_x,
            y: self.cursor_y,
            w,
            h,
            fill: Some(Color::from_hex(0xE5E7EB)),
            stroke: Some(Color::from_hex(0x9CA3AF)),
            stroke_width: 0.5,
        });
        let label = "Figure";
        self.push_cmd(DrawCommand::Text {
            content: label.into(),
            x: block_x + 4.0,
            y: self.cursor_y + styles.font_size.min(h * 0.35).max(8.0),
            font_size: styles.font_size.min(10.0),
            font_family: styles.font_family.clone(),
            bold: false,
            italic: false,
            color: Color::from_hex(0x4B5563),
            link_uri: None,
            link_width_pt: None,
        });
        self.cursor_y += h;
    }

    fn draw_page_chrome(&mut self) {
        if let Some(header) = &self.page_header {
            self.push_cmd(DrawCommand::Text {
                content: header.clone(),
                x: PAGE_MARGIN_PT,
                y: PAGE_MARGIN_PT * 0.65,
                font_size: 9.0,
                font_family: "Helvetica".to_string(),
                bold: false,
                italic: false,
                color: Color::from_hex(0x6B7280),
                link_uri: None,
                link_width_pt: None,
            });
        }
        if let Some(footer) = &self.page_footer {
            self.push_cmd(DrawCommand::Text {
                content: footer.clone(),
                x: PAGE_MARGIN_PT,
                y: A4_HEIGHT_PT - PAGE_MARGIN_PT * 0.35,
                font_size: 9.0,
                font_family: "Helvetica".to_string(),
                bold: false,
                italic: false,
                color: Color::from_hex(0x6B7280),
                link_uri: None,
                link_width_pt: None,
            });
        }
    }

    // --- Place arbitrary node ---

    fn place_node(&mut self, node_idx: LayoutNodeIdx) -> f32 {
        let node = self.node_with_override(node_idx);
        self.queue_block_start(node.arena_id);
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;
        let margin_top = styles.margin.top * MM_TO_PT;
        let margin_bottom = styles.margin.bottom * MM_TO_PT;
        let padding_left = styles.padding.left * MM_TO_PT;
        let padding_top_pt = styles.padding.top * MM_TO_PT;
        let padding_bottom_pt = styles.padding.bottom * MM_TO_PT;
        let block_x = match styles.float {
            FloatMode::Left => PAGE_MARGIN_PT,
            FloatMode::Right => {
                (A4_WIDTH_PT - PAGE_MARGIN_PT - node.width.max(1.0)).max(PAGE_MARGIN_PT)
            }
            FloatMode::None => node.x,
        };

        self.cursor_y += margin_top;

        let wrap_opacity = styles.opacity < 1.0 - 1e-4;
        let wrap_clip = styles.overflow_clip;
        let est_h_for_clip = self.estimate_height(node_idx).max(1.0);

        if wrap_opacity {
            self.push_cmd(DrawCommand::PushOpacity {
                alpha: styles.opacity.clamp(0.0, 1.0),
            });
        }

        if let Some(bg) = styles.background
            && !matches!(
                &node.content,
                LayoutContent::Text(_) | LayoutContent::Inline(_)
            ) {
                let estimated_h = self.estimate_height(node_idx);
                // Known limitation: container-level background only covers the first
                // page segment. Multi-page containers that cross a page boundary
                // skip the bg rect to avoid an orphan rect hanging off the page.
                let remaining = (CONTENT_BOTTOM - self.cursor_y).max(0.0);
                if estimated_h <= remaining + 0.5 {
                    self.push_cmd(DrawCommand::Rect {
                        x: block_x,
                        y: self.cursor_y,
                        w: node.width.max(1.0),
                        h: estimated_h,
                        fill: Some(bg),
                        stroke: None,
                        stroke_width: 0.0,
                    });
                }
            }

        if wrap_clip {
            self.push_cmd(DrawCommand::PushClipRect {
                x: block_x,
                y: self.cursor_y,
                w: node.width.max(1.0),
                h: est_h_for_clip,
            });
        }

        let consumed = match node.content.clone() {
            // --- Text block ---
            LayoutContent::Text(text) => {
                // ListItem uses `place_list_item` (bullet). Must not `return` early: opacity/clip
                // wraps were already pushed above and need PopClip/PopOpacity after this arm.
                if matches!(node.kind, BoxKind::ListItem) {
                    self.place_list_item(node_idx, &text)
                } else if matches!(node.kind, BoxKind::Code) {
                    // CODE raw body: one output line per source line, no wrapping,
                    // literal leading whitespace preserved.
                    let line_h = styles.font_size * styles.line_height;
                    let lines: Vec<TextLine> = text
                        .split('\n')
                        .map(|s| TextLine {
                            text: s.to_string(),
                            width: 0.0,
                            line_height_pt: line_h,
                            font_size: styles.font_size,
                        })
                        .collect();
                    self.paint_text_lines_paginated(
                        &lines,
                        block_x,
                        node.width,
                        padding_left,
                        &styles,
                        bold,
                        padding_top_pt,
                        padding_bottom_pt,
                    )
                } else {
                    let width = text_container_width_pt(node.width, styles.padding.left, styles.padding.right);
                    let lines = break_text(
                        &text,
                        width,
                        styles.font_size,
                        styles.line_height,
                        bold,
                        styles.letter_spacing,
                        styles.word_spacing,
                    );
                    self.paint_text_lines_paginated(
                        &lines,
                        block_x,
                        node.width,
                        padding_left,
                        &styles,
                        bold,
                        padding_top_pt,
                        padding_bottom_pt,
                    )
                }
            }
            LayoutContent::Inline(runs) => {
                let width = text_container_width_pt(node.width, styles.padding.left, styles.padding.right);
                let justify = styles.justify || styles.text_align == super::styles::TextAlign::Justify;
                let lines = break_inline_runs(
                    &runs,
                    width,
                    &TextLayoutOpts {
                        font_size_pt: styles.font_size,
                        line_height: styles.line_height,
                        letter_spacing_pt: styles.letter_spacing,
                        word_spacing_pt: styles.word_spacing,
                        base_bold: bold,
                        justify,
                        base_mono: styles.font_family.eq_ignore_ascii_case("Courier"),
                    },
                );
                self.paint_inline_lines_paginated(
                    &lines,
                    block_x,
                    node.width,
                    padding_left,
                    &styles,
                    bold,
                    padding_top_pt,
                    padding_bottom_pt,
                )
            }

            // --- Container ---
            LayoutContent::Children(child_indices) => match node.kind {
                BoxKind::Table => self.place_table(node_idx, child_indices),
                BoxKind::Grid => self.place_grid(node_idx, child_indices),
                BoxKind::List => {
                    let is_ordered = styles.list_style == ListStyle::Ordered;
                    let mut total = 0.0f32;
                    for (i, child_idx) in child_indices.into_iter().enumerate() {
                        self.list_item_counter = if is_ordered { Some(i + 1) } else { None };
                        let h = self.place_node(child_idx);
                        total += h;
                    }
                    self.list_item_counter = None;
                    total
                }
                BoxKind::ListItem => self.place_list_item_children(node_idx, child_indices),
                _ => {
                    let mut total = 0.0f32;
                    for child_idx in child_indices {
                        let h = self.place_node(child_idx);
                        total += h;
                    }
                    total
                }
            },

            LayoutContent::Empty => {
                if matches!(node.kind, BoxKind::Figure) {
                    let h = Self::figure_placeholder_height_pt(&styles);
                    self.draw_figure_placeholder(block_x, &node);
                    h
                } else if matches!(node.kind, BoxKind::Hr) {
                    let x = block_x;
                    let w = node.width.max(CONTENT_WIDTH_PT);
                    self.push_cmd(DrawCommand::Line {
                        x1: x,
                        y1: self.cursor_y,
                        x2: x + w,
                        y2: self.cursor_y,
                        color: Color::from_hex(0xCCCCCC),
                        width: 0.5,
                    });
                    0.0
                } else {
                    0.0
                }
            }
        };

        if wrap_clip {
            self.push_cmd(DrawCommand::PopClip);
        }

        if wrap_opacity {
            self.push_cmd(DrawCommand::PopOpacity);
        }

        self.cursor_y += margin_bottom;
        consumed + margin_top + margin_bottom
    }

    // ─── Multi-page text painting ─────────────────────────────────────────────

    /// Split `lines` into page-sized segments and paint each segment with its own
    /// background rect, issuing `new_page` between segments. Returns the total
    /// visual height consumed (sum of per-page segment heights).
    fn paint_text_lines_paginated(
        &mut self,
        lines: &[TextLine],
        block_x: f32,
        width_bb: f32,
        padding_left: f32,
        styles: &super::styles::ResolvedStyles,
        bold: bool,
        padding_top: f32,
        padding_bottom: f32,
    ) -> f32 {
        if lines.is_empty() {
            return 0.0;
        }
        let segments = self.compute_line_segments_with_lead(
            lines.iter().map(|l| l.line_height_pt),
            padding_top,
        );
        let italic = styles.font_style == FontStyle::Italic;
        let mut total = 0.0f32;
        for (idx, (a, b)) in segments.iter().enumerate() {
            let is_first = idx == 0;
            let is_last = idx == segments.len() - 1;
            if idx > 0 {
                self.new_page();
            }
            let seg_top = self.cursor_y;
            let seg_lead = if is_first { padding_top } else { 0.0 };
            let seg_tail = if is_last { padding_bottom } else { 0.0 };
            let seg_h: f32 = lines[*a..*b].iter().map(|l| l.line_height_pt).sum();
            if let Some(bg) = styles.background {
                self.push_cmd(DrawCommand::Rect {
                    x: block_x,
                    y: seg_top,
                    w: width_bb.max(1.0),
                    h: seg_lead + seg_h + seg_tail,
                    fill: Some(bg),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
            for (k, line) in lines[*a..*b].iter().enumerate() {
                let y = seg_top + seg_lead + styles.font_size + k as f32 * line.line_height_pt;
                self.push_cmd(DrawCommand::Text {
                    content: line.text.clone(),
                    x: block_x + padding_left,
                    y,
                    font_size: styles.font_size,
                    font_family: styles.font_family.clone(),
                    bold,
                    italic,
                    color: styles.color,
                    link_uri: None,
                    link_width_pt: None,
                });
            }
            self.cursor_y = seg_top + seg_lead + seg_h + seg_tail;
            total += seg_lead + seg_h + seg_tail;
        }
        total
    }

    fn paint_inline_lines_paginated(
        &mut self,
        lines: &[InlineLine],
        block_x: f32,
        width_bb: f32,
        padding_left: f32,
        styles: &super::styles::ResolvedStyles,
        bold: bool,
        padding_top: f32,
        padding_bottom: f32,
    ) -> f32 {
        if lines.is_empty() {
            return 0.0;
        }
        let segments = self.compute_line_segments_with_lead(
            lines.iter().map(|l| l.line_height_pt),
            padding_top,
        );
        let italic_base = styles.font_style == FontStyle::Italic;
        let mut total = 0.0f32;
        for (idx, (a, b)) in segments.iter().enumerate() {
            let is_first = idx == 0;
            let is_last = idx == segments.len() - 1;
            if idx > 0 {
                self.new_page();
            }
            let seg_top = self.cursor_y;
            let seg_lead = if is_first { padding_top } else { 0.0 };
            let seg_tail = if is_last { padding_bottom } else { 0.0 };
            let seg_h: f32 = lines[*a..*b].iter().map(|l| l.line_height_pt).sum();
            if let Some(bg) = styles.background {
                self.push_cmd(DrawCommand::Rect {
                    x: block_x,
                    y: seg_top,
                    w: width_bb.max(1.0),
                    h: seg_lead + seg_h + seg_tail,
                    fill: Some(bg),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
            for (k, line) in lines[*a..*b].iter().enumerate() {
                let baseline_y = seg_top + seg_lead + styles.font_size + k as f32 * line.line_height_pt;
                let mut x_cursor = block_x + padding_left;
                for frag in &line.fragments {
                    let font_family = if frag.code {
                        "Courier".to_string()
                    } else {
                        styles.font_family.clone()
                    };
                    self.push_cmd(DrawCommand::Text {
                        content: frag.text.clone(),
                        x: x_cursor,
                        y: baseline_y,
                        font_size: styles.font_size,
                        font_family,
                        bold: bold || frag.bold,
                        italic: italic_base || frag.italic,
                        color: if frag.link.is_some() {
                            Color::from_hex(0x1D4ED8)
                        } else {
                            styles.color
                        },
                        link_uri: frag.link.clone(),
                        link_width_pt: frag.link.as_ref().map(|_| frag.width),
                    });
                    x_cursor += frag.width;
                }
            }
            self.cursor_y = seg_top + seg_lead + seg_h + seg_tail;
            total += seg_lead + seg_h + seg_tail;
        }
        total
    }

    /// Given a sequence of per-line heights, compute (start_idx, end_idx) segment
    /// ranges so each segment fits between the current cursor_y (for the first
    /// segment) or CONTENT_TOP (for subsequent segments) and CONTENT_BOTTOM.
    /// A segment never starts empty; lines that exceed a full page are placed
    /// alone on a page (inevitable overflow).
    /// Compute `(start, end)` line ranges that fit between the current
    /// `cursor_y + lead` (first seg) or `CONTENT_TOP` (continuation segs) and
    /// `CONTENT_BOTTOM`. `lead` reserves vertical padding-top so the first line
    /// does not clip against `CONTENT_BOTTOM`. Pass `lead = 0.0` for no padding.
    fn compute_line_segments_with_lead<I: Iterator<Item = f32>>(
        &self,
        heights: I,
        lead: f32,
    ) -> Vec<(usize, usize)> {
        let heights: Vec<f32> = heights.collect();
        let mut segments: Vec<(usize, usize)> = Vec::new();
        let mut seg_start = 0usize;
        let mut sim_y = self.cursor_y + lead;
        for (i, &h) in heights.iter().enumerate() {
            if sim_y + h > CONTENT_BOTTOM && sim_y > CONTENT_TOP && i > seg_start {
                segments.push((seg_start, i));
                seg_start = i;
                sim_y = CONTENT_TOP;
            }
            sim_y += h;
        }
        segments.push((seg_start, heights.len()));
        segments
    }

    // --- List item with bullet ---

    fn place_list_item(&mut self, node_idx: LayoutNodeIdx, text: &str) -> f32 {
        let node = self.layout.nodes[node_idx].clone();
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;

        let width = text_container_width_pt(node.width, styles.padding.left, styles.padding.right);
        let lines = break_text(
            text,
            width,
            styles.font_size,
            styles.line_height,
            bold,
            styles.letter_spacing,
            styles.word_spacing,
        );
        let block_h = text_block_height(&lines);

        if self.cursor_y + block_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
            self.new_page();
        }

        // Bullet "•" or "1." to the left of text start
        let bullet_text = match self.list_item_counter {
            Some(n) => format!("{}.", n),
            None => "\u{2022}".to_string(),
        };
        let bullet_x = node.x - 4.5 * MM_TO_PT;
        let bullet_y = self.cursor_y + styles.font_size;
        self.push_cmd(DrawCommand::Text {
            content: bullet_text,
            x: bullet_x,
            y: bullet_y,
            font_size: styles.font_size,
            font_family: styles.font_family.clone(),
            bold: false,
            italic: false,
            color: styles.color,
            link_uri: None,
            link_width_pt: None,
        });

        for (i, line) in lines.iter().enumerate() {
            let y = self.cursor_y + styles.font_size + i as f32 * line.line_height_pt;
            if y > CONTENT_BOTTOM {
                break;
            }
            self.push_cmd(DrawCommand::Text {
                content: line.text.clone(),
                x: node.x,
                y,
                font_size: styles.font_size,
                font_family: styles.font_family.clone(),
                bold,
                italic: styles.font_style == FontStyle::Italic,
                color: styles.color,
                link_uri: None,
                link_width_pt: None,
            });
        }

        self.cursor_y += block_h;
        block_h
    }

    /// `ITEM(P(...))` and similar: layout has children, not direct text — draw marker + body.
    fn place_list_item_children(
        &mut self,
        list_item_idx: LayoutNodeIdx,
        child_indices: Vec<LayoutNodeIdx>,
    ) -> f32 {
        if child_indices.is_empty() {
            return 0.0;
        }
        if child_indices.len() == 1 {
            let child_idx = child_indices[0];
            match &self.layout.nodes[child_idx].content.clone() {
                LayoutContent::Text(t) => return self.place_list_item(list_item_idx, t),
                LayoutContent::Inline(runs) => {
                    return self.place_list_item_inline(list_item_idx, runs);
                }
                _ => {}
            }
        }
        let mut total = 0.0f32;
        for child_idx in child_indices {
            total += self.place_node(child_idx);
        }
        total
    }

    fn place_list_item_inline(&mut self, list_item_idx: LayoutNodeIdx, runs: &[InlineRun]) -> f32 {
        let node = self.layout.nodes[list_item_idx].clone();
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;
        let width = text_container_width_pt(node.width, styles.padding.left, styles.padding.right);
        let justify = styles.justify || styles.text_align == super::styles::TextAlign::Justify;
        let lines = break_inline_runs(
            runs,
            width,
            &TextLayoutOpts {
                font_size_pt: styles.font_size,
                line_height: styles.line_height,
                letter_spacing_pt: styles.letter_spacing,
                word_spacing_pt: styles.word_spacing,
                base_bold: bold,
                justify,
                base_mono: styles.font_family.eq_ignore_ascii_case("Courier"),
            },
        );
        let block_h = inline_lines_block_height(&lines, styles.font_size, styles.line_height);

        if self.cursor_y + block_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
            self.new_page();
        }

        let bullet_text = match self.list_item_counter {
            Some(n) => format!("{}.", n),
            None => "\u{2022}".to_string(),
        };
        let bullet_x = node.x - 4.5 * MM_TO_PT;
        let bullet_y = self.cursor_y + styles.font_size;
        self.push_cmd(DrawCommand::Text {
            content: bullet_text,
            x: bullet_x,
            y: bullet_y,
            font_size: styles.font_size,
            font_family: styles.font_family.clone(),
            bold: false,
            italic: false,
            color: styles.color,
            link_uri: None,
            link_width_pt: None,
        });

        for (line_idx, line) in lines.iter().enumerate() {
            let baseline_y =
                self.cursor_y + styles.font_size + line_idx as f32 * line.line_height_pt;
            if baseline_y > CONTENT_BOTTOM {
                break;
            }
            let mut x_cursor = node.x;
            for frag in &line.fragments {
                let frag_font_family = if frag.code {
                    "Courier".to_string()
                } else {
                    styles.font_family.clone()
                };
                self.push_cmd(DrawCommand::Text {
                    content: frag.text.clone(),
                    x: x_cursor,
                    y: baseline_y,
                    font_size: styles.font_size,
                    font_family: frag_font_family,
                    bold: bold || frag.bold,
                    italic: (styles.font_style == FontStyle::Italic) || frag.italic,
                    color: if frag.link.is_some() {
                        Color::from_hex(0x1D4ED8)
                    } else {
                        styles.color
                    },
                    link_uri: frag.link.clone(),
                    link_width_pt: frag.link.as_ref().map(|_| frag.width),
                });
                if frag.link.is_some() && !frag.text.trim().is_empty() {
                    let underline_y = baseline_y + 1.0;
                    self.push_cmd(DrawCommand::Line {
                        x1: x_cursor,
                        y1: underline_y,
                        x2: x_cursor + frag.width.max(0.0),
                        y2: underline_y,
                        color: Color::from_hex(0x1D4ED8),
                        width: 0.5,
                    });
                }
                x_cursor += frag.width;
            }
        }

        self.cursor_y += block_h;
        block_h
    }

    // ─── GRID ─────────────────────────────────────────────────────────────────

    fn place_grid(&mut self, grid_idx: LayoutNodeIdx, child_indices: Vec<LayoutNodeIdx>) -> f32 {
        let grid_node = self.layout.nodes[grid_idx].clone();
        let grid_styles = self.styled.get(grid_node.arena_id).styles.clone();
        let cols = grid_styles.grid_column_count().max(1);
        let grid_start_y = self.cursor_y;

        for row_cells in child_indices.chunks(cols) {
            if row_cells.is_empty() {
                continue;
            }

            // Row page-break: if max cell height doesn't fit in remaining page,
            // break before the row. Uses `estimate_height` (taffy layout height as
            // fallback) to keep a row intact across page boundaries.
            let row_est_h = row_cells
                .iter()
                .map(|&ci| {
                    let h_est = self.estimate_height(ci);
                    h_est.max(self.layout.nodes[ci].height)
                })
                .fold(0.0f32, f32::max);
            if self.cursor_y + row_est_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
                self.new_page();
            }

            let row_start_y = self.cursor_y;
            let row_min_y = row_cells
                .iter()
                .map(|&ci| self.layout.nodes[ci].y)
                .fold(f32::INFINITY, f32::min);
            let mut max_bottom = row_start_y;

            for &ci in row_cells {
                let cell_y = self.layout.nodes[ci].y;
                let dy = cell_y - row_min_y;
                self.cursor_y = row_start_y + dy;
                let top_before = self.cursor_y;
                self.place_node(ci);
                max_bottom = max_bottom.max(self.cursor_y);
                // Ensure row height covers taffy vertical offset + painted extent
                max_bottom = max_bottom.max(top_before + (self.layout.nodes[ci].height).max(0.0));
            }

            self.cursor_y = max_bottom;
        }

        self.cursor_y - grid_start_y
    }

    // ─── TABLE ────────────────────────────────────────────────────────────────

    fn place_table(&mut self, table_idx: LayoutNodeIdx, row_indices: Vec<LayoutNodeIdx>) -> f32 {
        let table_node = self.layout.nodes[table_idx].clone();
        let table_styles = self.styled.get(table_node.arena_id).styles.clone();
        let table_start_y = self.cursor_y;
        let table_x = table_node.x;
        let table_w = table_node.width.max(1.0);

        // Column count: max cells across rows. With colspan the effective span
        // count can exceed this on a row, but unspanned rows still define the
        // baseline track count.
        let num_cols = row_indices
            .iter()
            .map(|&ri| {
                match &self.layout.nodes[ri].content {
                    LayoutContent::Children(cells) => cells
                        .iter()
                        .map(|&ci| {
                            self.styled
                                .get(self.layout.nodes[ci].arena_id)
                                .styles
                                .cell_span
                                .max(1)
                        })
                        .sum::<usize>(),
                    _ => 0,
                }
            })
            .max()
            .unwrap_or(0);

        // Column widths:
        // - TABLE `{cols: ...}` / `{columns: ...}` track list (fr + fixed) → solved against table_w.
        // - Otherwise → equal split.
        let col_widths = compute_table_col_widths(
            &table_styles.grid_column_tracks,
            num_cols,
            table_w,
        );

        for (row_num, row_idx) in row_indices.into_iter().enumerate() {
            let row_node = self.layout.nodes[row_idx].clone();
            let row_styles = self.styled.get(row_node.arena_id).styles.clone();

            let cell_indices = match &row_node.content {
                LayoutContent::Children(cells) => cells.clone(),
                _ => vec![],
            };

            if cell_indices.is_empty() {
                continue;
            }

            // Resolve (col_idx, cell_w) per cell, honoring `cell_span`. Shared by
            // the child rect_override pass, row-height estimate, and paint loop so
            // all three agree when a cell spans >1 columns.
            let cell_plan: Vec<(usize, f32)> = {
                let mut plan = Vec::with_capacity(cell_indices.len());
                let mut col_idx = 0usize;
                for &ci in &cell_indices {
                    let span = self
                        .styled
                        .get(self.layout.nodes[ci].arena_id)
                        .styles
                        .cell_span
                        .max(1);
                    let end = (col_idx + span).min(col_widths.len().max(col_idx + 1));
                    let mut w = 0.0f32;
                    for k in col_idx..end {
                        if let Some(&cw) = col_widths.get(k) {
                            w += cw;
                        }
                    }
                    if w <= 0.0 {
                        w = self.layout.nodes[ci].width;
                    }
                    plan.push((col_idx, w.max(1.0)));
                    col_idx += span;
                }
                plan
            };

            // Taffy runs flex layout per ROW, producing different per-row cell widths
            // and x positions when intrinsic content differs. `col_widths` is the
            // normalized cross-row column layout; snap each cell's direct children
            // (e.g. paragraphs) to the column rect so text renders at the column,
            // not at the per-row taffy x. Also fixes text wrapping and row-height
            // estimation below since both read node width.
            for (&ci, &(col_idx, cell_w)) in cell_indices.iter().zip(cell_plan.iter()) {
                let prefix_x = table_x
                    + col_widths.iter().take(col_idx).copied().sum::<f32>();
                if let LayoutContent::Children(children) =
                    self.layout.nodes[ci].content.clone()
                {
                    let cell_styles =
                        self.styled.get(self.layout.nodes[ci].arena_id).styles.clone();
                    let pl = cell_styles.padding.left * MM_TO_PT;
                    let pr = cell_styles.padding.right * MM_TO_PT;
                    let inner_x = prefix_x + pl;
                    let inner_w = (cell_w - pl - pr).max(1.0);
                    for child_idx in children {
                        self.rect_override.insert(child_idx, (inner_x, inner_w));
                    }
                }
            }

            let row_min_cell_y = cell_indices
                .iter()
                .map(|&ci| self.layout.nodes[ci].y)
                .fold(f32::INFINITY, f32::min);

            // Row height = max over cells. `nowrap`/`truncate` force a single-line
            // height so the cell stays compact regardless of content length.
            let row_height = cell_indices
                .iter()
                .zip(cell_plan.iter())
                .map(|(&ci, &(_col_idx, cell_w))| {
                    let cell = &self.layout.nodes[ci];
                    let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                    let bold = cell_styles.font_weight == FontWeight::Bold;
                    let pad_v = cell_styles.padding.top * MM_TO_PT
                        + cell_styles.padding.bottom * MM_TO_PT;
                    let one_line_h = cell_styles.font_size * cell_styles.line_height;
                    match &cell.content {
                        LayoutContent::Text(t) => {
                            if cell_styles.nowrap || cell_styles.truncate {
                                one_line_h + pad_v
                            } else {
                                let w = text_container_width_pt(
                                    cell_w,
                                    cell_styles.padding.left,
                                    cell_styles.padding.right,
                                );
                                let lines = break_text(
                                    t,
                                    w,
                                    cell_styles.font_size,
                                    cell_styles.line_height,
                                    bold,
                                    cell_styles.letter_spacing,
                                    cell_styles.word_spacing,
                                );
                                text_block_height(&lines) + pad_v
                            }
                        }
                        LayoutContent::Inline(runs) => {
                            if cell_styles.nowrap || cell_styles.truncate {
                                one_line_h + pad_v
                            } else {
                                let w = text_container_width_pt(
                                    cell_w,
                                    cell_styles.padding.left,
                                    cell_styles.padding.right,
                                );
                                let justify = cell_styles.justify
                                    || cell_styles.text_align == TextAlign::Justify;
                                inline_runs_block_height(
                                    runs,
                                    w,
                                    &TextLayoutOpts {
                                        font_size_pt: cell_styles.font_size,
                                        line_height: cell_styles.line_height,
                                        letter_spacing_pt: cell_styles.letter_spacing,
                                        word_spacing_pt: cell_styles.word_spacing,
                                        base_bold: bold,
                                        justify,
                                        base_mono: cell_styles
                                            .font_family
                                            .eq_ignore_ascii_case("Courier"),
                                    },
                                ) + pad_v
                            }
                        }
                        LayoutContent::Children(children) => {
                            children
                                .iter()
                                .map(|&child_idx| self.estimate_height(child_idx))
                                .sum::<f32>()
                                + pad_v
                        }
                        LayoutContent::Empty => 0.0,
                    }
                })
                .fold(0.0f32, f32::max)
                .max(12.0);

            let page_before_break = self.pages.len();
            if !row_styles.allow_row_overflow
                && self.cursor_y + row_height > CONTENT_BOTTOM
                && self.cursor_y > CONTENT_TOP
            {
                self.new_page();
            }
            let row_start_y = self.cursor_y;

            // Top border line: drawn before first row, or again after a page break
            // to give the continuation a visible top edge. This replaces the
            // previous outer-border rect which could not span multiple pages.
            let is_first_row_on_page = row_num == 0 || self.pages.len() > page_before_break;
            if is_first_row_on_page {
                self.push_cmd(DrawCommand::Line {
                    x1: table_x,
                    y1: row_start_y,
                    x2: table_x + table_w,
                    y2: row_start_y,
                    color: Color::from_hex(0xCCCCCC),
                    width: 0.5,
                });
            }

            // Row background: explicit row color OR default header tint
            let row_bg = row_styles.background.or_else(|| {
                if row_num == 0 {
                    Some(Color::from_hex(0xF5F5F5))
                } else {
                    None
                }
            });

            if let Some(bg) = row_bg {
                self.push_cmd(DrawCommand::Rect {
                    x: table_x,
                    y: row_start_y,
                    w: table_w,
                    h: row_height,
                    fill: Some(bg),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }

            // Row cells
            for (&cell_idx, &(col_idx, cell_w)) in
                cell_indices.iter().zip(cell_plan.iter())
            {
                let cell_x = table_x
                    + col_widths.iter().take(col_idx).copied().sum::<f32>();
                let cell = self.layout.nodes[cell_idx].clone();
                let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                let bold = cell_styles.font_weight == FontWeight::Bold;
                let padding_top = cell_styles.padding.top * MM_TO_PT;
                let padding_bottom = cell_styles.padding.bottom * MM_TO_PT;
                let padding_left = cell_styles.padding.left * MM_TO_PT;

                // Effective horizontal align:
                // explicit cell `align` > TABLE `col_aligns[col_idx]` > inherited/default.
                let effective_align = cell_styles
                    .explicit_text_align
                    .or_else(|| table_styles.col_aligns.get(col_idx).copied())
                    .unwrap_or(cell_styles.text_align);

                let inner_w = text_container_width_pt(
                    cell_w,
                    cell_styles.padding.left,
                    cell_styles.padding.right,
                );

                // Per-cell background
                if let Some(cell_bg) = cell_styles.background {
                    self.push_cmd(DrawCommand::Rect {
                        x: cell_x,
                        y: row_start_y,
                        w: cell_w,
                        h: row_height,
                        fill: Some(cell_bg),
                        stroke: None,
                        stroke_width: 0.0,
                    });
                }

                // Lay out text lines first (so `content_h` is known for vertical align).
                // For nowrap/truncate we build a single line manually and skip break_*.
                enum CellLines {
                    Plain(Vec<TextLine>),
                    Inline(Vec<InlineLine>),
                    Children,
                    None,
                }

                let cell_lines = match &cell.content {
                    LayoutContent::Text(text) => {
                        if cell_styles.nowrap {
                            let t = text.replace('\n', " ");
                            let w = text_width_pt_with_spacing(
                                &t,
                                cell_styles.font_size,
                                bold,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                            );
                            CellLines::Plain(vec![TextLine {
                                text: t,
                                width: w,
                                line_height_pt: cell_styles.font_size * cell_styles.line_height,
                                font_size: cell_styles.font_size,
                            }])
                        } else if cell_styles.truncate {
                            let t = text.replace('\n', " ");
                            let clipped = truncate_to_width(
                                &t,
                                inner_w,
                                cell_styles.font_size,
                                bold,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                            );
                            let w = text_width_pt_with_spacing(
                                &clipped,
                                cell_styles.font_size,
                                bold,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                            );
                            CellLines::Plain(vec![TextLine {
                                text: clipped,
                                width: w,
                                line_height_pt: cell_styles.font_size * cell_styles.line_height,
                                font_size: cell_styles.font_size,
                            }])
                        } else {
                            CellLines::Plain(break_text(
                                text,
                                inner_w,
                                cell_styles.font_size,
                                cell_styles.line_height,
                                bold,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                            ))
                        }
                    }
                    LayoutContent::Inline(runs) => {
                        let justify = cell_styles.justify
                            || cell_styles.text_align == TextAlign::Justify;
                        let base_mono_cell =
                            cell_styles.font_family.eq_ignore_ascii_case("Courier");
                        if cell_styles.nowrap {
                            // Ask break_inline_runs for a very wide width so it never wraps.
                            CellLines::Inline(break_inline_runs(
                                runs,
                                f32::MAX / 4.0,
                                &TextLayoutOpts {
                                    font_size_pt: cell_styles.font_size,
                                    line_height: cell_styles.line_height,
                                    letter_spacing_pt: cell_styles.letter_spacing,
                                    word_spacing_pt: cell_styles.word_spacing,
                                    base_bold: bold,
                                    justify: false,
                                    base_mono: base_mono_cell,
                                },
                            ))
                        } else if cell_styles.truncate {
                            let all = break_inline_runs(
                                runs,
                                f32::MAX / 4.0,
                                &TextLayoutOpts {
                                    font_size_pt: cell_styles.font_size,
                                    line_height: cell_styles.line_height,
                                    letter_spacing_pt: cell_styles.letter_spacing,
                                    word_spacing_pt: cell_styles.word_spacing,
                                    base_bold: bold,
                                    justify: false,
                                    base_mono: base_mono_cell,
                                },
                            );
                            let mut first_line = all.into_iter().next();
                            if let Some(line) = first_line.as_mut() {
                                truncate_inline_line(
                                    line,
                                    inner_w,
                                    cell_styles.font_size,
                                    bold,
                                    cell_styles.letter_spacing,
                                    cell_styles.word_spacing,
                                );
                            }
                            CellLines::Inline(first_line.into_iter().collect())
                        } else {
                            CellLines::Inline(break_inline_runs(
                                runs,
                                inner_w,
                                &TextLayoutOpts {
                                    font_size_pt: cell_styles.font_size,
                                    line_height: cell_styles.line_height,
                                    letter_spacing_pt: cell_styles.letter_spacing,
                                    word_spacing_pt: cell_styles.word_spacing,
                                    base_bold: bold,
                                    justify,
                                    base_mono: base_mono_cell,
                                },
                            ))
                        }
                    }
                    LayoutContent::Children(_) => CellLines::Children,
                    LayoutContent::Empty => CellLines::None,
                };

                // Content height for vertical-align shift.
                let content_h = match &cell_lines {
                    CellLines::Plain(lines) => text_block_height(lines),
                    CellLines::Inline(lines) => inline_lines_block_height(
                        lines,
                        cell_styles.font_size,
                        cell_styles.line_height,
                    ),
                    CellLines::Children => row_height - padding_top - padding_bottom,
                    CellLines::None => 0.0,
                };
                let available_h = (row_height - padding_top - padding_bottom).max(0.0);
                let extra = (available_h - content_h).max(0.0);
                let valign_shift = match cell_styles.vertical_align {
                    VerticalAlign::Top => 0.0,
                    VerticalAlign::Middle => extra * 0.5,
                    VerticalAlign::Bottom => extra,
                };

                // Keep per-row y alignment for nested children content (legacy behavior).
                let cell_dy = if matches!(cell_lines, CellLines::Children) {
                    cell.y - row_min_cell_y
                } else {
                    0.0
                };
                let content_top = row_start_y + padding_top + cell_dy + valign_shift;

                match cell_lines {
                    CellLines::Plain(lines) => {
                        for (i, line) in lines.iter().enumerate() {
                            let y = content_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if y > CONTENT_BOTTOM {
                                break;
                            }
                            let x_off = align_offset(effective_align, inner_w, line.width);
                            self.push_cmd(DrawCommand::Text {
                                content: line.text.clone(),
                                x: cell_x + padding_left + x_off,
                                y,
                                font_size: cell_styles.font_size,
                                font_family: cell_styles.font_family.clone(),
                                bold,
                                italic: cell_styles.font_style == FontStyle::Italic,
                                color: cell_styles.color,
                                link_uri: None,
                                link_width_pt: None,
                            });
                        }
                    }
                    CellLines::Inline(lines) => {
                        for (i, line) in lines.iter().enumerate() {
                            let baseline_y = content_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if baseline_y > CONTENT_BOTTOM {
                                break;
                            }
                            let x_off = align_offset(effective_align, inner_w, line.width);
                            let mut x_cursor = cell_x + padding_left + x_off;
                            for frag in &line.fragments {
                                let font_family = if frag.code {
                                    "Courier".to_string()
                                } else {
                                    cell_styles.font_family.clone()
                                };
                                self.push_cmd(DrawCommand::Text {
                                    content: frag.text.clone(),
                                    x: x_cursor,
                                    y: baseline_y,
                                    font_size: cell_styles.font_size,
                                    font_family,
                                    bold: bold || frag.bold,
                                    italic: (cell_styles.font_style == FontStyle::Italic)
                                        || frag.italic,
                                    color: if frag.link.is_some() {
                                        Color::from_hex(0x1D4ED8)
                                    } else {
                                        cell_styles.color
                                    },
                                    link_uri: frag.link.clone(),
                                    link_width_pt: frag.link.as_ref().map(|_| frag.width),
                                });
                                x_cursor += frag.width;
                            }
                        }
                    }
                    CellLines::Children => {
                        let children = match &cell.content {
                            LayoutContent::Children(cs) => cs.clone(),
                            _ => Vec::new(),
                        };
                        let cursor_before_cell = self.cursor_y;
                        // Layout `y` is document-absolute (PAGE roots stack in `compute_layout`).
                        // Row painting uses page-local `content_top` for Text/Inline; Children must
                        // use the same origin: top of cell content + relative offset inside cell.
                        let children_min_y = children
                            .iter()
                            .map(|&ci| self.layout.nodes[ci].y)
                            .fold(f32::INFINITY, f32::min);
                        for &child_idx in &children {
                            let child = &self.layout.nodes[child_idx];
                            let child_styles = self.styled.get(child.arena_id).styles.clone();
                            let m_top = child_styles.margin.top * MM_TO_PT;
                            let rel_y = child.y - children_min_y;
                            self.cursor_y = content_top + rel_y - m_top;
                            self.place_node(child_idx);
                        }
                        self.cursor_y = cursor_before_cell;
                    }
                    CellLines::None => {}
                }
            }

            self.cursor_y += row_height;

            // Separator below row
            self.push_cmd(DrawCommand::Line {
                x1: table_x,
                y1: self.cursor_y,
                x2: table_x + table_w,
                y2: self.cursor_y,
                color: Color::from_hex(0xDDDDDD),
                width: 0.3,
            });
        }

        // No outer border rect: it cannot span multiple pages. Per-row separator
        // lines and the per-row top border already delimit the table visually.
        self.cursor_y - table_start_y
    }

    // --- Block height estimate (background rects) ---

    fn estimate_height(&self, node_idx: LayoutNodeIdx) -> f32 {
        let node_owned;
        let node: &LayoutBox = if self.rect_override.contains_key(&node_idx) {
            node_owned = self.node_with_override(node_idx);
            &node_owned
        } else {
            &self.layout.nodes[node_idx]
        };
        let styles = self.styled.get(node.arena_id).styles.clone();
        let margin_top = styles.margin.top * MM_TO_PT;
        let margin_bottom = styles.margin.bottom * MM_TO_PT;

        let inner = if matches!(node.kind, BoxKind::Figure) && matches!(node.content, LayoutContent::Empty)
        {
            Self::figure_placeholder_height_pt(&styles)
        } else {
            let bold = styles.font_weight == FontWeight::Bold;
            match &node.content {
                LayoutContent::Text(t) => {
                    let w = text_container_width_pt(
                        node.width,
                        styles.padding.left,
                        styles.padding.right,
                    );
                    let lines = break_text(
                        t,
                        w,
                        styles.font_size,
                        styles.line_height,
                        bold,
                        styles.letter_spacing,
                        styles.word_spacing,
                    );
                    text_block_height(&lines)
                }
                LayoutContent::Inline(runs) => {
                    let w = text_container_width_pt(
                        node.width,
                        styles.padding.left,
                        styles.padding.right,
                    );
                    let justify =
                        styles.justify || styles.text_align == super::styles::TextAlign::Justify;
                    inline_runs_block_height(
                        runs,
                        w,
                        &TextLayoutOpts {
                            font_size_pt: styles.font_size,
                            line_height: styles.line_height,
                            letter_spacing_pt: styles.letter_spacing,
                            word_spacing_pt: styles.word_spacing,
                            base_bold: bold,
                            justify,
                            base_mono: styles.font_family.eq_ignore_ascii_case("Courier"),
                        },
                    )
                }
                LayoutContent::Children(children) => {
                    children.iter().map(|&ci| self.estimate_height(ci)).sum()
                }
                LayoutContent::Empty => 0.0,
            }
        };

        margin_top + inner + margin_bottom
    }
}

// --- Cell-paint helpers ---

fn align_offset(align: TextAlign, inner_w: f32, line_w: f32) -> f32 {
    if line_w >= inner_w {
        return 0.0;
    }
    match align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Center => (inner_w - line_w) * 0.5,
        TextAlign::Right => inner_w - line_w,
    }
}

/// Clip `text` so its width + ellipsis fits into `max_w`. If the text already
/// fits, return it as-is. O(n^2) over char count, but n is small for cells.
fn truncate_to_width(
    text: &str,
    max_w: f32,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> String {
    let full_w = text_width_pt_with_spacing(
        text,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
    );
    if full_w <= max_w {
        return text.to_string();
    }
    let ell = "…";
    let ell_w = text_width_pt_with_spacing(
        ell,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
    );
    if ell_w >= max_w {
        return String::new();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        let candidate: String = chars.iter().collect();
        let w = text_width_pt_with_spacing(
            &candidate,
            font_size_pt,
            bold,
            letter_spacing_pt,
            word_spacing_pt,
        );
        if w + ell_w <= max_w {
            let trimmed = candidate.trim_end();
            return format!("{}{}", trimmed, ell);
        }
        chars.pop();
    }
    ell.to_string()
}

/// Largest char prefix of `text` whose rendered width ≤ `max_w`.
/// Returns the prefix + its measured width. Caller appends ellipsis separately.
fn fit_text_prefix(
    text: &str,
    max_w: f32,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> (String, f32) {
    let full_w = text_width_pt_with_spacing(
        text,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
    );
    if full_w <= max_w {
        return (text.to_string(), full_w);
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        let candidate: String = chars.iter().collect();
        let w = text_width_pt_with_spacing(
            &candidate,
            font_size_pt,
            bold,
            letter_spacing_pt,
            word_spacing_pt,
        );
        if w <= max_w {
            return (candidate, w);
        }
        chars.pop();
    }
    (String::new(), 0.0)
}

/// Clip an `InlineLine` in-place so total fragment width + ellipsis ≤ `max_w`.
/// Fragment styles are preserved. Truncation always terminates with a trailing
/// `…` fragment so the visual cue is consistent regardless of which fragment
/// boundary ran out of budget.
fn truncate_inline_line(
    line: &mut InlineLine,
    max_w: f32,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) {
    if line.width <= max_w {
        return;
    }
    let ell = "…";
    let ell_w = text_width_pt_with_spacing(
        ell,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
    );
    if ell_w >= max_w {
        line.fragments.clear();
        line.width = 0.0;
        line.full_text.clear();
        return;
    }

    let mut acc_w = 0.0f32;
    let mut kept: Vec<super::text::InlineFragment> = Vec::new();
    for frag in line.fragments.clone().into_iter() {
        if acc_w + frag.width + ell_w <= max_w {
            acc_w += frag.width;
            kept.push(frag);
            continue;
        }
        let budget = (max_w - acc_w - ell_w).max(0.0);
        if budget > 0.0 {
            let bold_frag = bold || frag.bold;
            let (prefix, w) = fit_text_prefix(
                &frag.text,
                budget,
                font_size_pt,
                bold_frag,
                letter_spacing_pt,
                word_spacing_pt,
            );
            if !prefix.is_empty() {
                let mut clipped = frag.clone();
                clipped.text = prefix;
                clipped.width = w;
                acc_w += w;
                kept.push(clipped);
            }
        }
        break;
    }

    kept.push(super::text::InlineFragment {
        text: ell.to_string(),
        bold: false,
        italic: false,
        code: false,
        link: None,
        width: ell_w,
    });
    line.width = acc_w + ell_w;
    line.full_text = kept.iter().map(|f| f.text.as_str()).collect();
    line.fragments = kept;
}

// --- Table column-width solver ---

/// Resolve a TABLE's column widths against `table_w`.
///
/// - `tracks` empty → equal split (existing memory decision:
///   "TABLE default = equal column widths").
/// - `tracks` present:
///   - `LengthPt` tracks consume fixed pt first,
///   - `Fr` / `Auto` tracks share the remaining width proportionally
///     (Auto counted as `1fr` for now; no intrinsic content sizing),
///   - If `tracks.len() != num_cols`, pad with `Fr(1.0)` or truncate.
/// - Fallback: if all tracks are fixed and sum differs from `table_w`,
///   scale uniformly to match.
fn compute_table_col_widths(
    tracks: &[GridColumnTrack],
    num_cols: usize,
    table_w: f32,
) -> Vec<f32> {
    if num_cols == 0 {
        return Vec::new();
    }
    if tracks.is_empty() {
        return vec![table_w / num_cols as f32; num_cols];
    }

    let mut tracks: Vec<GridColumnTrack> = tracks.to_vec();
    if tracks.len() < num_cols {
        tracks.extend(std::iter::repeat_n(GridColumnTrack::Fr(1.0), num_cols - tracks.len()));
    } else if tracks.len() > num_cols {
        tracks.truncate(num_cols);
    }

    let mut widths = vec![0.0f32; num_cols];
    let mut fixed_total = 0.0f32;
    let mut fr_total = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        match *t {
            GridColumnTrack::LengthPt(pt) => {
                widths[i] = pt.max(0.0);
                fixed_total += pt.max(0.0);
            }
            GridColumnTrack::Fr(f) => fr_total += f.max(0.0),
            GridColumnTrack::Auto => fr_total += 1.0,
        }
    }

    if fr_total > 0.0 {
        let remaining = (table_w - fixed_total).max(0.0);
        for (i, t) in tracks.iter().enumerate() {
            match *t {
                GridColumnTrack::Fr(f) => widths[i] = remaining * (f.max(0.0) / fr_total),
                GridColumnTrack::Auto => widths[i] = remaining * (1.0 / fr_total),
                _ => {}
            }
        }
    } else if fixed_total > 0.5 && (fixed_total - table_w).abs() > 0.5 {
        let scale = table_w / fixed_total;
        widths.iter_mut().for_each(|w| *w *= scale);
    }

    widths
}

// --- Public API ---

pub fn paginate(layout: &LayoutTree, styled: &super::arena::DocumentArena) -> PageTree {
    let mut pager = Paginator::new(layout, styled);

    // One layout root per top-level PAGE block: each root must start on a new physical page,
    // even when the previous page has free vertical space (see `engine::pipeline_tests`).
    for (root_num, &root_idx) in layout.roots.iter().enumerate() {
        let node = &layout.nodes[root_idx];
        let root_styles = styled.get(node.arena_id).styles.clone();
        pager.page_header = root_styles.page_header.clone();
        pager.page_footer = root_styles.page_footer.clone();
        if root_num == 0 {
            pager.draw_page_chrome();
        } else {
            pager.new_page();
        }
        match &node.content {
            LayoutContent::Children(children) => {
                let children = children.clone();
                for (idx, child_idx) in children.iter().enumerate() {
                    let child_node = &layout.nodes[*child_idx];
                    let child_styles = styled.get(child_node.arena_id).styles.clone();
                    if child_styles.keep_with_next && idx + 1 < children.len() {
                        let h1 = pager.estimate_height(*child_idx);
                        let h2 = pager.estimate_height(children[idx + 1]);
                        let remaining = CONTENT_BOTTOM - pager.cursor_y;
                        if h1 + h2 > remaining && pager.cursor_y > CONTENT_TOP {
                            pager.new_page();
                        }
                    } else if child_styles.keep_together {
                        let h = pager.estimate_height(*child_idx);
                        if h > (CONTENT_BOTTOM - pager.cursor_y) && pager.cursor_y > CONTENT_TOP {
                            pager.new_page();
                        }
                    }
                    pager.place_node(*child_idx);
                }
            }
            _ => {
                pager.place_node(root_idx);
            }
        }
    }

    PageTree {
        pages: pager.pages,
        block_start_page: pager.block_start_page,
        anchor_positions: pager.anchor_positions,
        text_units: consolidate_text_units(pager.text_units),
    }
}

/// Inline styling splits a painted line into several `DrawCommand::Text`
/// fragments (per bold/italic/color run, plus a separate glyph per space). For
/// Cmd+F we want one searchable line per text unit so `"hello page"` matches
/// across runs. Fragments are grouped by `(page, y_rounded, block_id)`, sorted
/// by `x`, and concatenated into a single TextUnit spanning their extents.
fn consolidate_text_units(units: Vec<TextUnit>) -> Vec<TextUnit> {
    if units.is_empty() {
        return units;
    }
    // Group by line bucket.
    let mut groups: std::collections::HashMap<(u32, i32, String), Vec<TextUnit>> =
        std::collections::HashMap::new();
    for u in units {
        let y_bucket = (u.y * 10.0).round() as i32;
        let key = (u.page, y_bucket, u.block_id.clone());
        groups.entry(key).or_default().push(u);
    }
    let mut out: Vec<TextUnit> = groups
        .into_values()
        .map(|mut frags| {
            frags.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            let first = &frags[0];
            let min_x = frags
                .iter()
                .map(|f| f.x)
                .fold(f32::INFINITY, f32::min);
            let max_right = frags
                .iter()
                .map(|f| f.x + f.w)
                .fold(f32::NEG_INFINITY, f32::max);
            let max_h = frags
                .iter()
                .map(|f| f.h)
                .fold(0.0_f32, f32::max);
            let min_y = frags
                .iter()
                .map(|f| f.y)
                .fold(f32::INFINITY, f32::min);
            let text: String = frags.iter().map(|f| f.text.as_str()).collect();
            TextUnit {
                page: first.page,
                x: min_x,
                y: min_y,
                w: (max_right - min_x).max(0.0),
                h: max_h,
                text,
                block_id: first.block_id.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.page
            .cmp(&b.page)
            .then_with(|| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::arena::DocumentArena;
    use crate::engine::layout::LayoutBox;
    use crate::engine::styles::{BoxContent, BoxKind, InlineRun, ResolvedStyles, StyledBox};

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn opacity_wrap_emits_push_pop_around_content() {
        let mut arena = DocumentArena::new();

        let mut p_styles = ResolvedStyles::for_kind(&BoxKind::Paragraph);
        p_styles.opacity = 0.4;

        let p_id = arena.alloc(StyledBox {
            id: "p-1".to_string(),
            kind: BoxKind::Paragraph,
            styles: p_styles,
            content: BoxContent::Text("x".to_string()),
        });

        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![p_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 200.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: p_id,
                    kind: BoxKind::Paragraph,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 20.0,
                    content: LayoutContent::Text("x".to_string()),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        let cmds = &pages.pages[0].commands;
        assert!(
            cmds.iter().any(|c| matches!(c, DrawCommand::PushOpacity { alpha } if (*alpha - 0.4).abs() < 1e-4)),
            "expected PushOpacity(0.4), got {cmds:?}"
        );
        assert!(cmds.iter().any(|c| matches!(c, DrawCommand::PopOpacity)));
    }

    #[test]
    fn list_item_opacity_and_clip_stacks_balance_after_place_list_item() {
        let mut arena = DocumentArena::new();

        let mut item_styles = ResolvedStyles::for_kind(&BoxKind::ListItem);
        item_styles.opacity = 0.5;
        item_styles.overflow_clip = true;

        let item_id = arena.alloc(StyledBox {
            id: "item-1".to_string(),
            kind: BoxKind::ListItem,
            styles: item_styles,
            content: BoxContent::Text("li".to_string()),
        });

        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![item_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 200.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: item_id,
                    kind: BoxKind::ListItem,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 40.0,
                    content: LayoutContent::Text("li".to_string()),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        let cmds = &pages.pages[0].commands;

        let push_o = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::PushOpacity { .. }))
            .count();
        let pop_o = cmds.iter().filter(|c| matches!(c, DrawCommand::PopOpacity)).count();
        let push_c = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::PushClipRect { .. }))
            .count();
        let pop_c = cmds.iter().filter(|c| matches!(c, DrawCommand::PopClip)).count();
        assert_eq!(push_o, pop_o, "opacity q/Q stack: {cmds:?}");
        assert_eq!(push_c, pop_c, "clip stack: {cmds:?}");
    }

    #[test]
    fn place_node_background_uses_node_width_for_narrow_cells() {
        let mut arena = DocumentArena::new();

        let mut cell_styles = ResolvedStyles::for_kind(&BoxKind::Cell);
        let bg = Color::from_hex(0xD1D5DB);
        cell_styles.background = Some(bg);

        let cell_id = arena.alloc(StyledBox {
            id: "cell-1".to_string(),
            kind: BoxKind::Cell,
            styles: cell_styles,
            content: BoxContent::Text("Narrow grid cell".to_string()),
        });

        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![cell_id]),
        });
        arena.add_root(page_id);

        let cell_x = 301.0;
        let cell_w = 120.0;

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 200.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: cell_id,
                    kind: BoxKind::Cell,
                    x: cell_x,
                    y: PAGE_MARGIN_PT,
                    width: cell_w,
                    height: 40.0,
                    content: LayoutContent::Text("Narrow grid cell".to_string()),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        let page = pages.pages.first().expect("expected a rendered page");

        let bg_rect = page
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                DrawCommand::Rect {
                    x,
                    y: _,
                    w,
                    h: _,
                    fill,
                    stroke: _,
                    stroke_width: _,
                } if *fill == Some(bg) => Some((*x, *w)),
                _ => None,
            })
            .expect("expected background rect for cell");

        assert!(approx_eq(bg_rect.0, cell_x), "background x must match cell x");
        assert!(
            approx_eq(bg_rect.1, cell_w),
            "background width must match cell width"
        );
        assert!(
            bg_rect.0 + bg_rect.1 <= A4_WIDTH_PT + 0.01,
            "background must not overflow page width for this narrow cell"
        );
    }

    #[test]
    fn table_cell_children_render_inline_paragraph_content() {
        let mut arena = DocumentArena::new();

        let paragraph_id = arena.alloc(StyledBox {
            id: "p-1".to_string(),
            kind: BoxKind::Paragraph,
            styles: ResolvedStyles::for_kind(&BoxKind::Paragraph),
            content: BoxContent::Inline(vec![InlineRun {
                text: "Feature".to_string(),
                bold: true,
                italic: false,
                code: false,
                link: None,
            }]),
        });

        let cell_styles = ResolvedStyles::for_kind(&BoxKind::Cell);
        let cell_padding_left = cell_styles.padding.left * MM_TO_PT;
        let cell_padding_right = cell_styles.padding.right * MM_TO_PT;

        let cell_id = arena.alloc(StyledBox {
            id: "cell-1".to_string(),
            kind: BoxKind::Cell,
            styles: cell_styles,
            content: BoxContent::Children(vec![paragraph_id]),
        });

        let row_id = arena.alloc(StyledBox {
            id: "row-1".to_string(),
            kind: BoxKind::Row,
            styles: ResolvedStyles::for_kind(&BoxKind::Row),
            content: BoxContent::Children(vec![cell_id]),
        });

        let table_id = arena.alloc(StyledBox {
            id: "table-1".to_string(),
            kind: BoxKind::Table,
            styles: ResolvedStyles::for_kind(&BoxKind::Table),
            content: BoxContent::Children(vec![row_id]),
        });

        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![table_id]),
        });
        arena.add_root(page_id);

        let table_x = PAGE_MARGIN_PT;
        let table_w = 240.0;
        let row_y = PAGE_MARGIN_PT;
        let cell_x = table_x;
        let cell_w = table_w;
        let paragraph_x = cell_x + cell_padding_left;

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 200.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: table_id,
                    kind: BoxKind::Table,
                    x: table_x,
                    y: row_y,
                    width: table_w,
                    height: 80.0,
                    content: LayoutContent::Children(vec![2]),
                },
                LayoutBox {
                    arena_id: row_id,
                    kind: BoxKind::Row,
                    x: table_x,
                    y: row_y,
                    width: table_w,
                    height: 40.0,
                    content: LayoutContent::Children(vec![3]),
                },
                LayoutBox {
                    arena_id: cell_id,
                    kind: BoxKind::Cell,
                    x: cell_x,
                    y: row_y,
                    width: cell_w,
                    height: 40.0,
                    content: LayoutContent::Children(vec![4]),
                },
                LayoutBox {
                    arena_id: paragraph_id,
                    kind: BoxKind::Paragraph,
                    x: paragraph_x,
                    y: row_y,
                    width: (cell_w - cell_padding_left - cell_padding_right).max(1.0),
                    height: 20.0,
                    content: LayoutContent::Inline(vec![InlineRun {
                        text: "Feature".to_string(),
                        bold: true,
                        italic: false,
                        code: false,
                        link: None,
                    }]),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        let page = pages.pages.first().expect("expected a rendered page");

        let has_feature_text = page.commands.iter().any(|cmd| match cmd {
            DrawCommand::Text { content, .. } => content.contains("Feature"),
            _ => false,
        });
        assert!(
            has_feature_text,
            "table cell content from LayoutContent::Children must render text"
        );
    }

    /// Long paragraph whose total height exceeds one content column must split
    /// its lines across multiple pages instead of dropping them silently.
    #[test]
    fn long_paragraph_splits_lines_across_pages() {
        let mut arena = DocumentArena::new();

        // At 10pt font with default line-height, ~60 lines fit per page.
        // 3000 short words wraps to many more than that.
        let text = "word ".repeat(3000);
        let p_id = arena.alloc(StyledBox {
            id: "p-long".to_string(),
            kind: BoxKind::Paragraph,
            styles: ResolvedStyles::for_kind(&BoxKind::Paragraph),
            content: BoxContent::Text(text.clone()),
        });
        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![p_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 2000.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: p_id,
                    kind: BoxKind::Paragraph,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 2000.0,
                    content: LayoutContent::Text(text),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        assert!(
            pages.pages.len() >= 2,
            "long paragraph must span at least 2 pages, got {}",
            pages.pages.len()
        );
        for (i, page) in pages.pages.iter().enumerate() {
            let has_text = page
                .commands
                .iter()
                .any(|c| matches!(c, DrawCommand::Text { .. }));
            assert!(has_text, "page {} must contain at least one Text cmd", i + 1);
        }
    }

    /// `block_start_page` must record the page where a block's first draw lands,
    /// not the page where `place_node` entered.
    #[test]
    fn block_start_page_records_after_page_break() {
        let mut arena = DocumentArena::new();

        // Filler paragraph to push cursor down so the next one breaks.
        let filler_text = "line ".repeat(3000);
        let filler_id = arena.alloc(StyledBox {
            id: "p-filler".to_string(),
            kind: BoxKind::Paragraph,
            styles: ResolvedStyles::for_kind(&BoxKind::Paragraph),
            content: BoxContent::Text(filler_text.clone()),
        });
        let target_id = arena.alloc(StyledBox {
            id: "p-target".to_string(),
            kind: BoxKind::Paragraph,
            styles: ResolvedStyles::for_kind(&BoxKind::Paragraph),
            content: BoxContent::Text("target".to_string()),
        });
        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![filler_id, target_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 3000.0,
                    content: LayoutContent::Children(vec![1, 2]),
                },
                LayoutBox {
                    arena_id: filler_id,
                    kind: BoxKind::Paragraph,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 2500.0,
                    content: LayoutContent::Text(filler_text),
                },
                LayoutBox {
                    arena_id: target_id,
                    kind: BoxKind::Paragraph,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT + 2500.0,
                    width: CONTENT_WIDTH_PT,
                    height: 20.0,
                    content: LayoutContent::Text("target".to_string()),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        assert!(pages.pages.len() >= 2, "expected multi-page output");
        let target_page = pages
            .block_start_page
            .get("p-target")
            .copied()
            .expect("block_start_page must contain p-target");
        // Find which page actually contains "target" text
        let drawn_on = pages
            .pages
            .iter()
            .enumerate()
            .find_map(|(i, pg)| {
                pg.commands.iter().find_map(|c| match c {
                    DrawCommand::Text { content, .. } if content.contains("target") => Some(i as u32 + 1),
                    _ => None,
                })
            })
            .expect("target text must be drawn somewhere");
        assert_eq!(
            target_page, drawn_on,
            "block_start_page ({target_page}) must match page where text is drawn ({drawn_on})"
        );
    }

    /// `new_page` must rebalance the opacity stack: pop on old page, push on new.
    #[test]
    fn new_page_rebalances_active_opacity() {
        let mut arena = DocumentArena::new();

        let mut p_styles = ResolvedStyles::for_kind(&BoxKind::Paragraph);
        p_styles.opacity = 0.5;
        let text = "word ".repeat(3000);
        let p_id = arena.alloc(StyledBox {
            id: "p-opa".to_string(),
            kind: BoxKind::Paragraph,
            styles: p_styles,
            content: BoxContent::Text(text.clone()),
        });
        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![p_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 3000.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: p_id,
                    kind: BoxKind::Paragraph,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 3000.0,
                    content: LayoutContent::Text(text),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        assert!(pages.pages.len() >= 2);
        for (i, page) in pages.pages.iter().enumerate() {
            let pushes = page
                .commands
                .iter()
                .filter(|c| matches!(c, DrawCommand::PushOpacity { .. }))
                .count();
            let pops = page
                .commands
                .iter()
                .filter(|c| matches!(c, DrawCommand::PopOpacity))
                .count();
            assert_eq!(
                pushes, pops,
                "page {}: PushOpacity/PopOpacity count mismatch: {pushes}/{pops}",
                i + 1
            );
        }
    }

    /// Clipped node spanning a page break must have balanced PushClipRect/PopClip on each page.
    #[test]
    fn new_page_rebalances_active_clip() {
        let mut arena = DocumentArena::new();

        let mut quote_styles = ResolvedStyles::for_kind(&BoxKind::Quote);
        quote_styles.overflow_clip = true;
        let text = "word ".repeat(3000);
        let quote_id = arena.alloc(StyledBox {
            id: "quote-clip".to_string(),
            kind: BoxKind::Quote,
            styles: quote_styles,
            content: BoxContent::Text(text.clone()),
        });
        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![quote_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 3000.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: quote_id,
                    kind: BoxKind::Quote,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 3000.0,
                    content: LayoutContent::Text(text),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        assert!(pages.pages.len() >= 2);
        for (i, page) in pages.pages.iter().enumerate() {
            let pushes = page
                .commands
                .iter()
                .filter(|c| matches!(c, DrawCommand::PushClipRect { .. }))
                .count();
            let pops = page
                .commands
                .iter()
                .filter(|c| matches!(c, DrawCommand::PopClip))
                .count();
            assert_eq!(
                pushes, pops,
                "page {}: PushClipRect/PopClip count mismatch: {pushes}/{pops}",
                i + 1
            );
        }
    }

    /// Grid with rows that exceed page capacity must break between rows.
    #[test]
    fn grid_breaks_between_rows_when_remaining_space_insufficient() {
        let mut arena = DocumentArena::new();

        // Tall cell content via explicit height so estimate_height exceeds page.
        let mut tall_cell_styles = ResolvedStyles::for_kind(&BoxKind::Cell);
        tall_cell_styles.height = Some(250.0); // 250mm ≈ 708pt — well over page
        let mk_cell = |arena: &mut DocumentArena, styles: ResolvedStyles, id: &str| {
            arena.alloc(StyledBox {
                id: id.to_string(),
                kind: BoxKind::Cell,
                styles,
                content: BoxContent::Text(format!("cell-{id}")),
            })
        };

        let c1 = mk_cell(&mut arena, tall_cell_styles.clone(), "a");
        let c2 = mk_cell(&mut arena, tall_cell_styles.clone(), "b");
        let grid_id = arena.alloc(StyledBox {
            id: "g".to_string(),
            kind: BoxKind::Grid,
            styles: ResolvedStyles::for_kind(&BoxKind::Grid),
            content: BoxContent::Children(vec![c1, c2]),
        });
        let page_id = arena.alloc(StyledBox {
            id: "page-1".to_string(),
            kind: BoxKind::Page,
            styles: ResolvedStyles::for_kind(&BoxKind::Page),
            content: BoxContent::Children(vec![grid_id]),
        });
        arena.add_root(page_id);

        let layout = LayoutTree {
            nodes: vec![
                LayoutBox {
                    arena_id: page_id,
                    kind: BoxKind::Page,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 1500.0,
                    content: LayoutContent::Children(vec![1]),
                },
                LayoutBox {
                    arena_id: grid_id,
                    kind: BoxKind::Grid,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 1500.0,
                    content: LayoutContent::Children(vec![2, 3]),
                },
                LayoutBox {
                    arena_id: c1,
                    kind: BoxKind::Cell,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT,
                    width: CONTENT_WIDTH_PT,
                    height: 708.0,
                    content: LayoutContent::Text("cell-a".to_string()),
                },
                LayoutBox {
                    arena_id: c2,
                    kind: BoxKind::Cell,
                    x: PAGE_MARGIN_PT,
                    y: PAGE_MARGIN_PT + 708.0,
                    width: CONTENT_WIDTH_PT,
                    height: 708.0,
                    content: LayoutContent::Text("cell-b".to_string()),
                },
            ],
            roots: vec![0],
        };

        let pages = paginate(&layout, &arena);
        // Grid is 1 column here (default), so each row is one cell; two rows.
        // First row fills a page; second row must move to the next.
        assert!(
            pages.pages.len() >= 2,
            "expected page break between tall grid rows, got {}",
            pages.pages.len()
        );
    }
}
