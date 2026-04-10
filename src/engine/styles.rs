use super::arena::NodeId;

/// Вид блока — определяет семантику при лэйауте.
#[derive(Debug, Clone, PartialEq)]
pub enum BoxKind {
    Page,
    Heading(u8),    // 1-6
    Paragraph,
    Table,
    Row,
    Cell,
    Grid,
    Image,
    Code,
    Quote,
    List,
    ListItem,
    Unknown(String),
}

impl BoxKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "PAGE"  => BoxKind::Page,
            "H1"    => BoxKind::Heading(1),
            "H2"    => BoxKind::Heading(2),
            "H3"    => BoxKind::Heading(3),
            "H4"    => BoxKind::Heading(4),
            "H5"    => BoxKind::Heading(5),
            "H6"    => BoxKind::Heading(6),
            "P"     => BoxKind::Paragraph,
            "TABLE" => BoxKind::Table,
            "ROW"   => BoxKind::Row,
            "CELL"  => BoxKind::Cell,
            "GRID"  => BoxKind::Grid,
            "IMAGE" => BoxKind::Image,
            "CODE"  => BoxKind::Code,
            "QUOTE" => BoxKind::Quote,
            "LIST"  => BoxKind::List,
            "ITEM"  => BoxKind::ListItem,
            other   => BoxKind::Unknown(other.to_string()),
        }
    }

    /// Является ли блок строчным (inline) контейнером текста
    pub fn is_text_container(&self) -> bool {
        matches!(
            self,
            BoxKind::Heading(_)
                | BoxKind::Paragraph
                | BoxKind::Cell
                | BoxKind::ListItem
                | BoxKind::Quote
                | BoxKind::Code
        )
    }
}

/// Абсолютно разрешённые стили узла.
/// После резолвинга здесь нет переменных (#var), нет процентов — только
/// конкретные значения в pt/mm.
#[derive(Debug, Clone)]
pub struct ResolvedStyles {
    /// Размер шрифта в pt
    pub font_size: f32,
    pub font_family: String,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,

    pub color: Color,
    pub background: Option<Color>,

    pub margin: EdgeInsets,
    pub padding: EdgeInsets,

    /// Ширина блока (если задана явно)
    pub width: Option<f32>,
    /// Высота блока (если задана явно)
    pub height: Option<f32>,

    /// Межстрочный интервал (коэффициент × font_size)
    pub line_height: f32,

    /// Выравнивание текста
    pub text_align: TextAlign,

    /// Тип дисплея (block / grid / flex)
    pub display: Display,

    /// Кол-во колонок в grid (применимо к GRID / TABLE)
    pub grid_columns: Option<usize>,

    /// Колонный gap в mm
    pub column_gap: f32,
    pub row_gap: f32,
}

impl Default for ResolvedStyles {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            font_family: "Helvetica".to_string(),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            color: Color::BLACK,
            background: None,
            margin: EdgeInsets::new(0.0, 0.0, 4.0, 0.0),  // bottom 4mm
            padding: EdgeInsets::zero(),
            width: None,
            height: None,
            line_height: 1.4,
            text_align: TextAlign::Left,
            display: Display::Block,
            grid_columns: None,
            column_gap: 4.0,
            row_gap: 2.0,
        }
    }
}

impl ResolvedStyles {
    /// Сенсибл-дефолты для конкретного вида блока
    pub fn for_kind(kind: &BoxKind) -> Self {
        let mut s = Self::default();
        match kind {
            BoxKind::Heading(1) => {
                s.font_size = 18.0;
                s.font_weight = FontWeight::Bold;
                s.margin = EdgeInsets::new(0.0, 0.0, 5.0, 0.0);
            }
            BoxKind::Heading(2) => {
                s.font_size = 12.0;
                s.font_weight = FontWeight::Bold;
                s.margin = EdgeInsets::new(6.0, 0.0, 3.0, 0.0);
            }
            BoxKind::Heading(3) => {
                s.font_size = 11.0;
                s.font_weight = FontWeight::Bold;
                s.margin = EdgeInsets::new(4.0, 0.0, 2.0, 0.0);
            }
            BoxKind::Heading(_) => {
                s.font_size = 11.0;
                s.font_weight = FontWeight::Bold;
                s.margin = EdgeInsets::new(3.0, 0.0, 2.0, 0.0);
            }
            BoxKind::Cell => {
                s.padding = EdgeInsets::uniform(2.0);
            }
            BoxKind::Code => {
                s.font_family = "Courier".to_string();
                s.font_size = 10.0;
                s.background = Some(Color::from_hex(0xF5F5F5));
                s.padding = EdgeInsets::uniform(4.0);
            }
            BoxKind::Quote => {
                s.margin = EdgeInsets::new(0.0, 8.0, 4.0, 8.0);
                s.color = Color::from_hex(0x666666);
            }
            _ => {}
        }
        s
    }
}

/// Контент узла — либо текст, либо список дочерних NodeId
#[derive(Debug, Clone)]
pub enum BoxContent {
    Text(String),
    Children(Vec<NodeId>),
    Empty,
}

/// Основная единица Styled Tree.
/// Содержит полностью разрешённые стили и ссылки на детей по NodeId.
#[derive(Debug, Clone)]
pub struct StyledBox {
    pub id: String,
    pub kind: BoxKind,
    pub styles: ResolvedStyles,
    pub content: BoxContent,
}

// ─── вспомогательные типы ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0 };

    pub fn from_hex(hex: u32) -> Self {
        Color {
            r: ((hex >> 16) & 0xFF) as f32 / 255.0,
            g: ((hex >> 8) & 0xFF) as f32 / 255.0,
            b: (hex & 0xFF) as f32 / 255.0,
        }
    }

    /// Разбирает строки вида "#FF0000", "#FFF", "red", "black"
    pub fn from_str(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        match s {
            "black"   => return Some(Color::BLACK),
            "white"   => return Some(Color::WHITE),
            "red"     => return Some(Color::from_hex(0xFF0000)),
            "green"   => return Some(Color::from_hex(0x008000)),
            "blue"    => return Some(Color::from_hex(0x0000FF)),
            "gray" | "grey" => return Some(Color::from_hex(0x808080)),
            _ => {}
        }
        if s.len() == 6 {
            u32::from_str_radix(s, 16).ok().map(Color::from_hex)
        } else if s.len() == 3 {
            let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
            Some(Color {
                r: r as f32 / 255.0,
                g: g as f32 / 255.0,
                b: b as f32 / 255.0,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeInsets {
    pub fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }

    pub fn uniform(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    /// top, right, bottom, left
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Grid,
    Flex,
    None,
}
