//! Font embedding: Type0 / Identity-H with a `CIDFontType2` descendant and a
//! TrueType program (`FontFile2`), or a `CIDFontType0` descendant and a
//! CID-keyed CFF program (`FontFile3` / `CIDFontType0C`); `W`
//! widths for used glyphs (for every glyph when encrypted) and a
//! `ToUnicode` `CMap`. A font SVG text draws upright in columns has a
//! second Type0 font over the same descendant, `Identity-V`, with the
//! font's vertical metrics in `DW2` (none per glyph: content streams
//! adjust the positions, so an encrypted file tells no more).
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
use pdf_writer::{Filter, Finish, Name, Rect, Ref, Str};

use super::{deflate, Out, Profile, Writer};
use crate::flate;
use crate::fonts::LoadedFont;
use crate::lru::SharedLru;
use crate::InkoxideError;

/// The character collection of every font here (`Adobe-Identity-0`),
/// inside the `CMap` stream.
pub(super) const IDENTITY: SystemInfo<'static> = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// [`IDENTITY`]'s registry and ordering strings, as `out` writes them in a
/// dictionary.
pub(super) fn identity(out: &mut Out) -> [Vec<u8>; 2] {
    [
        out.bytes(IDENTITY.registry.0),
        out.bytes(IDENTITY.ordering.0),
    ]
}

