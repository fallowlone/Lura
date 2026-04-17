/// Fonts and line breaking for text.
///
/// Character widths use ttf-parser with real font metrics.
/// Loaded once in `OnceLock` on first use.
/// If no system font is found, falls back to a 0.55 width factor.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::sync::Arc;
use std::collections::hash_map::DefaultHasher;
use fontdb::{Database, Family, ID, Query, Weight, Style as FontdbStyle, Stretch};
use rustybuzz::UnicodeBuffer;
use super::layout::MM_TO_PT;
use super::styles::InlineRun;

// --- Global metrics cache ---

struct GlyphMetrics {
    advances: HashMap<char, u16>,
    units_per_em: u16,
}

#[derive(Clone)]
struct FontSource {
    data: Arc<[u8]>,
    face_index: u32,
}

struct FontSources {
    regular: Option<FontSource>,
    bold: Option<FontSource>,
    mono_regular: Option<FontSource>,
    mono_bold: Option<FontSource>,
}

static METRICS_REGULAR: OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static METRICS_BOLD:    OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static METRICS_MONO_REGULAR: OnceLock<Option<GlyphMetrics>> = OnceLock::new();
static METRICS_MONO_BOLD: OnceLock<Option<GlyphMetrics>> = OnceLock::new();
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
    mono: bool,
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
    // Cache ASCII + Latin Extended (covers German umlauts and typography)
    for code in 32u32..1024u32 {
        if let Some(ch) = char::from_u32(code)
            && let Some(gid) = face.glyph_index(ch)
                && let Some(adv) = face.glyph_hor_advance(gid) {
                    advances.insert(ch, adv);
                }
    }
    // Bullet and typographic symbols
    for ch in ['•', '–', '—', '…', '"', '"', '€', '©', '®'] {
        if let Some(gid) = face.glyph_index(ch)
            && let Some(adv) = face.glyph_hor_advance(gid) {
                advances.insert(ch, adv);
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

    let mono_reg_id = query_mono_font_id(&db, Weight::NORMAL);
    let mono_bold_id = query_mono_font_id(&db, Weight::BOLD);
    let mono_regular = mono_reg_id.and_then(|id| extract_font_source(&db, id));
    let mono_bold = match (mono_bold_id, mono_reg_id, mono_regular.as_ref()) {
        (Some(bold_m), Some(reg_m), Some(mreg_src)) if bold_m == reg_m => Some(FontSource {
            data: Arc::clone(&mreg_src.data),
            face_index: mreg_src.face_index,
        }),
        (Some(id), _, _) => extract_font_source(&db, id),
        _ => mono_regular.clone(),
    };

    FontSources {
        regular,
        bold,
        mono_regular,
        mono_bold,
    }
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

fn query_mono_font_id(db: &Database, weight: Weight) -> Option<ID> {
    db.query(&Query {
        families: &[
            Family::Name("Menlo"),
            Family::Name("Monaco"),
            Family::Name("Courier New"),
            Family::Name("Courier"),
            Family::Monospace,
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

fn get_font_source_mono(bold: bool) -> Option<&'static FontSource> {
    let sources = FONT_SOURCES.get_or_init(load_font_sources);
    if bold {
        sources
            .mono_bold
            .as_ref()
            .or(sources.mono_regular.as_ref())
    } else {
        sources.mono_regular.as_ref()
    }
}

fn load_metrics_mono(bold: bool) -> Option<GlyphMetrics> {
    let source = get_font_source_mono(bold)?;
    let face = ttf_parser::Face::parse(source.data.as_ref(), source.face_index).ok()?;
    let units_per_em = face.units_per_em();
    let mut advances = HashMap::with_capacity(512);
    for code in 32u32..1024u32 {
        if let Some(ch) = char::from_u32(code)
            && let Some(gid) = face.glyph_index(ch)
                && let Some(adv) = face.glyph_hor_advance(gid) {
                    advances.insert(ch, adv);
                }
    }
    for ch in ['•', '–', '—', '…', '"', '"', '€', '©', '®'] {
        if let Some(gid) = face.glyph_index(ch)
            && let Some(adv) = face.glyph_hor_advance(gid) {
                advances.insert(ch, adv);
            }
    }
    Some(GlyphMetrics { advances, units_per_em })
}

fn get_metrics_mono(bold: bool) -> Option<&'static GlyphMetrics> {
    let lock: &OnceLock<Option<GlyphMetrics>> = if bold {
        &METRICS_MONO_BOLD
    } else {
        &METRICS_MONO_REGULAR
    };
    lock.get_or_init(|| load_metrics_mono(bold)).as_ref()
}

/// Horizontal advance of a character in pt at the given font size.
pub fn char_advance_pt(ch: char, font_size_pt: f32, bold: bool) -> f32 {
    char_advance_pt_inner(ch, font_size_pt, bold, false)
}

fn char_advance_pt_inner(ch: char, font_size_pt: f32, bold: bool, mono: bool) -> f32 {
    let metrics = if mono {
        get_metrics_mono(bold).or_else(|| get_metrics_mono(false))
    } else {
        get_metrics(bold)
    };
    if let Some(m) = metrics
        && let Some(&adv) = m.advances.get(&ch) {
            return adv as f32 / m.units_per_em as f32 * font_size_pt;
        }
    // Fallback: monospace-ish vs proportional
    font_size_pt * if mono { 0.6 } else { 0.55 }
}

/// String width in pt.
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
    text_width_pt_with_spacing_inner(
        text,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
        false,
    )
}

/// Lower bound for horizontal advance when drawing with PDF built-in Courier (Core 14).
/// System monospace metrics (Menlo, etc.) are often narrower than Courier, which caused
/// layout to place the next fragment too early and overlap monospace runs.
pub(crate) fn courier_core14_width_floor_pt(text: &str, font_size_pt: f32) -> f32 {
    text.chars()
        .map(|c| {
            if c.is_whitespace() {
                font_size_pt * 0.35
            } else {
                font_size_pt * 0.60
            }
        })
        .sum()
}

fn text_width_pt_with_spacing_mono(
    text: &str,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> f32 {
    text_width_pt_with_spacing_inner(
        text,
        font_size_pt,
        bold,
        letter_spacing_pt,
        word_spacing_pt,
        true,
    )
}

fn text_width_pt_with_spacing_inner(
    text: &str,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
    mono: bool,
) -> f32 {
    let key = TextWidthKey {
        text_hash: stable_hash(text),
        text_len: text.len(),
        font_size_bits: font_size_pt.to_bits(),
        letter_spacing_bits: letter_spacing_pt.to_bits(),
        word_spacing_bits: word_spacing_pt.to_bits(),
        bold,
        mono,
    };

    if let Some(cached) = text_width_cache().lock().ok().and_then(|m| m.get(&key).copied()) {
        return cached;
    }

    let mut width = if let Some(shaped) = shape_text_width_pt(text, font_size_pt, bold, mono) {
        shaped
    } else {
        text
            .chars()
            .map(|c| char_advance_pt_inner(c, font_size_pt, bold, mono))
            .sum()
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

fn shape_text_width_pt(text: &str, font_size_pt: f32, bold: bool, mono: bool) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    let source = if mono {
        get_font_source_mono(bold).or_else(|| get_font_source_mono(false))?
    } else {
        get_font_source(bold)?
    };
    let rb_face = rustybuzz::Face::from_slice(source.data.as_ref(), source.face_index)?;
    let ttf_face = ttf_parser::Face::parse(source.data.as_ref(), source.face_index).ok()?;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let glyph_buffer = rustybuzz::shape(&rb_face, &[], buffer);
    let upem = ttf_face.units_per_em() as f32;
    if upem <= 0.0 {
        return None;
    }
    let infos = glyph_buffer.glyph_infos();
    if infos.is_empty() {
        return None;
    }
    let mut width_units = 0.0f32;
    for info in infos {
        let gid = ttf_parser::GlyphId(info.glyph_id as u16);
        if let Some(adv) = ttf_face.glyph_hor_advance(gid) {
            width_units += adv as f32;
        } else {
            width_units += upem * 0.55;
        }
    }
    let w = width_units / upem * font_size_pt;
    // HarfBuzz can yield zero total advance for some runs (e.g. lone spaces on certain faces).
    // Fall back to per-glyph metrics so inline fragments keep real spacing.
    if w < 1e-3 {
        return None;
    }
    Some(w)
}

// --- Text lines ---

#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    pub width: f32,
    pub line_height_pt: f32,
    pub font_size: f32,
}

/// Break text into lines to fit container width.
/// Uses real font metrics (`GlyphMetrics`) when available.
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
    let mut last_pos = 0usize;

    for (pos, opportunity) in &break_opportunities {
        let segment = &text[last_pos..*pos];

        // Tentatively add segment to current line
        let tentative = if current_line.is_empty() {
            segment.to_string()
        } else {
            format!("{}{}", current_line, segment)
        };
        let tentative_width = text_width_pt_with_spacing(
            tentative.trim_end(),
            font_size_pt,
            bold,
            letter_spacing_pt,
            word_spacing_pt,
        );

        // If adding this segment exceeds max width, finalize current line first
        if tentative_width > max_width_pt && !current_line.is_empty() {
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
        }

        current_line.push_str(segment);

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

/// Text block height: first-line baseline + (N-1) × line_height.
pub fn text_block_height(lines: &[TextLine]) -> f32 {
    if lines.is_empty() {
        return 0.0;
    }
    let first = &lines[0];
    first.font_size + (lines.len().saturating_sub(1)) as f32 * first.line_height_pt
}

/// Maximum width of a single whitespace-delimited token (for min-content probes).
pub fn max_word_width_pt(
    text: &str,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> f32 {
    text.split_whitespace()
        .map(|w| {
            text_width_pt_with_spacing(w, font_size_pt, bold, letter_spacing_pt, word_spacing_pt)
        })
        .fold(0.0f32, f32::max)
        .max(1.0)
}

/// Block height from already-wrapped inline lines (matches paginator math).
pub fn inline_lines_block_height(lines: &[InlineLine], font_size_pt: f32, line_height: f32) -> f32 {
    if lines.is_empty() {
        0.0
    } else {
        font_size_pt + (lines.len().saturating_sub(1)) as f32 * (font_size_pt * line_height)
    }
}

/// Layout options for text wrapping and measurement.
pub struct TextLayoutOpts {
    pub font_size_pt: f32,
    pub line_height: f32,
    pub letter_spacing_pt: f32,
    pub word_spacing_pt: f32,
    pub base_bold: bool,
    pub justify: bool,
}

/// Block height for inline runs after wrapping to `max_width_pt` (matches paginator math).
pub fn inline_runs_block_height(
    runs: &[InlineRun],
    max_width_pt: f32,
    opts: &TextLayoutOpts,
) -> f32 {
    let lines = break_inline_runs(
        runs,
        max_width_pt,
        opts,
    );
    inline_lines_block_height(&lines, opts.font_size_pt, opts.line_height)
}

/// Widest line when wrapping is effectively disabled (max-content width probe).
pub fn inline_runs_intrinsic_max_line_width_pt(
    runs: &[InlineRun],
    font_size_pt: f32,
    line_height: f32,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
    base_bold: bool,
) -> f32 {
    const HUGE: f32 = 1_000_000.0;
    let lines = break_inline_runs(
        runs,
        HUGE,
        &TextLayoutOpts {
            font_size_pt,
            line_height,
            letter_spacing_pt,
            word_spacing_pt,
            base_bold,
            justify: false,
        },
    );
    lines.iter().map(|l| l.width).fold(0.0f32, f32::max).max(1.0)
}

/// Max word width across runs (min-content width heuristic for inline).
pub fn max_word_width_across_runs(
    runs: &[InlineRun],
    font_size_pt: f32,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
    base_bold: bool,
) -> f32 {
    runs.iter()
        .map(|r| {
            let b = base_bold || r.bold;
            if r.code {
                max_word_width_mono(
                    &r.text,
                    font_size_pt,
                    b,
                    letter_spacing_pt,
                    word_spacing_pt,
                )
            } else {
                max_word_width_pt(
                    &r.text,
                    font_size_pt,
                    b,
                    letter_spacing_pt,
                    word_spacing_pt,
                )
            }
        })
        .fold(0.0f32, f32::max)
        .max(1.0)
}

fn max_word_width_mono(
    text: &str,
    font_size_pt: f32,
    bold: bool,
    letter_spacing_pt: f32,
    word_spacing_pt: f32,
) -> f32 {
    text.split_whitespace()
        .map(|w| {
            let m = text_width_pt_with_spacing_mono(w, font_size_pt, bold, letter_spacing_pt, word_spacing_pt);
            m.max(courier_core14_width_floor_pt(w, font_size_pt))
        })
        .fold(0.0f32, f32::max)
        .max(1.0)
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
    /// Full line text (all fragments concatenated). Used for accurate shaping/rendering
    /// to avoid micro-gaps from per-fragment PDF text operators.
    pub full_text: String,
}

pub fn break_inline_runs(
    runs: &[InlineRun],
    max_width_pt: f32,
    opts: &TextLayoutOpts,
) -> Vec<InlineLine> {
    let mut lines: Vec<InlineLine> = Vec::new();
    let mut current = InlineLine {
        fragments: Vec::new(),
        width: 0.0,
        line_height_pt: opts.font_size_pt * opts.line_height,
        font_size: opts.font_size_pt,
        full_text: String::new(),
    };

    for run in runs {
        let mut parts = split_preserving_spaces(&run.text);
        if parts.is_empty() {
            parts.push(run.text.clone());
        }
        for part in parts {
            let measure_bold = opts.base_bold || run.bold;
            let width = if run.code {
                let m = text_width_pt_with_spacing_mono(
                    &part,
                    opts.font_size_pt,
                    measure_bold,
                    opts.letter_spacing_pt,
                    opts.word_spacing_pt,
                );
                m.max(courier_core14_width_floor_pt(&part, opts.font_size_pt))
            } else {
                text_width_pt_with_spacing(
                    &part,
                    opts.font_size_pt,
                    measure_bold,
                    opts.letter_spacing_pt,
                    opts.word_spacing_pt,
                )
            };

            let should_wrap = current.width + width > max_width_pt
                && !current.fragments.is_empty()
                && !part.trim().is_empty();

            if should_wrap {
                // Compute full_text for the completed line
                current.full_text = current.fragments.iter().map(|f| f.text.as_str()).collect();
                lines.push(current);
                current = InlineLine {
                    fragments: Vec::new(),
                    width: 0.0,
                    line_height_pt: opts.font_size_pt * opts.line_height,
                    font_size: opts.font_size_pt,
                    full_text: String::new(),
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
        current.full_text = current.fragments.iter().map(|f| f.text.as_str()).collect();
        lines.push(current);
    }

    if opts.justify {
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

#[cfg(test)]
mod space_width_tests {
    use super::*;
    use crate::engine::styles::InlineRun;

    #[test]
    fn whitespace_fragments_have_nonzero_width_with_base_bold() {
        let runs = vec![InlineRun {
            text: "Lura Capability Showcase 2026".into(),
            bold: false,
            italic: false,
            code: false,
            link: None,
        }];
        let lines = break_inline_runs(&runs, 100_000.0, &TextLayoutOpts {
            font_size_pt: 12.0,
            line_height: 1.2,
            letter_spacing_pt: 0.0,
            word_spacing_pt: 0.0,
            base_bold: true,
            justify: false,
        });
        let frags = &lines[0].fragments;
        for f in frags {
            assert!(
                f.width > 1e-4,
                "zero-width fragment {:?} (base_bold heading-like)",
                f.text
            );
        }
    }

    /// Regression: if bold word width is under-measured vs real glyphs, PDF fragments overlap.
    #[test]
    fn bold_lura_width_matches_typical_helvetica_bold_advance() {
        let w = text_width_pt("Lura", 14.0, true);
        assert!(
            w > 18.0,
            "bold 'Lura' at 14pt should be ~25–35pt wide; got {w} (check font shaping / metrics)"
        );
    }

    #[test]
    fn h1_sized_break_inline_first_word_wide_enough() {
        let runs = vec![InlineRun {
            text: "Lura Capability".into(),
            bold: false,
            italic: false,
            code: false,
            link: None,
        }];
        let lines = break_inline_runs(&runs, 1_000_000.0, &TextLayoutOpts {
            font_size_pt: 14.0,
            line_height: 1.2,
            letter_spacing_pt: 0.0,
            word_spacing_pt: 0.0,
            base_bold: true,
            justify: false,
        });
        let w0 = lines[0].fragments[0].width;
        assert!(
            w0 > 18.0,
            "first fragment width {w0} with font14 base_bold (H1-like)"
        );
    }
}
