/// Фаза 4а: LayoutTree → PageTree
///
/// Последовательный курсор (cursor_y) управляет вертикальным потоком.
/// Координаты X и ширина берутся из taffy.
/// Для GRID: cursor_y сохраняется и восстанавливается для каждой ячейки в строке.

use super::layout::{LayoutContent, LayoutNodeIdx, LayoutTree,
                    A4_HEIGHT_PT, A4_WIDTH_PT, PAGE_MARGIN_PT, MM_TO_PT, CONTENT_WIDTH_PT};
use super::styles::{BoxKind, Color, FontStyle, FontWeight, ListStyle};
use super::text::{break_text, text_block_height};

// ─── Команды отрисовки ────────────────────────────────────────────────────────

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
        Self { width: A4_WIDTH_PT, height: A4_HEIGHT_PT, commands: Vec::new() }
    }
}

#[derive(Debug)]
pub struct PageTree {
    pub pages: Vec<Page>,
}

impl PageTree {
    pub fn new() -> Self {
        Self { pages: vec![Page::new()] }
    }
}

impl Default for PageTree {
    fn default() -> Self { Self::new() }
}

// ─── Константы ────────────────────────────────────────────────────────────────

const CONTENT_TOP:    f32 = PAGE_MARGIN_PT;
const CONTENT_BOTTOM: f32 = A4_HEIGHT_PT - PAGE_MARGIN_PT;

// ─── Paginator ────────────────────────────────────────────────────────────────

struct Paginator<'a> {
    layout:            &'a LayoutTree,
    styled:            &'a super::arena::DocumentArena,
    pages:             Vec<Page>,
    cursor_y:          f32,
    /// Счётчик для текущего нумерованного списка (None = маркированный)
    list_item_counter: Option<usize>,
}

impl<'a> Paginator<'a> {
    fn new(layout: &'a LayoutTree, styled: &'a super::arena::DocumentArena) -> Self {
        Self { layout, styled, pages: vec![Page::new()], cursor_y: CONTENT_TOP, list_item_counter: None }
    }

    fn new_page(&mut self) {
        self.pages.push(Page::new());
        self.cursor_y = CONTENT_TOP;
    }

    fn push_cmd(&mut self, cmd: DrawCommand) {
        self.pages.last_mut().unwrap().commands.push(cmd);
    }

    // ─── Размещение произвольного узла ─────────────────────────────────────

