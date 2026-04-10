/// Фаза 3: Шрифты и разбивка текста на строки
///
/// - `fontdb` — загружает системные .ttf/.otf файлы, строит базу данных шрифтов
/// - `unicode-linebreak` — находит допустимые позиции переноса строк (Unicode UAX #14)
///
/// `rustybuzz` (full text shaping с глифами) вынесен в будущую итерацию:
/// для v2 используем приблизительную ширину символа через metrics шрифта из fontdb.

use fontdb::{Database, Family, Query, Weight, Style as FontdbStyle};
use super::styles::{FontWeight, FontStyle};
use super::layout::MM_TO_PT;

// ─── Font Database ────────────────────────────────────────────────────────────

/// Глобальная база системных шрифтов.
pub struct FontManager {
    pub db: Database,
}

impl FontManager {
    /// Создаёт менеджер и загружает все системные шрифты.
    pub fn load() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db }
    }

    /// Находит ID шрифта по семейству, жирности и стилю.
    /// Если точного совпадения нет — возвращает ближайший fallback.
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
            families: &[
                Family::Name(family),
                Family::SansSerif,  // fallback
            ],
            weight: weight_val,
            style: style_val,
            stretch: fontdb::Stretch::Normal,
        };

        self.db.query(&query)
    }

    /// Возвращает приблизительную ширину одного символа (em) в pt.
    /// Используется для оценки ширины строки до rustybuzz shaping.
    pub fn char_width_pt(font_size_pt: f32) -> f32 {
        // Консервативная оценка: 0.6 × font_size для моноширинного,
        // 0.55 × font_size для пропорционального.
        font_size_pt * 0.55
    }
}

// ─── Текстовые строки ─────────────────────────────────────────────────────────

/// Одна визуальная строка после line-break
#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    /// Ширина строки в pt
    pub width: f32,
    /// Высота строки (line_height × font_size) в pt
    pub line_height_pt: f32,
    /// Размер шрифта в pt (нужен для вычисления baseline)
    pub font_size: f32,
}

/// Разбивает текст на строки по ширине контейнера.
///
/// Алгоритм:
/// 1. Находим допустимые позиции переноса через `unicode-linebreak`
/// 2. Жадно набираем слова в строку, пока ширина не превышает max_width
/// 3. При превышении переносим на следующую строку
pub fn break_text(
    text: &str,
    max_width_pt: f32,
    font_size_pt: f32,
    line_height: f32,
) -> Vec<TextLine> {
    if text.is_empty() {
        return vec![];
    }

    let char_w = FontManager::char_width_pt(font_size_pt);
    let line_h = font_size_pt * line_height;

    // Получаем допустимые позиции переноса (Unicode UAX #14)
    let break_opportunities = unicode_linebreak::linebreaks(text).collect::<Vec<_>>();

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0f32;

    let mut last_pos = 0usize;

    for (pos, opportunity) in &break_opportunities {
        let segment = &text[last_pos..*pos];
        let segment_width = segment.chars().count() as f32 * char_w;

        if current_width + segment_width > max_width_pt && !current_line.is_empty() {
            let w = current_line.chars().count() as f32 * char_w;
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

        // Принудительный перенос (newline / mandatory break)
        if *opportunity == unicode_linebreak::BreakOpportunity::Mandatory {
            let w = current_line.chars().count() as f32 * char_w;
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

    // Последний кусок
    if !current_line.trim().is_empty() {
        let w = current_line.chars().count() as f32 * char_w;
        lines.push(TextLine {
            text: current_line.trim_end().to_string(),
            width: w.min(max_width_pt),
            line_height_pt: line_h,
            font_size: font_size_pt,
        });
    }

    lines
}

/// Оценивает высоту текстового блока.
///
/// Текст рендерится по baseline: первая строка — cursor_y + font_size,
/// каждая следующая — через line_height_pt. Чтобы cursor_y после блока
/// оказался под последней строкой с учётом descender, добавляем font_size
/// как высоту «кэпа» первой строки.
pub fn text_block_height(lines: &[TextLine]) -> f32 {
    if lines.is_empty() {
        return 0.0;
    }
    let first = &lines[0];
    // font_size (baseline первой строки) + (N-1) * line_height + небольшой descender зазор
    first.font_size + (lines.len().saturating_sub(1)) as f32 * first.line_height_pt
}

/// Конвертирует mm → pt (удобная утилита)
#[allow(dead_code)]
pub fn mm_to_pt(mm: f32) -> f32 {
    mm * MM_TO_PT
}
