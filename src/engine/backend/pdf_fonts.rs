//! Embedded TrueType subsets for the PDF backend (Type0 / CIDFontType2, Identity-H).
//!
//! Text is encoded as 2-byte glyph ids of a per-document subset of the bundled faces, so any
//! character the face covers (Latin, Cyrillic, Greek, ...) survives. A ToUnicode map keeps the
//! text extractable. Characters the face lacks fall back to `.notdef`.
use std::collections::BTreeMap;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Name, Pdf, Rect, Ref, Str};
use subsetter::GlyphRemapper;

use crate::engine::fonts;

const RESOURCE_NAMES: [&[u8]; fonts::SLOT_COUNT] =
    [b"F1", b"F2", b"F3", b"F4", b"F5", b"F6", b"F7", b"F8"];

pub fn resource_name(slot: usize) -> Name<'static> {
    Name(RESOURCE_NAMES[slot])
}

struct Embedded {
    face: ttf_parser::Face<'static>,
    remap: GlyphRemapper,
    unicode: BTreeMap<u16, char>,
    widths: BTreeMap<u16, f32>,
}

impl Embedded {
    fn new(slot: usize) -> Self {
        let face = ttf_parser::Face::parse(fonts::data(slot), 0).expect("bundled font parses");
        let mut e = Self {
            face,
            remap: GlyphRemapper::new(),
            unicode: BTreeMap::new(),
            widths: BTreeMap::new(),
        };
        e.glyph(None); // keep .notdef at gid 0
        e
    }

    /// New (subset) glyph id for `c`; registers its width and Unicode value on first use.
    fn glyph(&mut self, c: Option<char>) -> u16 {
        let old = c
            .and_then(|c| self.face.glyph_index(c))
            .map_or(0, |g| g.0);
        let new = self.remap.remap(old);
        if let (Some(c), true) = (c, old != 0) {
            self.unicode.entry(new).or_insert(c);
        }
        if !self.widths.contains_key(&new) {
            let adv = self
                .face
                .glyph_hor_advance(ttf_parser::GlyphId(old))
                .unwrap_or(0);
            let per_mille = f32::from(adv) * 1000.0 / f32::from(self.face.units_per_em());
            self.widths.insert(new, per_mille);
        }
        new
    }
}

/// Faces used by one document, filled while the content streams are built.
pub struct FontSet {
    faces: Vec<Option<Embedded>>,
}

impl FontSet {
    pub fn new() -> Self {
        Self { faces: (0..fonts::SLOT_COUNT).map(|_| None).collect() }
    }

    /// Encode `text` for `Tj` with the given face: 2 big-endian bytes per glyph.
    pub fn encode(&mut self, slot: usize, text: &str) -> Vec<u8> {
        let e = self.faces[slot].get_or_insert_with(|| Embedded::new(slot));
        let mut out = Vec::with_capacity(text.len() * 2);
        for c in text.chars() {
            out.extend_from_slice(&e.glyph(Some(c)).to_be_bytes());
        }
        out
    }

    /// Write the font objects of every used face. Returns `(slot, font ref)` pairs, or `None`
    /// if a subset could not be produced.
    pub fn write(self, pdf: &mut Pdf, mut next_ref: impl FnMut() -> Ref) -> Option<Vec<(usize, Ref)>> {
        let mut out = Vec::new();
        for (slot, face) in self.faces.into_iter().enumerate() {
            let Some(e) = face else { continue };
            let subset = subsetter::subset(fonts::data(slot), 0, &e.remap).ok()?;
            let (type0, cid, descriptor, file, cmap) =
                (next_ref(), next_ref(), next_ref(), next_ref(), next_ref());
            let base = format!("LURAA{}+{}", (b'A' + slot as u8) as char, fonts::name(slot));
            let base = Name(base.as_bytes());
            let info = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };

            pdf.type0_font(type0)
                .base_font(base)
                .encoding_predefined(Name(b"Identity-H"))
                .descendant_font(cid)
                .to_unicode(cmap);

            {
                let mut c = pdf.cid_font(cid);
                c.subtype(CidFontType::Type2)
                    .base_font(base)
                    .system_info(info)
                    .font_descriptor(descriptor)
                    .default_width(0.0)
                    .cid_to_gid_map_predefined(Name(b"Identity"));
                c.widths().consecutive(
                    0,
                    (0..e.remap.num_gids()).map(|g| e.widths.get(&g).copied().unwrap_or(0.0)),
                );
            }

            let (mono, italic, bold) = (slot >= 4, slot & 2 != 0, slot & 1 != 0);
            let k = 1000.0 / f32::from(e.face.units_per_em());
            let bb = e.face.global_bounding_box();
            let ascent = f32::from(e.face.ascender()) * k;
            let mut flags = FontFlags::NON_SYMBOLIC;
            flags.set(FontFlags::FIXED_PITCH, mono);
            flags.set(FontFlags::ITALIC, italic);
            pdf.font_descriptor(descriptor)
                .name(base)
                .flags(flags)
                .bbox(Rect::new(
                    f32::from(bb.x_min) * k,
                    f32::from(bb.y_min) * k,
                    f32::from(bb.x_max) * k,
                    f32::from(bb.y_max) * k,
                ))
                .italic_angle(e.face.italic_angle().unwrap_or(0.0))
                .ascent(ascent)
                .descent(f32::from(e.face.descender()) * k)
                .cap_height(e.face.capital_height().map_or(ascent * 0.7, |h| f32::from(h) * k))
                .stem_v(if bold { 120.0 } else { 80.0 })
                .font_file2(file);

            pdf.stream(file, &subset).pair(Name(b"Length1"), subset.len() as i32);

            let mut map = UnicodeCmap::new(
                Name(b"Custom"),
                SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 },
            );
            for (&gid, &ch) in &e.unicode {
                map.pair(gid, ch);
            }
            let map = map.finish();
            pdf.cmap(cmap, map.as_slice())
                .name(Name(b"Custom"))
                .system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });

            out.push((slot, type0));
        }
        Some(out)
    }
}
