/// Шрифты и разбивка текста на строки.
///
/// Измерение ширины символов — через ttf-parser с реальными метриками шрифта.
/// Загружается один раз в OnceLock при первом обращении.
/// Если системный шрифт не найден — fallback на коэффициент 0.55.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::sync::Arc;
use std::collections::hash_map::DefaultHasher;
use fontdb::{Database, Family, ID, Query, Weight, Style as FontdbStyle, Stretch};
use rustybuzz::UnicodeBuffer;
use super::layout::MM_TO_PT;
use super::styles::InlineRun;

// ─── Глобальный кеш метрик ────────────────────────────────────────────────────

struct GlyphMetrics {
    advances: HashMap<char, u16>,
    units_per_em: u16,
}

struct FontSource {
    data: Arc<[u8]>,
    face_index: u32,
}

struct FontSources {
    regular: Option<FontSource>,
    bold: Option<FontSource>,
}

static METRICS_REGULAR: OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static METRICS_BOLD:    OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static FONT_SOURCES:    OnceLock<FontSources> = OnceLock::new();
static TEXT_WIDTH_CACHE: OnceLock<Mutex<HashMap<TextWidthKey, f32>>> = OnceLock::new();
static BREAK_TEXT_CACHE: OnceLock<Mutex<HashMap<BreakTextKey, Vec<TextLine>>>> = OnceLock::new();

const TEXT_WIDTH_CACHE_LIMIT: usize = 8192;
const BREAK_TEXT_CACHE_LIMIT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextWidthKey {
    text_hash: u64,
    text_len: usize,
    font_size_bits: u32,
    letter_spacing_bits: u32,
    word_spacing_bits: u32,
    bold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BreakTextKey {
    text_hash: u64,
    text_len: usize,
    max_width_bits: u32,
    font_size_bits: u32,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    word_spacing_bits: u32,
    bold: bool,
}

fn load_metrics(bold: bool) -> Option<GlyphMetrics> {
    let source = get_font_source(bold)?;
    let face = ttf_parser::Face::parse(source.data.as_ref(), source.face_index).ok()?;
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

    Some(GlyphMetrics { advances, units_per_em })
}

fn load_font_sources() -> FontSources {
    let mut db = Database::new();
    db.load_system_fonts();

    let regular_id = query_font_id(&db, Weight::NORMAL);
    let bold_id = query_font_id(&db, Weight::BOLD);

    let regular = regular_id.and_then(|id| extract_font_source(&db, id));
    let bold = match (bold_id, regular_id, regular.as_ref()) {
        (Some(bold), Some(reg), Some(regular_source)) if bold == reg => Some(FontSource {
            data: Arc::clone(&regular_source.data),
            face_index: regular_source.face_index,
        }),
        (Some(id), _, _) => extract_font_source(&db, id),
        _ => None,
    };

    FontSources { regular, bold }
}

fn query_font_id(db: &Database, weight: Weight) -> Option<ID> {
    db.query(&Query {
        families: &[
            Family::Name("Helvetica Neue"),
            Family::Name("Helvetica"),
            Family::Name("Arial"),
            Family::SansSerif,
        ],
        weight,
        style: FontdbStyle::Normal,
        stretch: Stretch::Normal,
    })
}

fn extract_font_source(db: &Database, id: ID) -> Option<FontSource> {
    let mut result: Option<FontSource> = None;
    db.with_face_data(id, |data, face_idx| {
        result = Some(FontSource {
            data: Arc::from(data.to_vec()),
            face_index: face_idx,
        });
    });
    result
}

fn get_metrics(bold: bool) -> Option<&'static GlyphMetrics> {
    let lock: &OnceLock<Option<GlyphMetrics>> = if bold { &METRICS_BOLD } else { &METRICS_REGULAR };
    lock.get_or_init(|| load_metrics(bold)).as_ref()
}

