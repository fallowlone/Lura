use crate::engine::paginate::{DrawCommand, PageTree};
use crate::engine::styles::Color;

#[derive(Debug, Clone)]
pub enum PainterCommand {
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

#[derive(Debug, Clone)]
pub struct PaintedPage {
    pub width: f32,
    pub height: f32,
    pub commands: Vec<PainterCommand>,
}

#[derive(Debug, Clone)]
pub struct PaintDocument {
    pub pages: Vec<PaintedPage>,
}

pub trait PainterBackend {
    fn render_document(&self, doc: &PaintDocument) -> Vec<u8>;
}

pub fn from_page_tree(page_tree: &PageTree) -> PaintDocument {
    let pages = page_tree.pages.iter()
        .map(|page| PaintedPage {
            width: page.width,
            height: page.height,
            commands: page.commands.iter().map(map_command).collect(),
        })
        .collect();
    PaintDocument { pages }
}

fn map_command(cmd: &DrawCommand) -> PainterCommand {
    match cmd {
        DrawCommand::Rect { x, y, w, h, fill, stroke, stroke_width } => PainterCommand::Rect {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
            fill: *fill,
            stroke: *stroke,
            stroke_width: *stroke_width,
        },
        DrawCommand::Line { x1, y1, x2, y2, color, width } => PainterCommand::Line {
            x1: *x1,
            y1: *y1,
            x2: *x2,
            y2: *y2,
            color: *color,
            width: *width,
        },
        DrawCommand::Text {
            content,
            x,
            y,
            font_size,
            font_family,
            bold,
            italic,
            color,
        } => PainterCommand::Text {
            content: content.clone(),
            x: *x,
            y: *y,
            font_size: *font_size,
            font_family: font_family.clone(),
            bold: *bold,
            italic: *italic,
            color: *color,
        },
    }
}
