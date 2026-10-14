//! Replaces each `<text>` with a placeholder for `usvg` to convert: a
//! "probe", a rectangle path over the text's ink, or a group of probes,
//! one per paint style of its spans.
//!
//! A probe's `style` holds the text's effects and the paint its spans
//! declare, or `inherit`, so that `usvg` resolves the paint the text would
//! get, through `use` copies too, whatever an author rule says about paths.
//! The PDF writer recognises a probe by its geometry, not its id: `usvg`
//! drops ids in `use` copies and markers. Each probe's corners are
//! distinct, so a probe names one text and one paint style.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::SvgText;
use crate::text::LaidText;

/// Largest probe coordinate: the SVG checks refuse path data beyond 1e7.
/// Text further out is off any page; its probe stays at the edge.
const MAX_PROBE: f32 = 1e6;

/// Probe corners `[x0, y0, x1, y1]` (as bits) → the text (its index in the
/// render's list) and the paint style it draws.
pub(crate) type Probes = HashMap<[u32; 4], (u32, u32)>;

/// Declarations every probe ends with: markers would be drawn on it.
const NO_MARKERS: &str = "marker-start:none;marker-mid:none;marker-end:none";

/// The paint properties of a group placeholder: inherited, not taken from
/// an author rule about `a`.
const INHERIT_PAINT: &str = "fill:inherit;fill-opacity:inherit;fill-rule:inherit;stroke:inherit;\
stroke-width:inherit;stroke-opacity:inherit;stroke-linecap:inherit;stroke-linejoin:inherit;\
stroke-miterlimit:inherit;stroke-dasharray:inherit;stroke-dashoffset:inherit;paint-order:inherit;\
visibility:inherit;color:inherit;";

/// Corners for a probe over `ink` that no other probe has.
fn unique(ink: [f32; 4], probes: &Probes) -> [f32; 4] {
    let [mut x0, mut y0, mut x1, mut y1] = ink.map(|v| v.clamp(-MAX_PROBE, MAX_PROBE));
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
    }
    if y0 > y1 {
        std::mem::swap(&mut y0, &mut y1);
    }
    if x1 <= x0 {
        x1 = x0.next_up();
    }
    if y1 <= y0 {
        y1 = y0.next_up();
    }
    while probes.contains_key(&[x0, y0, x1, y1].map(f32::to_bits)) {
        x1 = x1.next_up();
    }
    [x0, y0, x1, y1]
}

/// Path data for a probe: four lines, numbers written so that `usvg`
/// reads back the same `f32`s.
fn probe_path(r: [f32; 4]) -> String {
    let [x0, y0, x1, y1] = r.map(f64::from);
    format!("M{x0} {y0}L{x1} {y0}L{x1} {y1}L{x0} {y1}Z")
}

/// The placeholder for `text`, laid out as `laid`, which is text `index`
/// of the render; its probes go into `probes`.
pub(crate) fn placeholder(
    text: &SvgText,
    laid: &LaidText,
    index: u32,
    probes: &mut Probes,
) -> String {
    let Some(ink) = laid.ink else {
        // Nothing to draw; a `use` of its id draws nothing too.
        return format!("<g{}/>", text.attrs);
    };
    let mut paints: Vec<u32> = laid
        .glyphs
        .iter()
        .map(|g| g.paint)
        .chain(laid.decorations.iter().map(|d| d.paint))
        .collect();
    paints.sort_unstable();
    paints.dedup();
    let mut out = String::new();
    let mut probe = |out: &mut String, paint: u32, lead: &str, attrs: &str| {
        let r = unique(ink, probes);
        probes.insert(r.map(f32::to_bits), (index, paint));
        let style = text.paints.get(paint as usize).map_or("", String::as_str);
        let _ = write!(
            out,
            r#"<path{attrs} d="{}" style="{}{}{NO_MARKERS}"/>"#,
            probe_path(r),
            super::escape(lead),
            super::escape(style),
        );
    };
    if text.in_clip {
        // Clip paths take shapes only: one bare probe each.
        for (k, &paint) in paints.iter().enumerate() {
            let attrs = if k == 0 { text.attrs.as_str() } else { "" };
            probe(&mut out, paint, &text.effects, attrs);
        }
    } else if let [paint] = paints[..] {
        probe(&mut out, paint, &text.effects, &text.attrs);
    } else {
        let _ = write!(
            out,
            r#"<a{} style="{}{INHERIT_PAINT}">"#,
            text.attrs,
            super::escape(&text.effects)
        );
        for &paint in &paints {
            probe(&mut out, paint, "", "");
        }
        out.push_str("</a>");
    }
    out
}

/// `source` with each of `texts` replaced by its placeholder. `laid` holds
/// their layouts, and `first` is the render's index of the first.
pub(crate) fn rewrite(
    source: &str,
    texts: &[SvgText],
    laid: &[LaidText],
    first: u32,
    probes: &mut Probes,
) -> String {
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for (k, (text, laid)) in texts.iter().zip(laid).enumerate() {
        out.push_str(&source[at..text.range.start]);
        out.push_str(&placeholder(text, laid, first + k as u32, probes));
        at = text.range.end;
    }
    out.push_str(&source[at..]);
    out
}
