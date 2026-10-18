//! Replaces each `<text>` with a placeholder for `usvg` to convert: a
//! "probe", a rectangle over the text's ink, or a group of probes, one per
//! paint style of its spans.
//!
//! A probe's `style` holds the text's effects and the paint its spans
//! declare, or `inherit` where a style sheet could say otherwise, so that
//! `usvg` resolves the paint the text would get, through `use` copies too,
//! whatever an author rule says about the probe's element (see
//! [`ProbeForm`]). The PDF writer recognises a probe by its geometry, not
//! its id: `usvg` drops ids in `use` copies and markers. Each probe's
//! corners carry its index, so a probe names one text and one paint style.
//!
//! What a text's placeholder can be depends on the text alone, not on its
//! layout: a single probe when the text has one paint style, a group of
//! them otherwise, bare probes in a clip path. So the decode checks the
//! placeholders with the widest geometry and every paint style
//! ([`worst_case`]), and no render's placeholders cost the SVG checks
//! more.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::Range;

use super::{push_escaped, ProbeForm, ProbeShape, SvgText, SvgTextDoc, UseCopy};
use crate::text::LaidText;

/// Largest probe coordinate: the SVG checks refuse path data beyond 1e7.
/// Text further out is off any page; its probe stays at the edge.
const MAX_PROBE: f32 = 1e6;

/// Probe corners `[x0, y0, x1, y1]` (as bits) → the text (its index in the
/// render's list) and the paint style it draws.
pub(crate) type Probes = HashMap<[u32; 4], (u32, u32)>;

/// Declarations every probe ends with: markers would be drawn on it.
const NO_MARKERS: &str = "marker-start:none;marker-mid:none;marker-end:none";

/// Longest number in a probe's geometry: an `f32` written as the shortest
/// `f64` in exponent form, sign included.
const NUMBER_LEN: usize = 24;
/// A number that long, for the widest geometry.
const WIDEST_NUMBER: &str = "-1.00000000000000000e-10";
/// Longest geometry attribute value: eight numbers and what separates them.
const GEOMETRY_LEN: usize = 8 * NUMBER_LEN + 9;
/// Markup of a probe besides its attribute values
/// (`<polygon points="" style=""/>`, rounded up).
const PROBE_MARKUP: usize = 32;
/// Markup of a group besides its attribute values (`<a style=""></a>`,
/// rounded up), or of an empty placeholder.
const GROUP_MARKUP: usize = 24;

/// The most bytes the placeholders of `doc` take in the rewritten source,
/// whatever their layout.
pub(crate) fn most_bytes(doc: &SvgTextDoc) -> usize {
    let texts = doc.texts.iter().fold(0usize, |sum, t| {
        sum.saturating_add(text_bytes(t, &doc.form))
    });
    let copies = doc.copies.iter().fold(DEFS.len(), |sum, c| {
        let hrefs = c.hrefs.len() * (c.id.len() + 1);
        sum.saturating_add(c.range.len() + c.id.len() + 8 + hrefs)
    });
    texts.saturating_add(copies)
}

/// What holds the copies of [`SvgTextDoc::copies`].
const DEFS: &str = "<defs></defs>";

fn text_bytes(text: &SvgText, form: &ProbeForm) -> usize {
    let effects = escaped_len(&text.effects);
    let probe = |lead: usize, paint: &String| {
        PROBE_MARKUP + GEOMETRY_LEN + lead + escaped_len(paint) + NO_MARKERS.len()
    };
    let probes: usize = if text.in_clip || text.paints.len() == 1 {
        text.paints.iter().map(|p| probe(effects, p)).sum()
    } else {
        effects + form.group_paint.len() + text.paints.iter().map(|p| probe(0, p)).sum::<usize>()
    };
    GROUP_MARKUP + text.attrs.len() + probes
}

fn escaped_len(v: &str) -> usize {
    v.len()
        + v.bytes()
            .map(|b| match b {
                b'"' => 5,
                b'&' => 4,
                b'<' | b'>' => 3,
                _ => 0,
            })
            .sum::<usize>()
}

