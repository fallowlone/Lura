/// Phase 4a: LayoutTree → PageTree
///
/// Sequential `cursor_y` drives vertical flow.
/// X position and width come from taffy.
/// For GRID: `cursor_y` is saved and restored per cell in each row.
use super::layout::{
    LayoutContent, LayoutNodeIdx, LayoutTree, A4_HEIGHT_PT, A4_WIDTH_PT, CONTENT_WIDTH_PT,
    MM_TO_PT, PAGE_MARGIN_PT,
};
use super::styles::{BoxKind, Color, FloatMode, FontStyle, FontWeight, ListStyle};
use super::text::{break_inline_runs, break_text, text_block_height};

// --- Draw commands ---

#[derive(Debug, Clone)]
pub enum DrawCommand {
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
}

impl PageTree {
    pub fn new() -> Self {
        Self {
            pages: vec![Page::new()],
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
    /// Counter for the current ordered list (`None` = bulleted)
    list_item_counter: Option<usize>,
    page_header: Option<String>,
    page_footer: Option<String>,
}

impl<'a> Paginator<'a> {
    fn new(layout: &'a LayoutTree, styled: &'a super::arena::DocumentArena) -> Self {
        Self {
            layout,
            styled,
            pages: vec![Page::new()],
            cursor_y: CONTENT_TOP,
            list_item_counter: None,
            page_header: None,
            page_footer: None,
        }
    }

    fn new_page(&mut self) {
        self.pages.push(Page::new());
        self.cursor_y = CONTENT_TOP;
        self.draw_page_chrome();
    }

    fn push_cmd(&mut self, cmd: DrawCommand) {
        self.pages.last_mut().unwrap().commands.push(cmd);
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
            });
        }
    }

    // --- Place arbitrary node ---

    fn place_node(&mut self, node_idx: LayoutNodeIdx) -> f32 {
        let node = self.layout.nodes[node_idx].clone();
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

        if let Some(bg) = styles.background {
            let estimated_h = self.estimate_height(node_idx);
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

        let consumed = match node.content.clone() {
            // --- Text block ---
            LayoutContent::Text(text) => {
                // ListItem: special path with bullet
                if matches!(node.kind, BoxKind::ListItem) {
                    return self.place_list_item(node_idx, &text, margin_top, margin_bottom);
                }

                let width = node.width.max(CONTENT_WIDTH_PT * 0.3);
                let lines = break_text(
                    &text,
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

                for (i, line) in lines.iter().enumerate() {
                    let y = self.cursor_y + styles.font_size + i as f32 * line.line_height_pt;
                    if y > CONTENT_BOTTOM {
                        break;
                    }
                    self.push_cmd(DrawCommand::Text {
                        content: line.text.clone(),
                        x: block_x + padding_left,
                        y,
                        font_size: styles.font_size,
                        font_family: styles.font_family.clone(),
                        bold,
                        italic: styles.font_style == FontStyle::Italic,
                        color: styles.color,
                    });
                }
                self.cursor_y += block_h;
                block_h
            }
            LayoutContent::Inline(runs) => {
                let width = node.width.max(CONTENT_WIDTH_PT * 0.3);
                let lines = break_inline_runs(
                    &runs,
                    width,
                    styles.font_size,
                    styles.line_height,
                    styles.letter_spacing,
                    styles.word_spacing,
                    styles.justify || styles.text_align == super::styles::TextAlign::Justify,
                );
                let block_h = if lines.is_empty() {
                    0.0
                } else {
                    styles.font_size
                        + (lines.len().saturating_sub(1)) as f32
                            * (styles.font_size * styles.line_height)
                };

                if self.cursor_y + block_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
                    self.new_page();
                }

                for (line_idx, line) in lines.iter().enumerate() {
                    let baseline_y =
                        self.cursor_y + styles.font_size + line_idx as f32 * line.line_height_pt;
                    if baseline_y > CONTENT_BOTTOM {
                        break;
                    }
                    let mut x_cursor = block_x + padding_left;
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

        self.cursor_y += margin_bottom;
        consumed + margin_top + margin_bottom
    }

    // --- List item with bullet ---

    fn place_list_item(
        &mut self,
        node_idx: LayoutNodeIdx,
        text: &str,
        margin_top: f32,
        margin_bottom: f32,
    ) -> f32 {
        let node = self.layout.nodes[node_idx].clone();
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;

        let lines = break_text(
            text,
            node.width.max(1.0),
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
            });
        }

        self.cursor_y += block_h + margin_bottom;
        block_h + margin_top + margin_bottom
    }

    // ─── GRID ─────────────────────────────────────────────────────────────────

    fn place_grid(&mut self, grid_idx: LayoutNodeIdx, child_indices: Vec<LayoutNodeIdx>) -> f32 {
        let grid_node = self.layout.nodes[grid_idx].clone();
        let grid_styles = self.styled.get(grid_node.arena_id).styles.clone();
        let cols = grid_styles.grid_column_count().max(1);
        let grid_start_y = self.cursor_y;

        for row_cells in child_indices.chunks(cols) {
            let row_start_y = self.cursor_y;
            let mut max_h = 0.0f32;

            for &ci in row_cells {
                // Reset cursor_y to row start for each cell
                self.cursor_y = row_start_y;
                self.place_node(ci);
                // Track max row height
                let cell_h = self.cursor_y - row_start_y;
                if cell_h > max_h {
                    max_h = cell_h;
                }
            }

            // Advance cursor by the tallest cell in the row
            self.cursor_y = row_start_y + max_h;
        }

        self.cursor_y - grid_start_y
    }

    // ─── TABLE ────────────────────────────────────────────────────────────────

    fn place_table(&mut self, table_idx: LayoutNodeIdx, row_indices: Vec<LayoutNodeIdx>) -> f32 {
        let table_node = self.layout.nodes[table_idx].clone();
        let table_start_y = self.cursor_y;
        let table_x = table_node.x;
        let table_w = table_node.width.max(1.0);

        // Line above first table row
        self.push_cmd(DrawCommand::Line {
            x1: table_x,
            y1: self.cursor_y,
            x2: table_x + table_w,
            y2: self.cursor_y,
            color: Color::from_hex(0xCCCCCC),
            width: 0.5,
        });

        for (row_num, row_idx) in row_indices.into_iter().enumerate() {
            let row_node = self.layout.nodes[row_idx].clone();
            let row_styles = self.styled.get(row_node.arena_id).styles.clone();
            let row_start_y = self.cursor_y;

            let cell_indices = match &row_node.content {
                LayoutContent::Children(cells) => cells.clone(),
                _ => vec![],
            };

            if cell_indices.is_empty() {
                continue;
            }

            // Row height = max over cells
            let row_height = cell_indices
                .iter()
                .map(|&ci| {
                    let cell = &self.layout.nodes[ci];
                    let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                    let bold = cell_styles.font_weight == FontWeight::Bold;
                    match &cell.content {
                        LayoutContent::Text(t) => {
                            let w = cell.width.max(1.0);
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
                            let w = cell.width.max(1.0);
                            let lines = break_inline_runs(
                                runs,
                                w,
                                cell_styles.font_size,
                                cell_styles.line_height,
                                cell_styles.letter_spacing,
                                cell_styles.word_spacing,
                                cell_styles.justify,
                            );
                            if lines.is_empty() {
                                0.0
                            } else {
                                cell_styles.font_size
                                    + (lines.len().saturating_sub(1)) as f32
                                        * (cell_styles.font_size * cell_styles.line_height)
                                    + cell_styles.padding.top * MM_TO_PT
                                    + cell_styles.padding.bottom * MM_TO_PT
                            }
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

            if !row_styles.allow_row_split
                && self.cursor_y + row_height > CONTENT_BOTTOM
                && self.cursor_y > CONTENT_TOP
            {
                self.new_page();
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
            for &cell_idx in &cell_indices {
                let cell = self.layout.nodes[cell_idx].clone();
                let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                let bold = cell_styles.font_weight == FontWeight::Bold;
                let padding_top = cell_styles.padding.top * MM_TO_PT;
                let padding_left = cell_styles.padding.left * MM_TO_PT;

                // Per-cell background
                if let Some(cell_bg) = cell_styles.background {
                    self.push_cmd(DrawCommand::Rect {
                        x: cell.x,
                        y: row_start_y,
                        w: cell.width.max(1.0),
                        h: row_height,
                        fill: Some(cell_bg),
                        stroke: None,
                        stroke_width: 0.0,
                    });
                }

                match &cell.content {
                    LayoutContent::Text(text) => {
                        let w = cell.width.max(1.0);
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
                            let y = row_start_y
                                + padding_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if y > CONTENT_BOTTOM {
                                break;
                            }
                            self.push_cmd(DrawCommand::Text {
                                content: line.text.clone(),
                                x: cell.x + padding_left,
                                y,
                                font_size: cell_styles.font_size,
                                font_family: cell_styles.font_family.clone(),
                                bold,
                                italic: cell_styles.font_style == FontStyle::Italic,
                                color: cell_styles.color,
                            });
                        }
                    }
                    LayoutContent::Inline(runs) => {
                        let w = cell.width.max(1.0);
                        let lines = break_inline_runs(
                            runs,
                            w,
                            cell_styles.font_size,
                            cell_styles.line_height,
                            cell_styles.letter_spacing,
                            cell_styles.word_spacing,
                            cell_styles.justify,
                        );
                        for (i, line) in lines.iter().enumerate() {
                            let baseline_y = row_start_y
                                + padding_top
                                + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                            if baseline_y > CONTENT_BOTTOM {
                                break;
                            }
                            let mut x_cursor = cell.x + padding_left;
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
                                });
                                x_cursor += frag.width;
                            }
                        }
                    }
                    LayoutContent::Children(children) => {
                        let cursor_before_cell = self.cursor_y;
                        self.cursor_y = row_start_y + padding_top;
                        for &child_idx in children {
                            self.place_node(child_idx);
                        }
                        self.cursor_y = cursor_before_cell;
                    }
                    LayoutContent::Empty => {}
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

        // Table border
        let total_h = self.cursor_y - table_start_y;
        self.push_cmd(DrawCommand::Rect {
            x: table_x,
            y: table_start_y,
            w: table_w,
            h: total_h,
            fill: None,
            stroke: Some(Color::from_hex(0xCCCCCC)),
            stroke_width: 0.5,
        });

        total_h
    }

    // --- Block height estimate (background rects) ---

    fn estimate_height(&self, node_idx: LayoutNodeIdx) -> f32 {
        let node = &self.layout.nodes[node_idx];
        let styles = self.styled.get(node.arena_id).styles.clone();
        if matches!(node.kind, BoxKind::Figure) && matches!(node.content, LayoutContent::Empty) {
            return Self::figure_placeholder_height_pt(&styles);
        }
        let bold = styles.font_weight == FontWeight::Bold;
        match &node.content {
            LayoutContent::Text(t) => {
                let w = node.width.max(CONTENT_WIDTH_PT * 0.3);
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
                let w = node.width.max(CONTENT_WIDTH_PT * 0.3);
                let lines = break_inline_runs(
                    runs,
                    w,
                    styles.font_size,
                    styles.line_height,
                    styles.letter_spacing,
                    styles.word_spacing,
                    styles.justify,
                );
                if lines.is_empty() {
                    0.0
                } else {
                    styles.font_size
                        + (lines.len().saturating_sub(1)) as f32
                            * (styles.font_size * styles.line_height)
                }
            }
            LayoutContent::Children(children) => {
                children.iter().map(|&ci| self.estimate_height(ci)).sum()
            }
            LayoutContent::Empty => 0.0,
        }
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

    PageTree { pages: pager.pages }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::arena::DocumentArena;
    use crate::engine::layout::LayoutBox;
    use crate::engine::styles::{BoxContent, InlineRun, ResolvedStyles, StyledBox};

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
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
}
