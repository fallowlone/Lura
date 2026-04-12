//! Parse GRID `columns` / `grid-columns` track lists for the layout engine.
//!
//! Syntax is grid-like (`fr`, `px`, `mm`, `pt`, `auto`) and feeds **taffy** via
//! [`tracks_to_taffy_components`], not a browser CSSOM.

use super::layout::MM_TO_PT;
use crate::parser::ast::Value;
use taffy::prelude::*;
use taffy::style::GridTemplateComponent;

/// One grid column track (after resolve to absolute units for the engine, except `fr`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridColumnTrack {
    /// `fr` flex fraction (resolved by taffy alongside other tracks).
    Fr(f32),
    /// Length in pt (for taffy `length()`).
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

/// Author `px` in track tokens: treat as 96 px per inch → pt (72 pt per inch).
const PX_TO_PT: f32 = 72.0 / 96.0;

/// Parse a string like `1fr 2fr`, `10mm 1fr`, `auto 1fr`.
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

/// Parse a GRID `columns` / `grid-columns` attr value:
/// - track string: `1fr 10mm auto`
/// - integer as column count: equal `1fr` tracks
/// - unit token: `2fr`, `10mm`, `120px`, `12pt`
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
            let num = format_number_for_grid_unit_token(*n).unwrap_or_else(|| n.to_string());
            let token = format!("{num}{unit}");
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

/// Format a number for concatenation with a unit (`2fr`, `10.5mm`) without scientific notation.
fn format_number_for_grid_unit_token(n: f64) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    if n.fract().abs() < 1e-9 {
        Some(format!("{}", n as i64))
    } else {
        let mut s = format!("{:.6}", n);
        while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
            s.pop();
        }
        Some(s)
    }
}

/// Column count for pagination. Empty slice means one `1fr` column (same as former `None`).
pub fn grid_column_count(tracks: &[GridColumnTrack]) -> usize {
    if tracks.is_empty() {
        1
    } else {
        tracks.len()
    }
}

/// Build `grid_template_columns` for taffy `Style` (taffy default `CheapCloneStr` is `String`).
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

    #[test]
    fn parse_value_number_three_equal_fr_columns() {
        let t = parse_grid_columns_value(&Value::Number(3.0)).unwrap();
        assert_eq!(t, vec![GridColumnTrack::Fr(1.0); 3]);
    }

    #[test]
    fn parse_value_unit_unknown_returns_none() {
        assert!(parse_grid_columns_value(&Value::Unit(10.0, "em".to_string())).is_none());
    }
}