/// How many low bits of each corner value carry a probe's index, for at
/// most `most` probes: a quarter of the index's bits each.
pub(crate) fn index_bits(most: usize) -> u32 {
    (usize::BITS - most.saturating_sub(1).leading_zeros()).div_ceil(4)
}

/// `v` as an unsigned number in the order of the floats.
fn ordered(v: f32) -> u32 {
    let b = v.to_bits();
    if b >> 31 == 1 {
        !b
    } else {
        b | 1 << 31
    }
}

fn from_ordered(u: u32) -> f32 {
    f32::from_bits(if u >> 31 == 1 { u & !(1 << 31) } else { !u })
}

/// The corners of probe `index` over `ink`: its box, with the index in
/// the low `bits` bits of the four values (a few units in the last place),
/// so no two probes of a render share their corners.
fn corners(ink: [f32; 4], index: u32, bits: u32) -> [f32; 4] {
    let [x0, y0, x1, y1] = ink.map(|v| ordered(v.clamp(-MAX_PROBE, MAX_PROBE)));
    let (mut x0, mut x1) = (x0.min(x1), x0.max(x1));
    let (mut y0, mut y1) = (y0.min(y1), y0.max(y1));
    let mask = (1u32 << bits) - 1;
    // Wide enough that the index keeps the sides apart.
    let least = 2 << bits;
    x1 = x1.max(x0 + least);
    y1 = y1.max(y0 + least);
    for (k, v) in [&mut x0, &mut y0, &mut x1, &mut y1].into_iter().enumerate() {
        let part = index.checked_shr(k as u32 * bits).unwrap_or(0) & mask;
        *v = (*v & !mask) | part;
    }
    [x0, y0, x1, y1].map(from_ordered)
}

/// A probe's geometry: its corners, or the widest numbers.
#[derive(Clone, Copy)]
enum Geometry {
    Corners([f32; 4]),
    Widest,
}

