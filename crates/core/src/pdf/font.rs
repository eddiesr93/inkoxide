//! Font embedding: Type0 / Identity-H with a `CIDFontType2` descendant and a
//! TrueType program (`FontFile2`), or a `CIDFontType0` descendant and a
//! CID-keyed CFF program (`FontFile3` / `CIDFontType0C`); `W`
//! widths for used glyphs and a `ToUnicode` `CMap`.
//!
//! With `subset_fonts` (the default), programs are subset to the glyphs
//! used (`fonts` does the subsetting; this module compresses and writes).
//! CIDs stay the original glyph ids, so content streams are encoded before
//! the glyph set is known: a TrueType subset gets a `CIDToGIDMap` stream
//! that maps them to the renumbered glyphs, and a CFF subset's charset maps
//! them. Without subsetting, or when the subsetter rejects a font, the
//! whole font is embedded, with CID = glyph id.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};

use super::{deflate, RefAlloc};
use crate::flate;
use crate::fonts::LoadedFont;
use crate::lru::SharedLru;
use crate::InkoxideError;

const IDENTITY: SystemInfo<'static> = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// The embedded font program of one font in one document.
#[derive(Clone)]
pub(crate) struct Program {
    /// Flate-compressed `FontFile2` / `FontFile3` data.
    data: Arc<[u8]>,
    /// Uncompressed length (`Length1`).
    length: usize,
    /// Flate-compressed `CIDToGIDMap` for a subset; `None`: identity.
    cid_to_gid: Option<Arc<[u8]>>,
    /// `ABCDEF+` subset tag; empty for a full font.
    tag: Arc<str>,
}

/// Compressed subsets kept across renders: documents from one template
/// use nearly the same glyphs, and compressing the subset is most of the
/// cost of embedding it.
const SUBSET_BUDGET: usize = 32 << 20;

/// Font hash and the glyph set.
type SubsetKey = (u64, Vec<u16>);

static SUBSETS: SharedLru<SubsetKey, Program> = SharedLru::new(SUBSET_BUDGET);

/// Prepares the font program: a subset of `used` when `subset` is set and
/// the subsetter accepts the font, otherwise the whole (cached) font.
pub(crate) fn program(
    font: &LoadedFont<'_>,
    used: &BTreeSet<u16>,
    subset: bool,
) -> Result<Program, InkoxideError> {
    if subset {
        let key = (font.data.hash, used.iter().copied().collect::<Vec<u16>>());
        if let Some(p) = SUBSETS.get(&key) {
            return Ok(p);
        }
        if let Some(p) = make_subset(font, &key.1)? {
            let size = p.data.len() + p.cid_to_gid.as_ref().map_or(0, |m| m.len());
            SUBSETS.insert(key, p.clone(), size);
            return Ok(p);
        }
        tracing::warn!(
            font = font.data.metrics.post_script_name,
            "subsetting failed; embedding the whole font"
        );
    }
    Ok(Program {
        data: font.data.compressed_program(|p| deflate(p, flate::LEVEL))?,
        length: font.data.bytes.len(),
        cid_to_gid: None,
        tag: "".into(),
    })
}

/// `None` when the subsetter rejects the font.
fn make_subset(font: &LoadedFont<'_>, glyphs: &[u16]) -> Result<Option<Program>, InkoxideError> {
    let Some(sub) = font.data.subset(glyphs) else {
        return Ok(None);
    };
    let cid_to_gid = match &sub.cid_to_gid {
        Some(map) => Some(deflate(map, flate::LEVEL)?.into()),
        None => None,
    };
    Ok(Some(Program {
        data: deflate(&sub.program, flate::LEVEL)?.into(),
        length: sub.program.len(),
        cid_to_gid,
        tag: subset_tag(font, glyphs).into(),
    }))
}

/// Six uppercase letters derived from the font and the glyph set, so equal
/// inputs give equal output and different subsets get different names.
fn subset_tag(font: &LoadedFont<'_>, glyphs: &[u16]) -> String {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    h.update(font.data.metrics.post_script_name.as_bytes());
    for g in glyphs {
        h.update(&g.to_le_bytes());
    }
    let mut v = h.digest();
    let mut tag = String::with_capacity(7);
    for _ in 0..6 {
        tag.push(char::from(b'A' + (v % 26) as u8));
        v /= 26;
    }
    tag.push('+');
    tag
}

/// Writes all objects of one font; returns the Type0 font reference.
pub(crate) fn write_font(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    font: &LoadedFont<'_>,
    program: &Program,
    used: &BTreeSet<u16>,
    unicode: Option<&BTreeMap<u16, String>>,
) -> Ref {
    let type0 = alloc.next();
    let cid = alloc.next();
    let descriptor = alloc.next();
    let file = alloc.next();
    let cmap_ref = alloc.next();
    let map_ref = program.cid_to_gid.as_ref().map(|_| alloc.next());

    let m = &font.data.metrics;
    let to_1000 = 1000.0 / m.units_per_em;
    let full_name = format!("{}{}", program.tag, m.post_script_name);
    let name = full_name.as_bytes();

    pdf.type0_font(type0)
        .base_font(Name(name))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid)
        .to_unicode(cmap_ref);

    let cff = m.cff;
    let mut c = pdf.cid_font(cid);
    c.subtype(if cff {
        CidFontType::Type0
    } else {
        CidFontType::Type2
    })
    .base_font(Name(name))
    .system_info(IDENTITY)
    .font_descriptor(descriptor)
    .default_width(0.0);
    match map_ref {
        Some(r) => {
            c.pair(Name(b"CIDToGIDMap"), r);
        }
        // Only TrueType CIDFonts take a CIDToGIDMap.
        None if !cff => {
            c.cid_to_gid_map_predefined(Name(b"Identity"));
        }
        None => {}
    }
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
    let mut d = pdf.font_descriptor(descriptor);
    d.name(Name(name))
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
        .stem_v((10.0 + 0.244 * (f32::from(m.weight) - 50.0)).round());
    if cff {
        d.font_file3(file);
    } else {
        d.font_file2(file);
    }
    d.finish();

    let mut stream = pdf.stream(file, &program.data);
    stream.filter(Filter::FlateDecode);
    if cff {
        stream.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
    } else {
        stream.pair(Name(b"Length1"), program.length as i32);
    }
    stream.finish();
    if let (Some(r), Some(map)) = (map_ref, &program.cid_to_gid) {
        pdf.stream(r, map).filter(Filter::FlateDecode);
    }

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

    type0
}
