//! Font embedding: Type0 / Identity-H with a CIDFontType2 descendant,
//! full TrueType program (`FontFile2`), `W` widths for used glyphs and a
//! ToUnicode CMap (§6.1).

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};

use crate::fonts::LoadedFont;
use crate::InkoxideError;

const IDENTITY: SystemInfo<'static> = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// Writes all objects of one font; returns the Type0 font reference.
pub(crate) fn write_font(
    pdf: &mut Pdf,
    next_ref: &mut dyn FnMut() -> Ref,
    font: &LoadedFont<'_>,
    used: &BTreeSet<u16>,
    unicode: Option<&BTreeMap<u16, String>>,
) -> Result<Ref, InkoxideError> {
    let type0 = next_ref();
    let cid = next_ref();
    let descriptor = next_ref();
    let file = next_ref();
    let cmap_ref = next_ref();

    let m = &font.data.metrics;
    let to_1000 = 1000.0 / m.units_per_em;
    let name = m.post_script_name.as_bytes();

    pdf.type0_font(type0)
        .base_font(Name(name))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid)
        .to_unicode(cmap_ref);

    let mut c = pdf.cid_font(cid);
    c.subtype(CidFontType::Type2)
        .base_font(Name(name))
        .system_info(IDENTITY)
        .font_descriptor(descriptor)
        .default_width(0.0)
        .cid_to_gid_map_predefined(Name(b"Identity"));
    {
        // Runs of consecutive glyph ids share one `consecutive` entry.
        let mut w = c.widths();
        let ids: Vec<u16> = used.iter().copied().collect();
        let mut i = 0;
        while i < ids.len() {
            let start = ids[i];
            let mut j = i + 1;
            while j < ids.len() && ids[j] == ids[j - 1] + 1 {
                j += 1;
            }
            let widths: Vec<f32> = ids[i..j]
                .iter()
                .map(|&g| (font.advance(g) * to_1000 * 1000.0).round() / 1000.0)
                .collect();
            w.consecutive(start, widths);
            i = j;
        }
    }
    c.finish();

    let mut flags = FontFlags::SYMBOLIC;
    if m.is_italic {
        flags |= FontFlags::ITALIC;
    }
    if m.is_fixed_pitch {
        flags |= FontFlags::FIXED_PITCH;
    }
    if m.is_serif {
        flags |= FontFlags::SERIF;
    }
    let s = |v: f32| (v * to_1000).round();
    pdf.font_descriptor(descriptor)
        .name(Name(name))
        .flags(flags)
        .bbox(Rect::new(
            s(m.bbox[0]),
            s(m.bbox[1]),
            s(m.bbox[2]),
            s(m.bbox[3]),
        ))
        .italic_angle(m.italic_angle)
        .ascent(s(m.ascender))
        .descent(s(m.descender))
        .cap_height(s(m.cap_height))
        // No reliable source in TrueType; the usual weight-based estimate.
        .stem_v((10.0 + 0.244 * (f32::from(m.weight) - 50.0)).round())
        .font_file2(file);

    let data = font.data.compressed()?;
    let mut stream = pdf.stream(file, &data);
    stream.filter(Filter::FlateDecode);
    stream.pair(Name(b"Length1"), font.data.bytes.len() as i32);
    stream.finish();

    let mut cmap = UnicodeCmap::new(Name(b"Custom"), IDENTITY);
    if let Some(map) = unicode {
        for (&gid, text) in map {
            if used.contains(&gid) && !text.is_empty() {
                cmap.pair_with_multiple(gid, text.chars());
            }
        }
    }
    pdf.cmap(cmap_ref, &cmap.finish())
        .name(Name(b"Custom"))
        .system_info(IDENTITY);

    Ok(type0)
}
