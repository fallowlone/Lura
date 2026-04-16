/// Phase 4a: LayoutTree → PageTree
///
/// Sequential `cursor_y` drives vertical flow.
/// X position and width come from taffy.
/// For GRID: `cursor_y` is saved and restored per cell in each row.
use std::collections::HashMap;

use super::layout::{
    text_container_width_pt, LayoutContent, LayoutNodeIdx, LayoutTree, A4_HEIGHT_PT, A4_WIDTH_PT,
    CONTENT_WIDTH_PT, MM_TO_PT, PAGE_MARGIN_PT,
};
use super::styles::{BoxKind, Color, FloatMode, FontStyle, FontWeight, InlineRun, ListStyle};
use super::text::{
    break_inline_runs, break_text, inline_lines_block_height, inline_runs_block_height,
    text_block_height, InlineLine, TextLine,
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

#[derive(Debug)]
pub struct PageTree {
    pub pages: Vec<Page>,
    /// Stable block `id` → 1-based page index where the block first enters pagination.
    pub block_start_page: HashMap<String, u32>,
}

impl PageTree {
    pub fn new() -> Self {
        Self {
            pages: vec![Page::new()],
            block_start_page: HashMap::new(),
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
}

impl<'a> Paginator<'a> {
    fn new(layout: &'a LayoutTree, styled: &'a super::arena::DocumentArena) -> Self {
        Self {
            layout,
            styled,
            pages: vec![Page::new()],
            cursor_y: CONTENT_TOP,
            block_start_page: HashMap::new(),
            list_item_counter: None,
            page_header: None,
            page_footer: None,
            pending_block_starts: Vec::new(),
            active_opacity: Vec::new(),
        }
    }

    /// Queue a block to record its start page when its first draw lands.
    fn queue_block_start(&mut self, arena_id: super::arena::NodeId) {
        let id = self.styled.get(arena_id).id.clone();
        if id.is_empty() || self.block_start_page.contains_key(&id) {
            return;
        }
        self.pending_block_starts.push(id);
    }

    fn flush_pending_block_starts(&mut self) {
        if self.pending_block_starts.is_empty() {
            return;
        }
        let page_1based = self.pages.len() as u32;
        for id in self.pending_block_starts.drain(..) {
            self.block_start_page.entry(id).or_insert(page_1based);
        }
    }

    fn new_page(&mut self) {
        // Close cross-page wraps on old page.
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
        let node = self.layout.nodes[node_idx].clone();
        self.queue_block_start(node.arena_id);
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;
        let margin_top = styles.margin.top * MM_TO_PT;
        let margin_bottom = styles.margin.bottom * MM_TO_PT;
        let padding_left = styles.padding.left * MM_TO_PT;
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

        if let Some(bg) = styles.background {
            if !matches!(
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
                    )
                }
            }
            LayoutContent::Inline(runs) => {
                let width = text_container_width_pt(node.width, styles.padding.left, styles.padding.right);
                let justify = styles.justify || styles.text_align == super::styles::TextAlign::Justify;
                let lines = break_inline_runs(
                    &runs,
                    width,
                    styles.font_size,
                    styles.line_height,
                    styles.letter_spacing,
                    styles.word_spacing,
                    bold,
                    justify,
                );
                self.paint_inline_lines_paginated(
                    &lines,
                    block_x,
                    node.width,
                    padding_left,
                    &styles,
                    bold,
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
    ) -> f32 {
        if lines.is_empty() {
            return 0.0;
        }
        let segments = self.compute_line_segments(lines.iter().map(|l| l.line_height_pt));
        let italic = styles.font_style == FontStyle::Italic;
        let mut total = 0.0f32;
        for (idx, (a, b)) in segments.iter().enumerate() {
            if idx > 0 {
                self.new_page();
            }
            let seg_top = self.cursor_y;
            let seg_h: f32 = lines[*a..*b].iter().map(|l| l.line_height_pt).sum();
            if let Some(bg) = styles.background {
                self.push_cmd(DrawCommand::Rect {
                    x: block_x,
                    y: seg_top,
                    w: width_bb.max(1.0),
                    h: seg_h,
                    fill: Some(bg),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
            for (k, line) in lines[*a..*b].iter().enumerate() {
                let y = seg_top + styles.font_size + k as f32 * line.line_height_pt;
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
            self.cursor_y = seg_top + seg_h;
            total += seg_h;
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
    ) -> f32 {
        if lines.is_empty() {
            return 0.0;
        }
        let segments = self.compute_line_segments(lines.iter().map(|l| l.line_height_pt));
        let italic_base = styles.font_style == FontStyle::Italic;
        let mut total = 0.0f32;
        for (idx, (a, b)) in segments.iter().enumerate() {
            if idx > 0 {
                self.new_page();
            }
            let seg_top = self.cursor_y;
            let seg_h: f32 = lines[*a..*b].iter().map(|l| l.line_height_pt).sum();
            if let Some(bg) = styles.background {
                self.push_cmd(DrawCommand::Rect {
                    x: block_x,
                    y: seg_top,
                    w: width_bb.max(1.0),
                    h: seg_h,
                    fill: Some(bg),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
            for (k, line) in lines[*a..*b].iter().enumerate() {
                let baseline_y = seg_top + styles.font_size + k as f32 * line.line_height_pt;
                // Render each line as a single Text command to avoid micro-gaps.
                let first = line.fragments.first();
                let font_family = if first.map_or(false, |f| f.code) {
                    "Courier".to_string()
                } else {
                    styles.font_family.clone()
                };
                let group_bold = first.map_or(bold, |f| bold || f.bold);
                let group_italic = italic_base || first.map_or(false, |f| f.italic);
                let (link_uri, link_width_pt) = line
                    .fragments
                    .iter()
                    .find(|f| f.link.is_some())
                    .map(|f| (f.link.clone(), Some(f.width)))
                    .unwrap_or((None, None));
                let group_color = if link_uri.is_some() {
                    Color::from_hex(0x1D4ED8)
                } else {
                    styles.color
                };
                self.push_cmd(DrawCommand::Text {
                    content: line.full_text.clone(),
                    x: block_x + padding_left,
                    y: baseline_y,
                    font_size: styles.font_size,
                    font_family,
                    bold: group_bold,
                    italic: group_italic,
                    color: group_color,
                    link_uri,
                    link_width_pt,
                });
            }
            self.cursor_y = seg_top + seg_h;
            total += seg_h;
        }
        total
    }

    /// Given a sequence of per-line heights, compute (start_idx, end_idx) segment
    /// ranges so each segment fits between the current cursor_y (for the first
    /// segment) or CONTENT_TOP (for subsequent segments) and CONTENT_BOTTOM.
    /// A segment never starts empty; lines that exceed a full page are placed
    /// alone on a page (inevitable overflow).
    fn compute_line_segments<I: Iterator<Item = f32>>(&self, heights: I) -> Vec<(usize, usize)> {
        let heights: Vec<f32> = heights.collect();
        let mut segments: Vec<(usize, usize)> = Vec::new();
        let mut seg_start = 0usize;
        let mut sim_y = self.cursor_y;
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
            styles.font_size,
            styles.line_height,
            styles.letter_spacing,
            styles.word_spacing,
            bold,
            justify,
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
        let table_start_y = self.cursor_y;
        let table_x = table_node.x;
        let table_w = table_node.width.max(1.0);

        // Pre-calculate column widths: find the max cell width at each column index across all rows
        let num_cols = row_indices
            .iter()
            .map(|&ri| {
                match &self.layout.nodes[ri].content {
                    LayoutContent::Children(cells) => cells.len(),
                    _ => 0,
                }
            })
            .max()
            .unwrap_or(0);

        let col_widths = if num_cols == 0 {
            vec![]
        } else {
            let mut widths = vec![0.0f32; num_cols];
            for &row_idx in &row_indices {
                let cell_indices = match &self.layout.nodes[row_idx].content {
                    LayoutContent::Children(cells) => cells.clone(),
                    _ => vec![],
                };
                for (col_idx, &cell_idx) in cell_indices.iter().enumerate() {
                    if col_idx < num_cols {
                        widths[col_idx] = widths[col_idx].max(self.layout.nodes[cell_idx].width);
                    }
                }
            }
            // Normalize to table width if there's slack
            let total: f32 = widths.iter().sum();
            if total > 0.0 && (total - table_w).abs() > 0.5 {
                let scale = table_w / total;
                widths.iter_mut().for_each(|w| *w *= scale);
            }
            widths
        };

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

            let row_min_cell_y = cell_indices
                .iter()
                .map(|&ci| self.layout.nodes[ci].y)
                .fold(f32::INFINITY, f32::min);

            // Row height = max over cells (use pre-calculated column widths for text wrapping)
            let row_height = cell_indices
                .iter()
                .enumerate()
                .map(|(col_idx, &ci)| {
                    let cell = &self.layout.nodes[ci];
                    let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                    let bold = cell_styles.font_weight == FontWeight::Bold;
                    // Use pre-calculated column width for consistent text wrapping
                    let cell_w = col_widths.get(col_idx).copied().unwrap_or_else(|| cell.width).max(1.0);
                    match &cell.content {
                        LayoutContent::Text(t) => {
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
                            text_block_height(&lines)
                                + cell_styles.padding.top * MM_TO_PT
                                + cell_styles.padding.bottom * MM_TO_PT
                        }
                        LayoutContent::Inline(runs) => {
                            let w = text_container_width_pt(
                                cell_w,
                                cell_styles.padding.left,
                                cell_styles.padding.right,
                            );
                            let justify = cell_styles.justify
                                || cell_styles.text_align == super::styles::TextAlign::Justify;
                            inline_runs_block_height(
                                runs,
                                w,
                                cell_styles.font_size,
                                cell_styles.line_height,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                                bold,
                                justify,
                            ) + cell_styles.padding.top * MM_TO_PT
                                + cell_styles.padding.bottom * MM_TO_PT
                        }
                        LayoutContent::Children(children) => {
                            children
                                .iter()
                                .map(|&child_idx| self.estimate_height(child_idx))
                                .sum::<f32>()
                                + cell_styles.padding.top * MM_TO_PT
                                + cell_styles.padding.bottom * MM_TO_PT
                        }
                        LayoutContent::Empty => 0.0,
                    }
                })
                .fold(0.0f32, f32::max)
                .max(12.0);

            let page_before_break = self.pages.len();
            if !row_styles.allow_row_split
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
            // Compute cumulative x positions based on pre-calculated column widths
            let mut cell_x = table_x;
            for (col_idx, &cell_idx) in cell_indices.iter().enumerate() {
                let cell = self.layout.nodes[cell_idx].clone();
                let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                let bold = cell_styles.font_weight == FontWeight::Bold;
                let padding_top = cell_styles.padding.top * MM_TO_PT;
                let _padding_bottom = cell_styles.padding.bottom * MM_TO_PT;
                let padding_left = cell_styles.padding.left * MM_TO_PT;

                // Use pre-calculated column width for this column
                let cell_w = col_widths.get(col_idx).copied().unwrap_or_else(|| cell.width).max(1.0);

                // Top-align cell content so the first text baseline lines up across columns in the
                // same row (vertical centering per cell made multi-line vs single-line cells ragged).
                let cell_dy = cell.y - row_min_cell_y;
                let content_top = row_start_y + padding_top + cell_dy;

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

                match &cell.content {
                    LayoutContent::Text(text) => {
                        let w = text_container_width_pt(
                            cell_w,
                            cell_styles.padding.left,
                            cell_styles.padding.right,
                        );
                        let lines = break_text(
                            text,
                            w,
                            cell_styles.font_size,
                            cell_styles.line_height,
                            bold,
                            cell_styles.letter_spacing,
                            cell_styles.word_spacing,
                        );
                        for (i, line) in lines.iter().enumerate() {
                            let y = content_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if y > CONTENT_BOTTOM {
                                break;
                            }
                            self.push_cmd(DrawCommand::Text {
                                content: line.text.clone(),
                                x: cell_x + padding_left,
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
                    LayoutContent::Inline(runs) => {
                        let w = text_container_width_pt(
                            cell_w,
                            cell_styles.padding.left,
                            cell_styles.padding.right,
                        );
                        let justify = cell_styles.justify
                            || cell_styles.text_align == super::styles::TextAlign::Justify;
                        let lines = break_inline_runs(
                            runs,
                            w,
                            cell_styles.font_size,
                            cell_styles.line_height,
                            cell_styles.letter_spacing,
                            cell_styles.word_spacing,
                            bold,
                            justify,
                        );
                        for (i, line) in lines.iter().enumerate() {
                            let baseline_y = content_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if baseline_y > CONTENT_BOTTOM {
                                break;
                            }
                            let mut x_cursor = cell_x + padding_left;
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
                    LayoutContent::Children(children) => {
                        let cursor_before_cell = self.cursor_y;
                        // Layout `y` is document-absolute (PAGE roots stack in `compute_layout`).
                        // Row painting uses page-local `content_top` for Text/Inline; Children must
                        // use the same origin: top of cell content + relative offset inside cell.
                        let children_min_y = children
                            .iter()
                            .map(|&ci| self.layout.nodes[ci].y)
                            .fold(f32::INFINITY, f32::min);
                        for &child_idx in children {
                            let child = &self.layout.nodes[child_idx];
                            let child_styles = self.styled.get(child.arena_id).styles.clone();
                            let m_top = child_styles.margin.top * MM_TO_PT;
                            let rel_y = child.y - children_min_y;
                            self.cursor_y = content_top + rel_y - m_top;
                            self.place_node(child_idx);
                        }
                        self.cursor_y = cursor_before_cell;
                    }
                    LayoutContent::Empty => {}
                }

                // Advance cell_x for next column
                cell_x += cell_w;
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
        let node = &self.layout.nodes[node_idx];
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
                        styles.font_size,
                        styles.line_height,
                        styles.letter_spacing,
                        styles.word_spacing,
                        bold,
                        justify,
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
    }
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
