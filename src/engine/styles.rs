use super::arena::NodeId;
use super::grid_tracks::GridColumnTrack;

/// Block kind: drives layout semantics.
#[derive(Debug, Clone, PartialEq)]
pub enum BoxKind {
    Page,
    Heading(u8),    // 1-6
    Paragraph,
    Table,
    Row,
    Cell,
    Grid,
    /// Semantic figure: optional `IMAGE` child plus caption blocks; raster decode is not wired yet.
    Figure,
    Code,
    Quote,
    List,
    ListItem,
    Hr,
    Unknown(String),
}

/// List numbering style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListStyle {
    Bullet,
    Ordered,
}

impl BoxKind {
    pub fn parse(s: &str) -> Self {
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
            "GRID"   => BoxKind::Grid,
            "FIGURE" => BoxKind::Figure,
            "IMAGE"  => BoxKind::Figure,
            "CODE"  => BoxKind::Code,
            "QUOTE" => BoxKind::Quote,
            "LIST"  => BoxKind::List,
            "ITEM"  => BoxKind::ListItem,
            "HR"    => BoxKind::Hr,
            other   => BoxKind::Unknown(other.to_string()),
        }
    }

    /// Whether this block is an inline text container
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

/// Fully resolved styles for a node.
/// After resolution there are no `#var` references or percentages, only
/// concrete values in pt/mm.
#[derive(Debug, Clone)]
pub struct ResolvedStyles {
    /// Font size in pt
    pub font_size: f32,
    pub font_family: String,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,

    pub color: Color,
    pub background: Option<Color>,

    /// Block opacity 0…1 (Graphics 1.0). Not inherited; default 1.
    pub opacity: f32,
    /// Rectangular clip to estimated block bounds (`overflow: clip` / `hidden`).
    pub overflow_clip: bool,

    pub margin: EdgeInsets,
    pub padding: EdgeInsets,

    /// Block width when set explicitly
    pub width: Option<f32>,
    /// Block height when set explicitly
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,

    /// Line height (factor × font_size)
    pub line_height: f32,

    /// Text alignment
    pub text_align: TextAlign,
    /// Extra spacing between letters (pt)
    pub letter_spacing: f32,
    /// Extra spacing between words (pt)
    pub word_spacing: f32,
    /// Force justified text for text blocks
    pub justify: bool,
    pub keep_together: bool,
    pub keep_with_next: bool,
    /// When true, a table row is painted in place even if it overflows past
    /// the page bottom (content is clipped at the page edge). When false
    /// (default), the paginator forces a page break before rows that would
    /// overflow. True row splitting — continuing cell content on the next
    /// page — is not implemented; this flag only controls the break-vs-clip
    /// decision for an oversized row.
    pub allow_row_overflow: bool,

    /// Display type (block / grid / flex)
    pub display: Display,

    /// Explicit GRID column tracks (`columns` in FOL). Empty = one `1fr` column.
    pub grid_column_tracks: Vec<GridColumnTrack>,

    /// Column gap in mm
    pub column_gap: f32,
    pub row_gap: f32,

    /// flex-grow for table cells (column width proportions)
    pub flex_grow: f32,

    /// List numbering style
    pub list_style: ListStyle,
    pub float: FloatMode,
    pub anchor: Option<String>,
    pub page_header: Option<String>,
    pub page_footer: Option<String>,

    /// CELL vertical alignment inside its row.
    pub vertical_align: VerticalAlign,
    /// CELL column span (number of columns consumed). Default 1.
    pub cell_span: usize,
    /// CELL: suppress line breaking inside this cell.
    pub nowrap: bool,
    /// CELL: render a single line, clip with ellipsis if it overflows the inner width.
    pub truncate: bool,
    /// TABLE: per-column horizontal alignment fallback used when a cell has no
    /// explicit `align`. Empty = no per-column override.
    pub col_aligns: Vec<TextAlign>,
    /// Set by resolver when the block carries an explicit `text-align`/`align`
    /// attr (single-word form). Distinguishes user-specified alignment from the
    /// inherited/default `text_align`, which lets TABLE `col_aligns` act as a
    /// fallback only for cells that did not opt in themselves.
    pub explicit_text_align: Option<TextAlign>,
}

