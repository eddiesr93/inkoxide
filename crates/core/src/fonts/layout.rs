//! Reads fonts' layout tables: finds the vertical forms of glyphs.

use read_fonts::tables::gsub::{SingleSubst, SubstitutionSubtables};
use read_fonts::types::{BigEndian, GlyphId, Tag};
use read_fonts::{FontRef, TableProvider};

/// Most features, lookups of the `vert` feature, and subtables of each,
/// [`vertical_form`] reads: real fonts have a few dozen features and a few
/// such lookups; a malformed one could list thousands and make each question
/// cost as much.
const VERTICAL_FEATURES: usize = 1024;
const VERTICAL_LOOKUPS: usize = 64;

/// Whether `glyph` has a vertical form: a single substitution of a lookup
/// of the font's `vert` feature covers it. Only `vert`: it is the feature
/// the shaper applies to vertical text, and `vrt2` it does not.
pub(super) fn vertical_form(tables: &FontRef<'_>, glyph: GlyphId) -> bool {
    let Ok(gsub) = tables.gsub() else {
        return false;
    };
    let (Ok(features), Ok(lookups)) = (gsub.feature_list(), gsub.lookup_list()) else {
        return false;
    };
    let vert = Tag::new(b"vert");
    let mut indices: Vec<u16> = features
        .feature_records()
        .iter()
        .take(VERTICAL_FEATURES)
        .filter(|r| r.feature_tag() == vert)
        .filter_map(|r| r.feature(features.offset_data()).ok())
        .flat_map(|f| f.lookup_list_indices().iter().map(BigEndian::get))
        .take(VERTICAL_LOOKUPS)
        .collect();
    indices.sort_unstable();
    indices.dedup();
    let covers = |s: SingleSubst<'_>| {
        let coverage = match s {
            SingleSubst::Format1(s) => s.coverage(),
            SingleSubst::Format2(s) => s.coverage(),
            _ => return false,
        };
        coverage.is_ok_and(|c| c.get(glyph).is_some())
    };
    indices
        .into_iter()
        .filter_map(|i| lookups.lookups().get(usize::from(i)).ok())
        .any(|l| match l.subtables() {
            Ok(SubstitutionSubtables::Single(s)) => s
                .iter()
                .take(VERTICAL_LOOKUPS)
                .filter_map(Result::ok)
                .any(covers),
            _ => false,
        })
}
