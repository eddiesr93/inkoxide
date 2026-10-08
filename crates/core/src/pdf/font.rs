//! Font embedding: Type0 / Identity-H with a `CIDFontType2` descendant and a
//! TrueType program (`FontFile2`), or a `CIDFontType0` descendant and a
//! CID-keyed CFF program (`FontFile3` / `CIDFontType0C`, ADR-037); `W`
//! widths for used glyphs and a `ToUnicode` `CMap` (§6.1).
//!
//! Programs are subset to the glyphs used (ADR-032). CIDs stay the
//! original glyph ids, so content streams are encoded before the glyph set
//! is known; a `CIDToGIDMap` stream maps them to the renumbered subset.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};

use subsetter::GlyphRemapper;

use super::deflate;
use crate::fonts::LoadedFont;
use crate::lru::Lru;
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

static SUBSETS: Mutex<Option<Lru<SubsetKey, Program>>> = Mutex::new(None);

/// Prepares the font program: a subset of `used` when `subset` is set and
/// the subsetter accepts the font, otherwise the whole (cached) font.
pub(crate) fn program(
    font: &LoadedFont<'_>,
    used: &BTreeSet<u16>,
    subset: bool,
) -> Result<Program, InkoxideError> {
    if subset {
        let key = (font.data.hash, used.iter().copied().collect::<Vec<u16>>());
        if let Some(p) = SUBSETS
            .lock()
            .ok()
            .and_then(|mut g| g.as_mut().and_then(|l| l.get(&key)))
        {
            return Ok(p);
        }
        if let Some(p) = make_subset(font, &key.1)? {
            if let Ok(mut g) = SUBSETS.lock() {
                let size = p.data.len() + p.cid_to_gid.as_ref().map_or(0, |m| m.len());
                g.get_or_insert_with(|| Lru::new(SUBSET_BUDGET))
                    .insert(key, p.clone(), size);
            }
            return Ok(p);
        }
        tracing::warn!(
            font = font.data.metrics.post_script_name,
            "subsetting failed; embedding the whole font"
        );
    }
    Ok(Program {
        data: font.data.compressed()?,
        length: font.data.bytes.len(),
        cid_to_gid: None,
        tag: "".into(),
    })
}

/// `None` when the subsetter rejects the font.
fn make_subset(font: &LoadedFont<'_>, glyphs: &[u16]) -> Result<Option<Program>, InkoxideError> {
    if font.data.metrics.cff {
        // CIDs stay the original glyph ids through the CFF charset.
        let Some(cff) = super::cff::subset_program(&font.data.bytes, glyphs) else {
            return Ok(None);
        };
        return Ok(Some(Program {
            data: deflate(&cff, 6)?.into(),
            length: cff.len(),
            cid_to_gid: None,
            tag: subset_tag(font, glyphs).into(),
        }));
    }
    let remapper = GlyphRemapper::new_from_glyphs_sorted(glyphs);
    let Ok(sub) = subsetter::subset(&font.data.bytes, 0, &remapper) else {
        return Ok(None);
    };
    let max = glyphs.last().copied().unwrap_or(0);
    let mut map = vec![0u8; 2 * (usize::from(max) + 1)];
    for &g in glyphs {
        let new = remapper.get(g).unwrap_or(0);
        map[2 * usize::from(g)..][..2].copy_from_slice(&new.to_be_bytes());
    }
    Ok(Some(Program {
        data: deflate(&sub, 6)?.into(),
        length: sub.len(),
        cid_to_gid: Some(deflate(&map, 6)?.into()),
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
    next_ref: &mut dyn FnMut() -> Ref,
    font: &LoadedFont<'_>,
    program: &Program,
    used: &BTreeSet<u16>,
    unicode: Option<&BTreeMap<u16, String>>,
) -> Ref {
    let type0 = next_ref();
    let cid = next_ref();
    let descriptor = next_ref();
    let file = next_ref();
    let cmap_ref = next_ref();
    let map_ref = program.cid_to_gid.as_ref().map(|_| next_ref());

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
