//! Bundled fonts (Liberation 2.1.5, SIL OFL, see `assets/fonts/LICENSE-Liberation`).
//!
//! Layout measures with these faces and the PDF backend embeds them, so output does not
//! depend on the fonts installed on the machine. Liberation is metric-compatible with
//! Arial/Helvetica (Sans) and Courier New (Mono) and covers Latin, Cyrillic and Greek.

pub const SLOT_COUNT: usize = 8;

const FACES: [&[u8]; SLOT_COUNT] = [
    include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/LiberationMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/LiberationMono-Bold.ttf"),
    include_bytes!("../../assets/fonts/LiberationMono-Italic.ttf"),
    include_bytes!("../../assets/fonts/LiberationMono-BoldItalic.ttf"),
];

const NAMES: [&str; SLOT_COUNT] = [
    "LiberationSans",
    "LiberationSans-Bold",
    "LiberationSans-Italic",
    "LiberationSans-BoldItalic",
    "LiberationMono",
    "LiberationMono-Bold",
    "LiberationMono-Italic",
    "LiberationMono-BoldItalic",
];

/// Face index: Sans then Mono, each Regular, Bold, Italic, BoldItalic.
pub fn slot(mono: bool, bold: bool, italic: bool) -> usize {
    usize::from(mono) * 4 + usize::from(italic) * 2 + usize::from(bold)
}

pub fn data(slot: usize) -> &'static [u8] {
    FACES[slot]
}

pub fn name(slot: usize) -> &'static str {
    NAMES[slot]
}

pub fn is_monospace_family(family: &str) -> bool {
    let l = family.to_ascii_lowercase();
    l.contains("courier") || l.contains("mono")
}