impl Default for ResolvedStyles {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            font_family: "Helvetica".to_string(),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            color: Color::BLACK,
            background: None,
            opacity: 1.0,
            overflow_clip: false,
            margin: EdgeInsets::new(0.0, 0.0, 2.5, 0.0),
            padding: EdgeInsets::zero(),
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            line_height: 1.3,
            text_align: TextAlign::Left,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            justify: false,
            keep_together: false,
            keep_with_next: false,
            allow_row_overflow: false,
            display: Display::Block,
            grid_column_tracks: Vec::new(),
            column_gap: 4.0,
            row_gap: 2.0,
            flex_grow: 0.0,
            list_style: ListStyle::Bullet,
            float: FloatMode::None,
            anchor: None,
            page_header: None,
            page_footer: None,
            vertical_align: VerticalAlign::Top,
            cell_span: 1,
            nowrap: false,
            truncate: false,
            col_aligns: Vec::new(),
            explicit_text_align: None,
        }
    }
}

impl ResolvedStyles {
    /// GRID column count for pagination (empty `grid_column_tracks` → 1).
    #[inline]
    pub fn grid_column_count(&self) -> usize {
        super::grid_tracks::grid_column_count(&self.grid_column_tracks)
    }

    /// Sensible defaults for a given block kind (parent-independent baseline).
    pub fn for_kind(kind: &BoxKind) -> Self {
        let mut s = Self::default();
        s.apply_kind_defaults(kind);
        s
    }

    /// Apply block-kind defaults on top of accumulated styles (e.g. after inheriting from the parent).
    pub fn apply_kind_defaults(&mut self, kind: &BoxKind) {
        match kind {
            BoxKind::Heading(1) => {
                self.font_size = 14.0;
                self.font_weight = FontWeight::Bold;
                self.margin = EdgeInsets::new(0.0, 0.0, 4.0, 0.0);
            }
            BoxKind::Heading(2) => {
                self.font_size = 10.5;
                self.font_weight = FontWeight::Bold;
                self.margin = EdgeInsets::new(5.0, 0.0, 2.0, 0.0);
            }
            BoxKind::Heading(3) => {
                self.font_size = 10.0;
                self.font_weight = FontWeight::Bold;
                self.margin = EdgeInsets::new(3.0, 0.0, 1.5, 0.0);
            }
            BoxKind::Heading(_) => {
                self.font_size = 10.0;
                self.font_weight = FontWeight::Bold;
                self.margin = EdgeInsets::new(3.0, 0.0, 1.5, 0.0);
            }
            BoxKind::Cell => {
                self.padding = EdgeInsets::new(1.5, 2.0, 1.5, 2.0);
                self.flex_grow = 1.0;
            }
            BoxKind::List => {
                self.padding = EdgeInsets::new(0.0, 0.0, 0.0, 6.0); // 6mm left indent for items
                self.margin = EdgeInsets::new(0.0, 0.0, 2.0, 0.0);
            }
            BoxKind::ListItem => {
                self.margin = EdgeInsets::new(0.0, 0.0, 1.5, 0.0);
            }
            BoxKind::Code => {
                self.font_family = "Courier".to_string();
                self.font_size = 10.0;
                self.background = Some(Color::from_hex(0xF5F5F5));
                self.padding = EdgeInsets::uniform(4.0);
            }
            BoxKind::Quote => {
                self.margin = EdgeInsets::new(0.0, 8.0, 4.0, 8.0);
                self.color = Color::from_hex(0x666666);
            }
            BoxKind::Hr => {
                self.margin = EdgeInsets::new(3.0, 0.0, 3.0, 0.0);
            }
            BoxKind::Figure => {
                self.margin = EdgeInsets::new(2.0, 0.0, 4.0, 0.0);
            }
            _ => {}
        }
    }
}

/// Node content: plain text or child `NodeId` list
#[derive(Debug, Clone)]
pub enum BoxContent {
    Text(String),
    Inline(Vec<InlineRun>),
    Children(Vec<NodeId>),
    Empty,
}

#[derive(Debug, Clone)]
pub struct InlineRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

/// Primary unit of the styled tree.
/// Holds fully resolved styles and child references by `NodeId`.
#[derive(Debug, Clone)]
pub struct StyledBox {
    pub id: String,
    pub kind: BoxKind,
    pub styles: ResolvedStyles,
    pub content: BoxContent,
}

// --- Helper types ---

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

    /// Parse strings like "#FF0000", "#FFF", "red", "black"
    pub fn parse(s: &str) -> Option<Self> {
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
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Grid,
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatMode {
    None,
    Left,
    Right,
}
