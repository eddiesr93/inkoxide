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
//! whole font is embedded, with CID = glyph id. Under PDF/A-1 a subset's
//! descriptor also lists its CIDs in a `CIDSet` stream.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};

use super::{deflate, Out, Profile, RefAlloc};
use crate::flate;
use crate::fonts::LoadedFont;
use crate::lru::SharedLru;
use crate::InkoxideError;

/// The character collection of every font here (`Adobe-Identity-0`),
/// inside the `CMap` stream.
const IDENTITY: SystemInfo<'static> = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// [`IDENTITY`]'s registry and ordering strings, as `out` writes them in a
/// dictionary.
fn identity(out: &mut Out) -> [Vec<u8>; 2] {
    [
        out.bytes(IDENTITY.registry.0),
        out.bytes(IDENTITY.ordering.0),
    ]
}

/// Writes a `CIDSystemInfo` dictionary of [`identity`]'s strings into `obj`,
/// as pdf-writer's `system_info` does.
fn system_info(obj: pdf_writer::Obj<'_>, [registry, ordering]: &[Vec<u8>; 2]) {
    obj.dict()
        .pair(Name(b"Registry"), Str(registry))
        .pair(Name(b"Ordering"), Str(ordering))
        .pair(Name(b"Supplement"), IDENTITY.supplement);
}

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

/// One font as a document embeds it.
pub(crate) struct Embedded<'f, 'a> {
    pub font: &'f LoadedFont<'a>,
    pub program: &'f Program,
    /// The glyphs the document uses.
    pub used: &'f BTreeSet<u16>,
    /// The text of each glyph, for `ToUnicode`.
    pub unicode: Option<&'f BTreeMap<u16, String>>,
}