/// The geometry attribute of a probe, with a leading space.
fn push_geometry(out: &mut String, shape: ProbeShape, g: Geometry) {
    let [x0, y0, x1, y1] = match g {
        Geometry::Corners(r) => r.map(|v| format!("{:e}", f64::from(v))),
        Geometry::Widest => [(); 4].map(|()| WIDEST_NUMBER.to_string()),
    };
    let _ = match shape {
        ProbeShape::Path => write!(out, r#" d="M{x0} {y0}L{x1} {y0}L{x1} {y1}L{x0} {y1}Z""#),
        ProbeShape::Polygon => write!(out, r#" points="{x0},{y0} {x1},{y0} {x1},{y1} {x0},{y1}""#),
    };
}

/// Writes the placeholder of `text` with a probe for each of `paints`
/// (none: a placeholder that draws nothing, whose id a `use` may name),
/// each with the geometry `geometry` gives it.
fn placeholder(
    out: &mut String,
    text: &SvgText,
    form: &ProbeForm,
    paints: &[u32],
    geometry: &mut dyn FnMut(u32) -> Geometry,
) {
    if paints.is_empty() {
        let _ = write!(out, "<g{}/>", text.attrs);
        return;
    }
    let tag = form.shape.tag();
    let mut probe = |out: &mut String, paint: u32, lead: &str, attrs: &str| {
        let style = text.paints.get(paint as usize).map_or("", String::as_str);
        let _ = write!(out, "<{tag}{attrs}");
        push_geometry(out, form.shape, geometry(paint));
        out.push_str(r#" style=""#);
        push_escaped(out, lead);
        push_escaped(out, style);
        out.push_str(NO_MARKERS);
        out.push_str(r#""/>"#);
    };
    if text.in_clip {
        // Clip paths take shapes only: one bare probe each.
        for (k, &paint) in paints.iter().enumerate() {
            let attrs = if k == 0 { text.attrs.as_str() } else { "" };
            probe(out, paint, &text.effects, attrs);
        }
    } else if text.paints.len() == 1 {
        probe(out, paints[0], &text.effects, &text.attrs);
    } else {
        let _ = write!(out, r#"<{}{} style=""#, form.group, text.attrs);
        push_escaped(out, &text.effects);
        out.push_str(&form.group_paint);
        out.push_str(r#"">"#);
        for &paint in paints {
            probe(out, paint, "", "");
        }
        let _ = write!(out, "</{}>", form.group);
    }
}

/// The paint styles `laid` draws, in order.
fn paints_of(laid: &LaidText) -> Vec<u32> {
    let mut paints: Vec<u32> = laid
        .glyphs
        .iter()
        .map(|g| g.paint)
        .chain(laid.decorations.iter().map(|d| d.paint))
        .chain(laid.images.iter().map(|i| i.paint))
        .collect();
    paints.sort_unstable();
    paints.dedup();
    paints
}

/// `source` with each text of `doc` replaced by its placeholder. `laid`
/// holds their layouts, and `first` is the render's index of the first.
/// The probes go into `probes`, numbered on from those there, with their
/// number in `bits` bits of each corner (see [`index_bits`]). The caller
/// bounds [`most_bytes`] first.
pub(crate) fn rewrite(
    source: &str,
    doc: &SvgTextDoc,
    laid: &[LaidText],
    first: u32,
    (probes, bits): (&mut Probes, u32),
) -> String {
    assemble(source, doc, |out, k| {
        let (text, laid) = (&doc.texts[k], &laid[k]);
        let text_index = first + k as u32;
        let paints = match laid.ink {
            Some(_) => paints_of(laid),
            None => Vec::new(),
        };
        let ink = laid.ink.unwrap_or_default();
        placeholder(out, text, &doc.form, &paints, &mut |paint| {
            let r = corners(ink, probes.len() as u32, bits);
            probes.insert(r.map(f32::to_bits), (text_index, paint));
            Geometry::Corners(r)
        });
    })
}

/// An edit of the source: a text replaced by its placeholder, a `use`
/// pointed at its copy, or the copies put in.
enum Edit {
    Text(usize),
    Href(usize),
    Copies,
}

/// `source` with each text of `doc` replaced by what `put` writes for
/// it (by its index), each `use` with a copy ([`SvgTextDoc::copies`])
/// naming the copy, and the copies, with their texts so replaced, in a
/// `defs` at the end of the root element.
fn assemble(source: &str, doc: &SvgTextDoc, mut put: impl FnMut(&mut String, usize)) -> String {
    let mut out = String::with_capacity(source.len().saturating_add(most_bytes(doc)));
    // The texts are in source order, and so are the `use` elements.
    let hrefs = doc
        .copies
        .iter()
        .enumerate()
        .flat_map(|(c, copy)| copy.hrefs.iter().map(move |r| (r.clone(), Edit::Href(c))));
    let mut texts = (0..doc.in_place())
        .map(|k| (doc.texts[k].range.clone(), Edit::Text(k)))
        .peekable();
    let mut hrefs = hrefs
        .chain((!doc.copies.is_empty()).then_some((doc.copies_at..doc.copies_at, Edit::Copies)))
        .peekable();
    let mut at = 0;
    loop {
        let next = match (texts.peek(), hrefs.peek()) {
            (Some(t), Some(h)) if h.0.start < t.0.start => hrefs.next(),
            (Some(_), _) => texts.next(),
            (None, Some(_)) => hrefs.next(),
            (None, None) => break,
        };
        let Some((range, edit)) = next else { break };
        out.push_str(&source[at..range.start]);
        match edit {
            Edit::Text(k) => put(&mut out, k),
            Edit::Href(c) => {
                out.push('#');
                out.push_str(&doc.copies[c].id);
            }
            Edit::Copies => {
                out.push_str("<defs>");
                for copy in &doc.copies {
                    copy_of(&mut out, source, doc, copy, &mut put);
                }
                out.push_str("</defs>");
            }
        }
        at = range.end;
    }
    out.push_str(&source[at..]);
    out
}

/// The copy `copy` of an element: its source, under the copy's id, the
/// ids in it left out and its texts replaced by what `put` writes.
fn copy_of(
    out: &mut String,
    source: &str,
    doc: &SvgTextDoc,
    copy: &UseCopy,
    put: &mut impl FnMut(&mut String, usize),
) {
    let mut edits: Vec<(Range<usize>, Option<usize>)> = copy
        .texts
        .clone()
        .map(|k| (doc.texts[k].range.clone(), Some(k)))
        .chain(copy.ids.iter().map(|r| (r.clone(), None)))
        .collect();
    if let Some(r) = &copy.own_id {
        edits.push((r.clone(), None));
    }
    edits.sort_by_key(|e| e.0.start);
    let mut at = copy.range.start;
    for (range, text) in edits {
        out.push_str(&source[at..range.start]);
        match text {
            Some(k) => put(out, k),
            None if Some(&range) == copy.own_id.as_ref() => {
                out.push_str("id=\"");
                out.push_str(&copy.id);
                out.push('"');
            }
            None => {}
        }
        at = range.end;
    }
    out.push_str(&source[at..copy.range.end]);
}

/// `source` with each text of `doc` replaced by the placeholder that
/// costs the SVG checks most: every paint style, the widest geometry.
/// The caller bounds [`most_bytes`] first.
pub(crate) fn worst_case(source: &str, doc: &SvgTextDoc) -> String {
    assemble(source, doc, |out, k| {
        let text = &doc.texts[k];
        let paints: Vec<u32> = (0..text.paints.len() as u32).collect();
        placeholder(out, text, &doc.form, &paints, &mut |_| Geometry::Widest);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_carry_the_index_and_keep_the_box() {
        let ink = [1.5, -8.25, 30.125, 2.0];
        let bits = index_bits(100_000);
        assert_eq!(bits, 5);
        let mut seen = std::collections::HashSet::new();
        for i in 0..100_000 {
            let r = corners(ink, i, bits);
            assert!(seen.insert(r.map(f32::to_bits)), "probe {i}");
            for (v, w) in r.iter().zip(ink) {
                assert!((v - w).abs() <= w.abs() * 1e-5, "{r:?}");
            }
        }
        // A box without area, at zero, gets one.
        let r = corners([0.0, -0.0, 0.0, 0.0], 3, index_bits(4));
        assert!(r[0] < r[2] && r[1] < r[3], "{r:?}");
        // One probe: its corners are the box's.
        assert_eq!(corners(ink, 0, index_bits(1)), ink);
    }

    /// Texts on one point get their own probes in one step each.
    #[test]
    fn rewrites_many_texts_on_one_point_in_linear_time() {
        crate::test_timing::assert_linear("texts", 50_000, |n| {
            let source = "<text>a</text>".repeat(n);
            let texts = (0..n)
                .map(|i| SvgText {
                    range: i * 14..(i + 1) * 14,
                    attrs: String::new(),
                    effects: String::new(),
                    in_clip: false,
                    chars: Vec::new(),
                    spans: Vec::new(),
                    paints: vec!["fill:red;".into()],
                    adjust: Vec::new(),
                    rtl: false,
                    writing: super::super::Writing::Horizontal,
                    paths: Vec::new(),
                })
                .collect();
            let doc = SvgTextDoc {
                texts,
                form: ProbeForm::default(),
                copies: Vec::new(),
                copies_at: 0,
            };
            let glyph = crate::text::SvgGlyph {
                font: 0,
                id: 1,
                size: 1.0,
                x: 0.0,
                y: 0.0,
                rotate: 0.0,
                scale_x: 1.0,
                paint: 0,
                text: (0, 0),
                char: 0,
                upright: false,
            };
            let laid = vec![
                LaidText {
                    glyphs: vec![glyph],
                    decorations: Vec::new(),
                    images: Vec::new(),
                    ink: Some([0.0, 0.0, 1.0, 1.0]),
                    text: String::new(),
                    actual: Vec::new(),
                };
                n
            ];
            let t = std::time::Instant::now();
            let mut probes = Probes::new();
            let bits = index_bits(doc.most_probes());
            let out = rewrite(&source, &doc, &laid, 0, (&mut probes, bits));
            let took = t.elapsed();
            assert_eq!(probes.len(), n);
            assert_eq!(out.matches("<path").count(), n);
            assert!(out.len() <= most_bytes(&doc));
            took
        });
    }
}
