/// Шрифты и разбивка текста на строки.
///
/// Измерение ширины символов — через ttf-parser с реальными метриками шрифта.
/// Загружается один раз в OnceLock при первом обращении.
/// Если системный шрифт не найден — fallback на коэффициент 0.55.

use std::collections::HashMap;
use std::sync::OnceLock;
use fontdb::{Database, Family, Query, Weight, Style as FontdbStyle, Stretch};
use super::styles::{FontWeight, FontStyle};
use super::layout::MM_TO_PT;

// ─── Глобальный кеш метрик ────────────────────────────────────────────────────

struct GlyphMetrics {
    advances: HashMap<char, u16>,
    units_per_em: u16,
}

static METRICS_REGULAR: OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static METRICS_BOLD:    OnceLock<Option<GlyphMetrics>> = OnceLock::new();

fn load_metrics(bold: bool) -> Option<GlyphMetrics> {
    let mut db = Database::new();
    db.load_system_fonts();

    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let id = db.query(&Query {
        families: &[
            Family::Name("Helvetica Neue"),
            Family::Name("Helvetica"),
            Family::Name("Arial"),
            Family::SansSerif,
        ],
        weight,
        style: FontdbStyle::Normal,
        stretch: Stretch::Normal,
    })?;

    let mut result: Option<GlyphMetrics> = None;
    db.with_face_data(id, |data, face_idx| {
        if let Ok(face) = ttf_parser::Face::parse(data, face_idx) {
            let units_per_em = face.units_per_em();
            let mut advances = HashMap::with_capacity(512);
            // Кешируем ASCII + Latin Extended (покрывает немецкие умлауты и типографику)
            for code in 32u32..1024u32 {
                if let Some(ch) = char::from_u32(code) {
                    if let Some(gid) = face.glyph_index(ch) {
                        if let Some(adv) = face.glyph_hor_advance(gid) {
                            advances.insert(ch, adv);
                        }
                    }
                }
            }
            // Bullet и типографские символы
            for ch in ['•', '–', '—', '…', '"', '"', '€', '©', '®'] {
                if let Some(gid) = face.glyph_index(ch) {
                    if let Some(adv) = face.glyph_hor_advance(gid) {
                        advances.insert(ch, adv);
                    }
                }
            }
            result = Some(GlyphMetrics { advances, units_per_em });
        }
    });
    result
}

fn get_metrics(bold: bool) -> Option<&'static GlyphMetrics> {
    let lock: &OnceLock<Option<GlyphMetrics>> = if bold { &METRICS_BOLD } else { &METRICS_REGULAR };
    lock.get_or_init(|| load_metrics(bold)).as_ref()
}

/// Возвращает горизонтальное смещение символа в pt при заданном размере шрифта.
pub fn char_advance_pt(ch: char, font_size_pt: f32, bold: bool) -> f32 {
    if let Some(m) = get_metrics(bold) {
        if let Some(&adv) = m.advances.get(&ch) {
            return adv as f32 / m.units_per_em as f32 * font_size_pt;
        }
    }
    // Fallback: консервативная аппроксимация
    font_size_pt * 0.55
}

/// Возвращает ширину строки в pt.
pub fn text_width_pt(text: &str, font_size_pt: f32, bold: bool) -> f32 {
    text.chars().map(|c| char_advance_pt(c, font_size_pt, bold)).sum()
}

// ─── Font Database ────────────────────────────────────────────────────────────

pub struct FontManager {
    pub db: Database,
}

impl FontManager {
    pub fn load() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db }
    }

    pub fn find_font(
        &self,
        family: &str,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<fontdb::ID> {
        let weight_val = match weight {
            FontWeight::Bold   => Weight::BOLD,
            FontWeight::Normal => Weight::NORMAL,
        };
        let style_val = match style {
            FontStyle::Italic => FontdbStyle::Italic,
            FontStyle::Normal => FontdbStyle::Normal,
        };

        let query = Query {
            families: &[Family::Name(family), Family::SansSerif],
            weight: weight_val,
            style: style_val,
            stretch: Stretch::Normal,
        };

        self.db.query(&query)
    }
}

// ─── Текстовые строки ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    pub width: f32,
    pub line_height_pt: f32,
    pub font_size: f32,
}

/// Разбивает текст на строки по ширине контейнера.
/// Использует реальные метрики шрифта (через GlyphMetrics) если доступны.
pub fn break_text(
    text: &str,
    max_width_pt: f32,
    font_size_pt: f32,
    line_height: f32,
    bold: bool,
) -> Vec<TextLine> {
    if text.is_empty() {
        return vec![];
    }

    let line_h = font_size_pt * line_height;

    let break_opportunities = unicode_linebreak::linebreaks(text).collect::<Vec<_>>();

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0f32;
    let mut last_pos = 0usize;

    for (pos, opportunity) in &break_opportunities {
        let segment = &text[last_pos..*pos];
        let segment_width: f32 = segment.chars()
            .map(|c| char_advance_pt(c, font_size_pt, bold))
            .sum();

        if current_width + segment_width > max_width_pt && !current_line.is_empty() {
            let w = text_width_pt(current_line.trim_end(), font_size_pt, bold);
            lines.push(TextLine {
                text: current_line.trim_end().to_string(),
                width: w.min(max_width_pt),
                line_height_pt: line_h,
                font_size: font_size_pt,
            });
            current_line = String::new();
            current_width = 0.0;
        }

        current_line.push_str(segment);
        current_width += segment_width;

        if *opportunity == unicode_linebreak::BreakOpportunity::Mandatory {
            let w = text_width_pt(current_line.trim_end(), font_size_pt, bold);
            lines.push(TextLine {
                text: current_line.trim_end().to_string(),
                width: w.min(max_width_pt),
                line_height_pt: line_h,
                font_size: font_size_pt,
            });
            current_line = String::new();
            current_width = 0.0;
        }

        last_pos = *pos;
    }

    if !current_line.trim().is_empty() {
        let w = text_width_pt(current_line.trim_end(), font_size_pt, bold);
        lines.push(TextLine {
            text: current_line.trim_end().to_string(),
            width: w.min(max_width_pt),
            line_height_pt: line_h,
            font_size: font_size_pt,
        });
    }

    lines
}

/// Высота текстового блока: baseline первой строки + (N-1) × line_height.
pub fn text_block_height(lines: &[TextLine]) -> f32 {
    if lines.is_empty() {
        return 0.0;
    }
    let first = &lines[0];
    first.font_size + (lines.len().saturating_sub(1)) as f32 * first.line_height_pt
}

#[allow(dead_code)]
pub fn mm_to_pt(mm: f32) -> f32 {
    mm * MM_TO_PT
}