fn get_font_source(bold: bool) -> Option<&'static FontSource> {
    let sources = FONT_SOURCES.get_or_init(load_font_sources);
    if bold {
        sources.bold.as_ref()
    } else {
        sources.regular.as_ref()
    }
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
    text_width_pt_with_spacing(text, font_size_pt, bold, 0.0, 0.0)
}

pub fn text_width_pt_with_spacing(
    text: &str,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> f32 {
    let key = TextWidthKey {
        text_hash: stable_hash(text),
        text_len: text.len(),
        font_size_bits: font_size_pt.to_bits(),
        letter_spacing_bits: letter_spacing_pt.to_bits(),
        word_spacing_bits: word_spacing_pt.to_bits(),
        bold,
    };

    if let Some(cached) = text_width_cache().lock().ok().and_then(|m| m.get(&key).copied()) {
        return cached;
    }

    let mut width = if let Some(shaped) = shape_text_width_pt(text, font_size_pt, bold) {
        shaped
    } else {
        text.chars().map(|c| char_advance_pt(c, font_size_pt, bold)).sum()
    };

    if letter_spacing_pt != 0.0 {
        let count = text.chars().count().saturating_sub(1) as f32;
        width += letter_spacing_pt * count;
    }
    if word_spacing_pt != 0.0 {
        let spaces = text.chars().filter(|c| *c == ' ').count() as f32;
        width += word_spacing_pt * spaces;
    }

    cache_text_width(key, width);
    width
}

fn shape_text_width_pt(text: &str, font_size_pt: f32, bold: bool) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    let source = get_font_source(bold)?;
    let rb_face = rustybuzz::Face::from_slice(source.data.as_ref(), source.face_index)?;
    let ttf_face = ttf_parser::Face::parse(source.data.as_ref(), source.face_index).ok()?;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let glyph_buffer = rustybuzz::shape(&rb_face, &[], buffer);
    let upem = ttf_face.units_per_em() as f32;
    if upem <= 0.0 {
        return None;
    }
    let mut width_units = 0.0f32;
    for info in glyph_buffer.glyph_infos() {
        let gid = ttf_parser::GlyphId(info.glyph_id as u16);
        if let Some(adv) = ttf_face.glyph_hor_advance(gid) {
            width_units += adv as f32;
        } else {
            width_units += upem * 0.55;
        }
    }
    Some(width_units / upem * font_size_pt)
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
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> Vec<TextLine> {
    if text.is_empty() {
        return vec![];
    }

    let key = BreakTextKey {
        text_hash: stable_hash(text),
        text_len: text.len(),
        max_width_bits: max_width_pt.to_bits(),
        font_size_bits: font_size_pt.to_bits(),
        line_height_bits: line_height.to_bits(),
        letter_spacing_bits: letter_spacing_pt.to_bits(),
        word_spacing_bits: word_spacing_pt.to_bits(),
        bold,
    };
    if let Some(cached) = break_text_cache().lock().ok().and_then(|m| m.get(&key).cloned()) {
        return cached;
    }

    let line_h = font_size_pt * line_height;

    let break_opportunities = unicode_linebreak::linebreaks(text).collect::<Vec<_>>();

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0f32;
    let mut last_pos = 0usize;

    for (pos, opportunity) in &break_opportunities {
        let segment = &text[last_pos..*pos];
        let segment_width = text_width_pt_with_spacing(
            segment,
            font_size_pt,
            bold,
            letter_spacing_pt,
            word_spacing_pt,
        );

        if current_width + segment_width > max_width_pt && !current_line.is_empty() {
            let w = text_width_pt_with_spacing(
                current_line.trim_end(),
                font_size_pt,
                bold,
                letter_spacing_pt,
                word_spacing_pt,
            );
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
            let w = text_width_pt_with_spacing(
                current_line.trim_end(),
                font_size_pt,
                bold,
                letter_spacing_pt,
                word_spacing_pt,
            );
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
        let w = text_width_pt_with_spacing(
            current_line.trim_end(),
            font_size_pt,
            bold,
            letter_spacing_pt,
            word_spacing_pt,
        );
        lines.push(TextLine {
            text: current_line.trim_end().to_string(),
            width: w.min(max_width_pt),
            line_height_pt: line_h,
            font_size: font_size_pt,
        });
    }

    cache_break_text(key, &lines);
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

#[derive(Debug, Clone)]
pub struct InlineFragment {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
    pub width: f32,
}

#[derive(Debug, Clone)]
pub struct InlineLine {
    pub fragments: Vec<InlineFragment>,
    pub width: f32,
    pub line_height_pt: f32,
    pub font_size: f32,
}

pub fn break_inline_runs(
    runs: &[InlineRun],
    max_width_pt: f32,
    font_size_pt: f32,
    line_height: f32,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
    justify: bool,
) -> Vec<InlineLine> {
    let mut lines: Vec<InlineLine> = Vec::new();
    let mut current = InlineLine {
        fragments: Vec::new(),
        width: 0.0,
        line_height_pt: font_size_pt * line_height,
        font_size: font_size_pt,
    };

    for run in runs {
        let mut parts = split_preserving_spaces(&run.text);
        if parts.is_empty() {
            parts.push(run.text.clone());
        }
        for part in parts {
            let bold = run.bold;
            let width = text_width_pt_with_spacing(
                &part,
                font_size_pt,
                bold,
                letter_spacing_pt,
                word_spacing_pt,
            );

            let should_wrap = current.width + width > max_width_pt
                && !current.fragments.is_empty()
                && !part.trim().is_empty();

            if should_wrap {
                lines.push(current);
                current = InlineLine {
                    fragments: Vec::new(),
                    width: 0.0,
                    line_height_pt: font_size_pt * line_height,
                    font_size: font_size_pt,
                };
            }

            current.fragments.push(InlineFragment {
                text: part.clone(),
                bold: run.bold,
                italic: run.italic,
                code: run.code,
                link: run.link.clone(),
                width,
            });
            current.width += width;
        }
    }

    if !current.fragments.is_empty() {
        lines.push(current);
    }

    if justify {
        let justified_line_count = lines.len().saturating_sub(1);
        for line in lines.iter_mut().take(justified_line_count) {
            let spaces = line.fragments.iter()
                .map(|f| f.text.chars().filter(|c| *c == ' ').count())
                .sum::<usize>();
            if spaces == 0 || line.width >= max_width_pt {
                continue;
            }
            let extra = (max_width_pt - line.width) / spaces as f32;
            for frag in &mut line.fragments {
                let space_count = frag.text.chars().filter(|c| *c == ' ').count() as f32;
                if space_count > 0.0 {
                    let add = extra * space_count;
                    frag.width += add;
                }
            }
            line.width = max_width_pt;
        }
    }

    lines
}

#[allow(dead_code)]
pub fn mm_to_pt(mm: f32) -> f32 {
    mm * MM_TO_PT
}

fn text_width_cache() -> &'static Mutex<HashMap<TextWidthKey, f32>> {
    TEXT_WIDTH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn break_text_cache() -> &'static Mutex<HashMap<BreakTextKey, Vec<TextLine>>> {
    BREAK_TEXT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_text_width(key: TextWidthKey, width: f32) {
    if let Ok(mut map) = text_width_cache().lock() {
        if map.len() >= TEXT_WIDTH_CACHE_LIMIT {
            map.clear();
        }
        map.insert(key, width);
    }
}

fn cache_break_text(key: BreakTextKey, lines: &[TextLine]) {
    if let Ok(mut map) = break_text_cache().lock() {
        if map.len() >= BREAK_TEXT_CACHE_LIMIT {
            map.clear();
        }
        map.insert(key, lines.to_vec());
    }
}

fn stable_hash(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn split_preserving_spaces(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_space && !current.is_empty() {
                out.push(current.clone());
                current.clear();
            }
            in_space = true;
            current.push(ch);
        } else {
            if in_space && !current.is_empty() {
                out.push(current.clone());
                current.clear();
            }
            in_space = false;
            current.push(ch);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