/// Writes all objects of one font; returns the Type0 font reference.
/// Under PDF/A-1 a subset's descriptor lists its CIDs, and the widths
/// keep to its array limit.
pub(crate) fn write_font(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    out: &mut Out,
    embedded: &Embedded<'_, '_>,
    profile: Profile,
) -> Result<Ref, InkoxideError> {
    let Embedded {
        font,
        program,
        used,
        unicode,
    } = *embedded;
    let type0 = alloc.next();
    let cid = alloc.next();
    let descriptor = alloc.next();
    let file = alloc.next();
    let cmap_ref = alloc.next();
    let map_ref = program.cid_to_gid.as_ref().map(|_| alloc.next());
    // Only a subset needs one (ISO 19005-1, 6.3.5); PDF/A-2 and later
    // need none, and check that one present is complete.
    let cid_set_ref = (profile.cid_set() && !program.tag.is_empty()).then(|| alloc.next());

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
    .base_font(Name(name));
    system_info(c.insert(Name(b"CIDSystemInfo")), &identity(out));
    c.font_descriptor(descriptor).default_width(0.0);
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
        let mut w = c.widths();
        for run in width_runs(used, profile.max_array()) {
            let widths: Vec<f32> = run
                .clone()
                .map(|g| (font.advance(g) * to_1000 * 1000.0).round() / 1000.0)
                .collect();
            w.consecutive(*run.start(), widths);
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
    if let Some(r) = cid_set_ref {
        d.cid_set(r);
    }
    d.finish();

    out.stream(pdf, file, &program.data, |stream| {
        stream.filter(Filter::FlateDecode);
        if cff {
            stream.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
        } else {
            stream.pair(Name(b"Length1"), program.length as i32);
        }
    });
    if let (Some(r), Some(map)) = (map_ref, &program.cid_to_gid) {
        out.stream(pdf, r, map, |s| {
            s.filter(Filter::FlateDecode);
        });
    }
    if let Some(r) = cid_set_ref {
        let set = deflate(&cid_set_bits(used), flate::LEVEL)?;
        out.stream(pdf, r, &set, |s| {
            s.filter(Filter::FlateDecode);
        });
    }

    let mut cmap = UnicodeCmap::new(Name(b"Custom"), IDENTITY);
    if let Some(map) = unicode {
        for (&gid, text) in map {
            if used.contains(&gid) && !text.is_empty() {
                cmap.pair_with_multiple(gid, text.chars());
            }
        }
    }
    let info = identity(out);
    out.cmap(pdf, cmap_ref, &cmap.finish(), |c| {
        c.name(Name(b"Custom"));
        system_info(c.insert(Name(b"CIDSystemInfo")), &info);
    });

    Ok(type0)
}

/// The glyph id ranges of a `W` array for the glyphs `used`: each run of
/// consecutive ids shares one `consecutive` entry. With `max_array`
/// (PDF/A-1), no array has more entries: a run is split into pieces of at
/// most that many glyphs, and when the runs would take more entries than
/// that, the ranges cover every id from the first glyph used to the last
/// instead, unused ones too.
fn width_runs(
    used: &BTreeSet<u16>,
    max_array: Option<usize>,
) -> Vec<std::ops::RangeInclusive<u16>> {
    let mut runs: Vec<std::ops::RangeInclusive<u16>> = Vec::new();
    for &g in used {
        match runs.last_mut() {
            Some(r) if r.end().checked_add(1) == Some(g) => *r = *r.start()..=g,
            _ => runs.push(g..=g),
        }
    }
    let Some(max) = max_array else {
        return runs;
    };
    // Each `consecutive` entry is two array items: its start and widths.
    if runs.len() > max / 2 {
        if let (Some(&first), Some(&last)) = (used.first(), used.last()) {
            runs = vec![first..=last];
        }
    }
    let piece = u16::try_from(max).unwrap_or(u16::MAX);
    runs.into_iter()
        .flat_map(|r| {
            let (start, end) = (*r.start(), *r.end());
            (start..=end)
                .step_by(usize::from(piece))
                .map(move |s| s..=s.saturating_add(piece - 1).min(end))
        })
        .collect()
}

/// A `CIDSet`: bit `c` (high bit first) is set for each CID `c` the
/// subset holds. A subset holds the glyphs `used`, under their original
/// ids, and `.notdef`.
fn cid_set_bits(used: &BTreeSet<u16>) -> Vec<u8> {
    let last = used.last().copied().unwrap_or(0);
    let mut bits = vec![0u8; usize::from(last) / 8 + 1];
    for c in used.iter().copied().chain([0]) {
        bits[usize::from(c) / 8] |= 0x80 >> (c % 8);
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_runs_keep_to_the_array_limit() {
        let used = BTreeSet::from([1, 2, 3, 7, 9, 10]);
        assert_eq!(width_runs(&used, None), [1..=3, 7..=7, 9..=10]);
        assert_eq!(width_runs(&used, Some(8191)), [1..=3, 7..=7, 9..=10]);
        // Pieces of at most `max` glyphs; past `max / 2` runs, one range
        // from the first glyph to the last.
        assert_eq!(width_runs(&used, Some(6)), [1..=3, 7..=7, 9..=10]);
        assert_eq!(
            width_runs(&used, Some(2)),
            [1..=2, 3..=4, 5..=6, 7..=8, 9..=10]
        );
        // Every other id: too many runs, so one range covers them all.
        let sparse: BTreeSet<u16> = (0..20_000).step_by(2).collect();
        let runs = width_runs(&sparse, Some(8191));
        assert_eq!(runs, [0..=8190, 8191..=16381, 16382..=19998]);
        assert_eq!(width_runs(&sparse, None).len(), 10_000);
        let all: BTreeSet<u16> = (0..=u16::MAX).collect();
        let runs = width_runs(&all, Some(8191));
        assert_eq!(runs.len(), 9);
        assert_eq!(*runs[8].end(), u16::MAX);
    }

    #[test]
    fn cid_sets_hold_the_used_cids_and_notdef() {
        assert_eq!(cid_set_bits(&BTreeSet::new()), [0x80]);
        assert_eq!(cid_set_bits(&BTreeSet::from([1, 7])), [0xC1]);
        assert_eq!(
            cid_set_bits(&BTreeSet::from([3, 8, 17])),
            [0x90, 0x80, 0x40]
        );
        assert_eq!(cid_set_bits(&BTreeSet::from([u16::MAX])).len(), 8192);
    }
}
