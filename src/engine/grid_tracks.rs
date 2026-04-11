//! Парсинг `columns` / `grid-template-columns` для GRID: подмножество CSS (fr, px, mm, pt, auto).

use std::fmt::Write;

use super::layout::MM_TO_PT;
use crate::parser::ast::Value;
use taffy::prelude::*;
use taffy::style::GridTemplateComponent;

/// Один track колонки grid (после резолва в абсолютные единицы для движка, кроме fr).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridColumnTrack {
    /// Доля `fr` (как в CSS).
    Fr(f32),
    /// Длина в pt (для taffy `length()`).
    LengthPt(f32),
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridTrackParseError {
    pub message: String,
}

impl GridTrackParseError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// CSS px при 96 DPI: 1px = 72/96 pt.
const PX_TO_PT: f32 = 72.0 / 96.0;

/// Разбор строки вида `1fr 2fr`, `10mm 1fr`, `auto 1fr`.
pub fn parse_grid_columns_str(input: &str) -> Result<Vec<GridColumnTrack>, GridTrackParseError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(GridTrackParseError::new("empty grid columns"));
    }
    let mut out = Vec::new();
    for token in s.split_whitespace() {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        out.push(parse_track_token(t)?);
    }
    if out.is_empty() {
        return Err(GridTrackParseError::new("no tracks"));
    }
    Ok(out)
}

/// Разбор значения attrs для GRID columns:
/// - строка треков: `1fr 10mm auto`
/// - число: количество равных `1fr` колонок
/// - unit-токен: `2fr`, `10mm`, `120px`, `12pt`
pub fn parse_grid_columns_value(value: &Value) -> Option<Vec<GridColumnTrack>> {
    match value {
        Value::Str(s) => {
            let t = s.trim();
            if let Ok(parsed) = parse_grid_columns_str(t) {
                Some(parsed)
            } else if let Ok(n) = t.parse::<usize>() {
                if n >= 1 {
                    Some(vec![GridColumnTrack::Fr(1.0); n])
                } else {
                    None
                }
            } else {
                None
            }
        }
        Value::Number(n) => Some(vec![GridColumnTrack::Fr(1.0); (*n as usize).max(1)]),
        Value::Unit(n, unit) => {
            let token = format!("{}{}", n, unit);
            parse_grid_columns_str(&token).ok()
        }
        _ => None,
    }
}

fn parse_track_token(token: &str) -> Result<GridColumnTrack, GridTrackParseError> {
    let t = token.trim().to_ascii_lowercase();
    if t == "auto" {
        return Ok(GridColumnTrack::Auto);
    }
    if let Some(prefix) = t.strip_suffix("fr") {
        let n = prefix
            .trim()
            .parse::<f32>()
            .map_err(|_| GridTrackParseError::new(format!("invalid fr track: {token}")))?;
        if n <= 0.0 {
            return Err(GridTrackParseError::new(format!("fr must be positive: {token}")));
        }
        return Ok(GridColumnTrack::Fr(n));
    }
    if let Some(prefix) = t.strip_suffix("mm") {
        let n = prefix
            .trim()
            .parse::<f32>()
            .map_err(|_| GridTrackParseError::new(format!("invalid mm track: {token}")))?;
        if n < 0.0 {
            return Err(GridTrackParseError::new(format!("negative length: {token}")));
        }
        return Ok(GridColumnTrack::LengthPt(n * MM_TO_PT));
    }
    if let Some(prefix) = t.strip_suffix("pt") {
        let n = prefix
            .trim()
            .parse::<f32>()
            .map_err(|_| GridTrackParseError::new(format!("invalid pt track: {token}")))?;
        if n < 0.0 {
            return Err(GridTrackParseError::new(format!("negative length: {token}")));
        }
        return Ok(GridColumnTrack::LengthPt(n));
    }
    if let Some(prefix) = t.strip_suffix("px") {
        let n = prefix
            .trim()
            .parse::<f32>()
            .map_err(|_| GridTrackParseError::new(format!("invalid px track: {token}")))?;
        if n < 0.0 {
            return Err(GridTrackParseError::new(format!("negative length: {token}")));
        }
        return Ok(GridColumnTrack::LengthPt(n * PX_TO_PT));
    }
    Err(GridTrackParseError::new(format!(
        "unsupported grid track (use fr, px, mm, pt, auto): {token}"
    )))
}

/// Количество колонок для пагинации. Пустой слайс = одна колонка `1fr` (как раньше `None`).
pub fn grid_column_count(tracks: &[GridColumnTrack]) -> usize {
    if tracks.is_empty() {
        1
    } else {
        tracks.len()
    }
}

/// CSS для `grid-template-columns` (согласовано с парсером).
pub fn tracks_to_css(tracks: &[GridColumnTrack]) -> String {
    let mut s = String::new();
    for (i, tr) in tracks.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        match *tr {
            GridColumnTrack::Fr(v) => {
                let _ = write!(&mut s, "{}", fmt_fr(v));
                s.push_str("fr");
            }
            GridColumnTrack::LengthPt(pt) => {
                let _ = write!(&mut s, "{}pt", fmt_fr(pt));
            }
            GridColumnTrack::Auto => s.push_str("auto"),
        }
    }
    s
}

fn fmt_fr(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Строит `grid_template_columns` для taffy `Style` (дефолтный `CheapCloneStr` в taffy — `String`).
pub fn tracks_to_taffy_components(
    tracks: &[GridColumnTrack],
) -> Vec<GridTemplateComponent<String>> {
    let slice = if tracks.is_empty() {
        &[GridColumnTrack::Fr(1.0)][..]
    } else {
        tracks
    };
    slice
        .iter()
        .map(|tr| match *tr {
            GridColumnTrack::Fr(v) => fr(v),
            GridColumnTrack::LengthPt(pt) => length(pt),
            GridColumnTrack::Auto => auto(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Value;

    #[test]
    fn parse_1fr_2fr() {
        let t = parse_grid_columns_str("1fr 2fr").unwrap();
        assert_eq!(t, vec![GridColumnTrack::Fr(1.0), GridColumnTrack::Fr(2.0)]);
    }

    #[test]
    fn parse_mixed_units() {
        let t = parse_grid_columns_str("10mm auto 1fr").unwrap();
        assert_eq!(t[0], GridColumnTrack::LengthPt(10.0 * MM_TO_PT));
        assert_eq!(t[1], GridColumnTrack::Auto);
        assert_eq!(t[2], GridColumnTrack::Fr(1.0));
    }

    #[test]
    fn rejects_minmax() {
        assert!(parse_grid_columns_str("minmax(0, 1fr)").is_err());
    }

    #[test]
    fn parse_value_unit_fr() {
        let t = parse_grid_columns_value(&Value::Unit(2.0, "fr".to_string())).unwrap();
        assert_eq!(t, vec![GridColumnTrack::Fr(2.0)]);
    }

    #[test]
    fn parse_value_unit_mm() {
        let t = parse_grid_columns_value(&Value::Unit(10.0, "mm".to_string())).unwrap();
        assert_eq!(t, vec![GridColumnTrack::LengthPt(10.0 * MM_TO_PT)]);
    }
}