    fn place_node(&mut self, node_idx: LayoutNodeIdx) -> f32 {
        let node = self.layout.nodes[node_idx].clone();
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;
        let margin_top    = styles.margin.top    * MM_TO_PT;
        let margin_bottom = styles.margin.bottom * MM_TO_PT;
        let padding_left  = styles.padding.left  * MM_TO_PT;

        self.cursor_y += margin_top;

        if let Some(bg) = styles.background {
            let estimated_h = self.estimate_height(node_idx);
            self.push_cmd(DrawCommand::Rect {
                x: node.x,
                y: self.cursor_y,
                w: node.width.max(CONTENT_WIDTH_PT),
                h: estimated_h,
                fill: Some(bg),
                stroke: None,
                stroke_width: 0.0,
            });
        }

        let consumed = match node.content.clone() {

            // ─── Текстовый блок ───────────────────────────────────────────
            LayoutContent::Text(text) => {
                // ListItem: особая обработка с bullet
                if matches!(node.kind, BoxKind::ListItem) {
                    return self.place_list_item(node_idx, &text, margin_top, margin_bottom);
                }

                let width = node.width.max(CONTENT_WIDTH_PT * 0.3);
                let lines = break_text(&text, width, styles.font_size, styles.line_height, bold);
                let block_h = text_block_height(&lines);

                if self.cursor_y + block_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
                    self.new_page();
                }

                for (i, line) in lines.iter().enumerate() {
                    let y = self.cursor_y + styles.font_size + i as f32 * line.line_height_pt;
                    if y > CONTENT_BOTTOM { break; }
                    self.push_cmd(DrawCommand::Text {
                        content: line.text.clone(),
                        x: node.x + padding_left,
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

            // ─── Контейнер ────────────────────────────────────────────────
            LayoutContent::Children(child_indices) => {
                match node.kind {
                    BoxKind::Table => self.place_table(node_idx, child_indices),
                    BoxKind::Grid  => self.place_grid(node_idx, child_indices),
                    BoxKind::List  => {
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
                }
            }

            LayoutContent::Empty => {
                if matches!(node.kind, BoxKind::Hr) {
                    let x = node.x;
                    let w = node.width.max(CONTENT_WIDTH_PT);
                    self.push_cmd(DrawCommand::Line {
                        x1: x,
                        y1: self.cursor_y,
                        x2: x + w,
                        y2: self.cursor_y,
                        color: Color::from_hex(0xCCCCCC),
                        width: 0.5,
                    });
                }
                0.0
            }
        };

        self.cursor_y += margin_bottom;
        consumed + margin_top + margin_bottom
    }

    // ─── LIST ITEM с bullet ───────────────────────────────────────────────────

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

        let lines = break_text(text, node.width.max(1.0), styles.font_size, styles.line_height, bold);
        let block_h = text_block_height(&lines);

        if self.cursor_y + block_h > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
            self.new_page();
        }

        // Bullet "•" или "1." — левее начала текста
        let bullet_text = match self.list_item_counter {
            Some(n) => format!("{}.", n),
            None    => "\u{2022}".to_string(),
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
            if y > CONTENT_BOTTOM { break; }
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
        let grid_node  = self.layout.nodes[grid_idx].clone();
        let grid_styles = self.styled.get(grid_node.arena_id).styles.clone();
        let cols = grid_styles.grid_columns.unwrap_or(1).max(1);
        let grid_start_y = self.cursor_y;

        for row_cells in child_indices.chunks(cols) {
            let row_start_y = self.cursor_y;
            let mut max_h = 0.0f32;

            for &ci in row_cells {
                // Восстанавливаем cursor_y в начало строки для каждой ячейки
                self.cursor_y = row_start_y;
                self.place_node(ci);
                // Запоминаем максимальную высоту строки
                let cell_h = self.cursor_y - row_start_y;
                if cell_h > max_h { max_h = cell_h; }
            }

            // Продвигаем курсор на высоту самой высокой ячейки строки
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

        // Линия над первой строкой таблицы
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

            // Высота строки = максимум по ячейкам
            let row_height = cell_indices.iter().map(|&ci| {
                let cell = &self.layout.nodes[ci];
                let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                let bold = cell_styles.font_weight == FontWeight::Bold;
                match &cell.content {
                    LayoutContent::Text(t) => {
                        let w = cell.width.max(1.0);
                        let lines = break_text(t, w, cell_styles.font_size, cell_styles.line_height, bold);
                        text_block_height(&lines)
                            + cell_styles.padding.top    * MM_TO_PT
                            + cell_styles.padding.bottom * MM_TO_PT
                    }
                    _ => 0.0,
                }
            }).fold(0.0f32, f32::max).max(12.0);

            if self.cursor_y + row_height > CONTENT_BOTTOM && self.cursor_y > CONTENT_TOP {
                self.new_page();
            }

            // Фон строки: явный background-color строки ИЛИ фон header по умолчанию
            let row_bg = row_styles.background
                .or_else(|| if row_num == 0 { Some(Color::from_hex(0xF5F5F5)) } else { None });

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

            // Ячейки строки
            for &cell_idx in &cell_indices {
                let cell = self.layout.nodes[cell_idx].clone();
                let cell_styles = self.styled.get(cell.arena_id).styles.clone();
                let bold = cell_styles.font_weight == FontWeight::Bold;
                let padding_top  = cell_styles.padding.top  * MM_TO_PT;
                let padding_left = cell_styles.padding.left * MM_TO_PT;

                // Фон отдельной ячейки
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

                if let LayoutContent::Text(text) = &cell.content {
                    let w = cell.width.max(1.0);
                    let lines = break_text(text, w, cell_styles.font_size, cell_styles.line_height, bold);
                    for (i, line) in lines.iter().enumerate() {
                        let y = row_start_y + padding_top + cell_styles.font_size
                                + i as f32 * line.line_height_pt;
                        if y > CONTENT_BOTTOM { break; }
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
            }

            self.cursor_y += row_height;

            // Разделитель под строкой
            self.push_cmd(DrawCommand::Line {
                x1: table_x,
                y1: self.cursor_y,
                x2: table_x + table_w,
                y2: self.cursor_y,
                color: Color::from_hex(0xDDDDDD),
                width: 0.3,
            });
        }

        // Рамка таблицы
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

    // ─── Оценка высоты блока (для фонового прямоугольника) ─────────────────

    fn estimate_height(&self, node_idx: LayoutNodeIdx) -> f32 {
        let node = &self.layout.nodes[node_idx];
        let styles = self.styled.get(node.arena_id).styles.clone();
        let bold = styles.font_weight == FontWeight::Bold;
        match &node.content {
            LayoutContent::Text(t) => {
                let w = node.width.max(CONTENT_WIDTH_PT * 0.3);
                let lines = break_text(t, w, styles.font_size, styles.line_height, bold);
                text_block_height(&lines)
            }
            LayoutContent::Children(children) => {
                children.iter().map(|&ci| self.estimate_height(ci)).sum()
            }
            LayoutContent::Empty => 0.0,
        }
    }
}

// ─── Публичный API ────────────────────────────────────────────────────────────

pub fn paginate(layout: &LayoutTree, styled: &super::arena::DocumentArena) -> PageTree {
    let mut pager = Paginator::new(layout, styled);

    for &root_idx in &layout.roots {
        let node = &layout.nodes[root_idx];
        match &node.content {
            LayoutContent::Children(children) => {
                let children = children.clone();
                for child_idx in children {
                    pager.place_node(child_idx);
                }
            }
            _ => { pager.place_node(root_idx); }
        }
    }

    PageTree { pages: pager.pages }
}
