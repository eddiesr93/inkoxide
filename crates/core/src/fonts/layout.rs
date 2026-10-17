//! Rejects fonts whose GSUB or GPOS lookups would panic the shaper.
//!
//! rustybuzz 0.20.1 (pinned in Cargo.toml) unwraps coverage tables that
//! ttf-parser reads lazily, through `LazyOffsetArray16::get`:
//!
//! - context format 3 input coverages (`hb/ot_layout_gsubgpos.rs`);
//! - chained context format 3 backtrack, input and lookahead coverages
//!   (`hb/ot_layout_gsubgpos.rs`);
//! - reverse chaining substitution backtrack and lookahead coverages
//!   (`hb/ot/layout/GSUB/reverse_chain_single_subst.rs`).
//!
//! `get` is `None` for a null offset or one that does not point to a
//! coverage table, so such a font registers and then panics the first time
//! a glyph reaches that subtable. Its other unwraps in GSUB and GPOS read
//! `LazyArray16`s within their length, which ttf-parser checks when it
//! parses the subtable. Extension subtables are resolved by ttf-parser.
//! Check this list again when upgrading rustybuzz.

use ttf_parser::gpos::PositioningSubtable;
use ttf_parser::gsub::SubstitutionSubtable;
use ttf_parser::opentype_layout::{
    ChainedContextLookup, ContextLookup, Coverage, LayoutTable, LookupSubtable,
};

/// The name of the first table (`GSUB` or `GPOS`) with a coverage offset
/// rustybuzz would unwrap and that does not resolve.
pub(super) fn unresolved_coverage(face: &ttf_parser::Face<'_>) -> Option<&'static str> {
    let tables = face.tables();
    let gsub = tables.gsub.is_some_and(|t| {
        !every_subtable(t, |s: &SubstitutionSubtable<'_>| match s {
            SubstitutionSubtable::Context(c) => context(c),
            SubstitutionSubtable::ChainContext(c) => chained_context(c),
            SubstitutionSubtable::ReverseChainSingle(r) => {
                let (back, ahead) = (r.backtrack_coverages, r.lookahead_coverages);
                resolves(back.len(), |i| back.get(i)) && resolves(ahead.len(), |i| ahead.get(i))
            }
            _ => true,
        })
    });
    let gpos = tables.gpos.is_some_and(|t| {
        !every_subtable(t, |s: &PositioningSubtable<'_>| match s {
            PositioningSubtable::Context(c) => context(c),
            PositioningSubtable::ChainContext(c) => chained_context(c),
            _ => true,
        })
    });
    if gsub {
        Some("GSUB")
    } else if gpos {
        Some("GPOS")
    } else {
        None
    }
}

/// Whether `ok` holds for every subtable rustybuzz loads from `table`.
/// It loads them through these iterators, which stop at the first lookup
/// or subtable that does not parse.
fn every_subtable<'a, T: LookupSubtable<'a>>(
    table: LayoutTable<'a>,
    ok: impl Fn(&T) -> bool,
) -> bool {
    table
        .lookups
        .into_iter()
        .all(|lookup| lookup.subtables.into_iter::<T>().all(|s| ok(&s)))
}

fn context(c: &ContextLookup<'_>) -> bool {
    match c {
        ContextLookup::Format3 { coverages, .. } => resolves(coverages.len(), |i| coverages.get(i)),
        _ => true,
    }
}

fn chained_context(c: &ChainedContextLookup<'_>) -> bool {
    match c {
        ChainedContextLookup::Format3 {
            backtrack_coverages: back,
            input_coverages: input,
            lookahead_coverages: ahead,
            ..
        } => {
            resolves(back.len(), |i| back.get(i))
                && resolves(input.len(), |i| input.get(i))
                && resolves(ahead.len(), |i| ahead.get(i))
        }
        _ => true,
    }
}

/// Whether each of the `len` entries of a coverage array parses.
fn resolves<'a>(len: u16, get: impl Fn(u16) -> Option<Coverage<'a>>) -> bool {
    (0..len).all(|i| get(i).is_some())
}

/// Most lookups of the `vert` and `vrt2` features, and subtables of each,
/// [`vertical_form`] reads: real fonts have a few; a crafted one could
/// list thousands and make each question cost as much.
const VERTICAL_LOOKUPS: usize = 64;

/// Whether `glyph` has a vertical form: a single substitution of a lookup
/// of the font's `vert` or `vrt2` feature covers it.
pub(super) fn vertical_form(face: &ttf_parser::Face<'_>, glyph: ttf_parser::GlyphId) -> bool {
    let Some(gsub) = face.tables().gsub else {
        return false;
    };
    let tags = [
        ttf_parser::Tag::from_bytes(b"vert"),
        ttf_parser::Tag::from_bytes(b"vrt2"),
    ];
    let mut lookups: Vec<u16> = gsub
        .features
        .into_iter()
        .filter(|f| tags.contains(&f.tag))
        .flat_map(|f| f.lookup_indices)
        .take(VERTICAL_LOOKUPS)
        .collect();
    lookups.sort_unstable();
    lookups.dedup();
    lookups
        .into_iter()
        .filter_map(|i| gsub.lookups.get(i))
        .any(|l| {
            l.subtables
            .into_iter::<SubstitutionSubtable<'_>>()
            .take(VERTICAL_LOOKUPS)
            .any(|s| matches!(s, SubstitutionSubtable::Single(s) if s.coverage().contains(glyph)))
        })
}