/// Writes a `CIDSystemInfo` dictionary of [`identity`]'s strings into `obj`,
/// as pdf-writer's `system_info` does.
pub(super) fn system_info(obj: pdf_writer::Obj<'_>, [registry, ordering]: &[Vec<u8>; 2]) {
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

/// The font's SHA-256 (with an instance's coordinates) and the glyph set:
/// two different fonts or instances never share a subset, and a cached
/// subset does not keep its font's bytes.
type SubsetKey = ([u8; 32], Vec<u16>);

pub(crate) static SUBSETS: SharedLru<SubsetKey, Program> = SharedLru::new(SUBSET_BUDGET);

/// Prepares the font program: a subset of `used` when `subset` is set and
/// the subsetter accepts the font, otherwise the whole (cached) font; a
/// variable font's instance whose glyphs do not all vary is a subset.
pub(crate) fn program(
    font: &LoadedFont<'_>,
    used: &BTreeSet<u16>,
    subset: bool,
) -> Result<Program, InkoxideError> {
    if subset {
        let key = (font.digest(), used.iter().copied().collect::<Vec<u16>>());
        if let Some(p) = SUBSETS.get(&key) {
            return Ok(p);
        }
        if let Some(p) = make_subset(font, &key.1)? {
            let size = p.data.len() + p.cid_to_gid.as_ref().map_or(0, |m| m.len());
            SUBSETS.insert(key, p.clone(), size);
            return Ok(p);
        }
        tracing::warn!(
            font = font.metrics().post_script_name,
            "subsetting failed; embedding the whole font"
        );
    }
    let (data, length) = match font.compressed_program(|p| deflate(p, flate::LEVEL)) {
        Ok(whole) => whole,
        // A variable font's instance over every glyph fails where one of
        // them does not vary; the glyphs the document uses may still.
        Err(e) if !subset && font.data.outlines == crate::fonts::Outlines::Variable => {
            let glyphs: Vec<u16> = used.iter().copied().collect();
            let Some(p) = make_subset(font, &glyphs)? else {
                return Err(e);
            };
            tracing::warn!(
                font = font.metrics().post_script_name,
                "the whole instance cannot be made; embedding the glyphs used"
            );
            return Ok(p);
        }
        Err(e) => return Err(e),
    };
    Ok(Program {
        data,
        length,
        cid_to_gid: None,
        tag: "".into(),
    })
}

/// `None` when the subsetter rejects the font.
fn make_subset(font: &LoadedFont<'_>, glyphs: &[u16]) -> Result<Option<Program>, InkoxideError> {
    let Some(sub) = font.subset(glyphs) else {
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
/// inputs give equal output and different subsets get different names. A
/// variable font's instances also hash the font and its coordinates: two
/// files may give their instances the same name.
fn subset_tag(font: &LoadedFont<'_>, glyphs: &[u16]) -> String {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    h.update(font.metrics().post_script_name.as_bytes());
    if font.data.outlines == crate::fonts::Outlines::Variable {
        h.update(&font.digest());
    }
    for g in glyphs {
        h.update(&g.to_le_bytes());
    }
    letters_tag(h.digest())
}

/// A subset tag made from `v`: six uppercase letters (its base-26 digits)
/// and a `+`.
pub(crate) fn letters_tag(mut v: u64) -> String {
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
    /// SVG text draws glyphs of it upright.
    pub vertical: bool,
}

/// The Type0 fonts of one embedded font.
pub(crate) struct FontRefs {
    /// `Identity-H`.
    pub horizontal: Ref,
    /// `Identity-V`, for glyphs drawn upright in a column.
    pub vertical: Option<VerticalFont>,
}

/// A font's `Identity-V` Type0 font. A glyph's origin is half its width
/// (`W`) left of its point in the column and `vy` below it; the next
/// point is `advance` down. Thousandths of the font size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VerticalFont {
    pub font: Ref,
    pub vy: f32,
    pub advance: f32,
}

/// The width of glyph `g` of `font` in its `W` array: thousandths of the
/// font size, rounded to thousandths.
pub(crate) fn pdf_width(font: &LoadedFont<'_>, g: u16) -> f32 {
    let to_1000 = 1000.0 / font.metrics().units_per_em;
    (font.advance(g) * to_1000 * 1000.0).round() / 1000.0
}

/// Writes all objects of one font; returns its Type0 fonts. Under
/// PDF/A-1 a subset's descriptor lists its CIDs, and the widths keep to
/// its array limit.
pub(crate) fn write_font(
    w: &mut Writer,
    embedded: &Embedded<'_, '_>,
    profile: Profile,
) -> Result<FontRefs, InkoxideError> {
    let Writer {
        pdf, alloc, out, ..
    } = w;
    let Embedded {
        font,
        program,
        used,
        unicode,
        vertical,
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
    let vtype0 = vertical.then(|| alloc.next());

    let m = font.metrics();
    let to_1000 = 1000.0 / m.units_per_em;
    let tag = out.subset_tag(&program.tag, &m.post_script_name);
    let full_name = format!("{tag}{}", m.post_script_name);
    let name = full_name.as_bytes();

    pdf.type0_font(type0)
        .base_font(Name(name))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid)
        .to_unicode(cmap_ref);
    if let Some(r) = vtype0 {
        pdf.type0_font(r)
            .base_font(Name(name))
            .encoding_predefined(Name(b"Identity-V"))
            .descendant_font(cid)
            .to_unicode(cmap_ref);
    }

    let cff = font.data.outlines == crate::fonts::Outlines::Cff;
    let mut c = pdf.cid_font(cid);
    c.subtype(if cff {
        CidFontType::Type0
    } else {
        CidFontType::Type2
    })
    .base_font(Name(name));
    system_info(c.insert(Name(b"CIDSystemInfo")), &identity(out));
    let width = |g: u16| pdf_width(font, g);
    // Dictionaries are not encrypted, so an encrypted file lists the
    // widths of the whole font: those of the glyphs used would tell
    // which they are.
    let (default_width, widths) = if out.is_encrypted() {
        let all: Vec<f32> = (0..font.glyph_count()).map(width).collect();
        whole_font_widths(&all)
    } else {
        let entries = width_runs(used, profile.max_array())
            .into_iter()
            .map(|run| WidthEntry::Each(*run.start(), run.map(width).collect()))
            .collect();
        (0.0, entries)
    };
    c.font_descriptor(descriptor).default_width(default_width);
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
        for entry in widths {
            match entry {
                WidthEntry::Each(start, widths) => w.consecutive(start, widths),
                WidthEntry::Same(first, last, width) => w.same(first, last, width),
            };
        }
    }
    // The metrics of most upright glyphs, so that a column of them needs
    // few adjustments.
    let (vy, advance) = if vertical {
        let (vy, advance) = font.vertical_metrics();
        ((vy * to_1000).round(), (advance * to_1000).round())
    } else {
        (0.0, 0.0)
    };
    if vertical {
        c.insert(Name(b"DW2")).array().items([vy, -advance]);
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
    // A variable font's `head` box is its default instance's, which an
    // instance's outlines can exceed: theirs too, of every glyph, so that
    // an encrypted file's box does not tell which glyphs are used.
    let bbox = if font.instance.is_some() {
        (0..font.glyph_count())
            .filter_map(|g| font.glyph_bbox(g))
            .fold(m.bbox, |b, g| {
                [
                    b[0].min(g[0]),
                    b[1].min(g[1]),
                    b[2].max(g[2]),
                    b[3].max(g[3]),
                ]
            })
    } else {
        m.bbox
    };
    let mut d = pdf.font_descriptor(descriptor);
    d.name(Name(name))
        .flags(flags)
        .bbox(Rect::new(s(bbox[0]), s(bbox[1]), s(bbox[2]), s(bbox[3])))
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
        let mapped = program.cid_to_gid.is_some();
        let set = deflate(&cid_set_bits(used, mapped), flate::LEVEL)?;
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

    Ok(FontRefs {
        horizontal: type0,
        vertical: vtype0.map(|font| VerticalFont { font, vy, advance }),
    })
}

/// One entry of a `W` array.
#[derive(Debug, PartialEq)]
enum WidthEntry {
    /// The widths of consecutive CIDs from the first.
    Each(u16, Vec<f32>),
    /// One width for every CID from the first to the last.
    Same(u16, u16, f32),
}

/// Shortest run of equal widths written as one [`WidthEntry::Same`]: it
/// takes three array items, the run inside an `Each` one item a width.
const SAME_RUN: usize = 4;

/// The `DW` and `W` of a font whose glyph `g` is `widths[g]` wide, for
/// every glyph: `DW` is the most common width (the smallest of equally
/// common ones), which `W` leaves out; runs of at least [`SAME_RUN`]
/// equal widths take one `Same` entry, the other widths `Each` entries.
fn whole_font_widths(widths: &[f32]) -> (f32, Vec<WidthEntry>) {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for w in widths {
        *counts.entry(w.to_bits()).or_default() += 1;
    }
    let default = counts
        .iter()
        .max_by_key(|&(&bits, &n)| (n, std::cmp::Reverse(bits)))
        .map_or(0, |(&bits, _)| bits);
    let id = |g: usize| u16::try_from(g).unwrap_or(u16::MAX);
    let mut entries = Vec::new();
    let mut g = 0;
    while g < widths.len() {
        let bits = widths[g].to_bits();
        let end = g + widths[g..]
            .iter()
            .take_while(|w| w.to_bits() == bits)
            .count();
        let same = &widths[g..end];
        match entries.last_mut() {
            _ if bits == default => {}
            _ if same.len() >= SAME_RUN => {
                entries.push(WidthEntry::Same(id(g), id(end - 1), widths[g]));
            }
            Some(WidthEntry::Each(start, run)) if usize::from(*start) + run.len() == g => {
                run.extend_from_slice(same);
            }
            _ => entries.push(WidthEntry::Each(id(g), same.to_vec())),
        }
        g = end;
    }
    (f32::from_bits(default), entries)
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
/// ids, and `.notdef`. A TrueType subset's `CIDToGIDMap` (`mapped`) maps
/// every CID up to the last used one, an unused one to `.notdef`, so it
/// holds all of them (ISO 14289-1, 7.21.4.2).
fn cid_set_bits(used: &BTreeSet<u16>, mapped: bool) -> Vec<u8> {
    let last = used.last().copied().unwrap_or(0);
    let mut bits = vec![0u8; usize::from(last) / 8 + 1];
    if mapped {
        for c in 0..=last {
            bits[usize::from(c) / 8] |= 0x80 >> (c % 8);
        }
    } else {
        for c in used.iter().copied().chain([0]) {
            bits[usize::from(c) / 8] |= 0x80 >> (c % 8);
        }
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
    fn whole_font_widths_cover_every_glyph() {
        use WidthEntry::{Each, Same};
        let widths = [
            0.0, 500.0, 500.0, 250.5, 600.0, 600.0, 600.0, 600.0, 500.0, 700.0, 500.0,
        ];
        let (default, entries) = whole_font_widths(&widths);
        assert_eq!(default.to_bits(), 500.0f32.to_bits());
        assert_eq!(
            entries,
            [
                Each(0, vec![0.0]),
                Each(3, vec![250.5]),
                Same(4, 7, 600.0),
                Each(9, vec![700.0])
            ]
        );
        // Every glyph's width, read back.
        let mut read = [default; 11];
        for e in &entries {
            match e {
                Each(start, run) => {
                    for (i, w) in run.iter().enumerate() {
                        read[usize::from(*start) + i] = *w;
                    }
                }
                Same(first, last, w) => {
                    read[usize::from(*first)..=usize::from(*last)].fill(*w);
                }
            }
        }
        assert_eq!(read.map(f32::to_bits), widths.map(f32::to_bits));
        // Short runs join the `Each` before them; ties pick the smallest.
        let (default, entries) = whole_font_widths(&[1.0, 2.0, 2.0, 3.0, 3.0, 1.0]);
        assert_eq!(default.to_bits(), 1.0f32.to_bits());
        assert_eq!(entries, [Each(1, vec![2.0, 2.0, 3.0, 3.0])]);
        let (default, entries) = whole_font_widths(&[]);
        assert_eq!((default.to_bits(), entries), (0, vec![]));
    }

    #[test]
    fn cid_sets_hold_every_cid_the_subset_holds() {
        // A CFF subset: the CIDs used, and `.notdef`.
        assert_eq!(cid_set_bits(&BTreeSet::new(), false), [0x80]);
        assert_eq!(cid_set_bits(&BTreeSet::from([1, 7]), false), [0xC1]);
        assert_eq!(
            cid_set_bits(&BTreeSet::from([3, 8, 17]), false),
            [0x90, 0x80, 0x40]
        );
        assert_eq!(cid_set_bits(&BTreeSet::from([u16::MAX]), false).len(), 8192);
        // A TrueType subset: every CID its map covers, up to the last used.
        assert_eq!(cid_set_bits(&BTreeSet::new(), true), [0x80]);
        assert_eq!(
            cid_set_bits(&BTreeSet::from([3, 8, 17]), true),
            [0xFF, 0xFF, 0xC0]
        );
        assert_eq!(
            cid_set_bits(&BTreeSet::from([u16::MAX]), true),
            [0xFF; 8192]
        );
    }
}
