//! The limits an SVG is held to, and what counts against them.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::images::ContentKey;

/// Most elements the converted tree may contain, counting every copy made
/// by `use`, markers and `url(#…)` references, nested SVG images included.
pub(super) const MAX_EXPANDED: usize = 250_000;

/// Most `data:` URL bytes decoded, counting every copy.
pub(super) const MAX_DATA_BYTES: usize = 128 << 20;

/// Most attribute text parsed, counting every copy: `usvg` parses an
/// element's attributes again for each copy, and stores inherited ones
/// again on every shape beneath them.
pub(super) const MAX_PARSED_BYTES: usize = 32 << 20;

/// Most style sheet declaration text copied onto elements: `usvg` stores
/// the declarations of every matching rule on each element, counting every
/// copy.
pub(super) const MAX_COPIED_BYTES: usize = 256 << 20;

/// Most text all entity references expand to.
pub(super) const MAX_ENTITY_TEXT: usize = 1 << 20;

/// Largest number in path data and in radii (`r`, `rx`, `ry`): an arc is
/// split into more curves the larger its radius (about 50 at this size,
/// in inches).
pub(super) const MAX_COORDINATE: f64 = 1e7;

/// Most vertices of a rectangle, circle or ellipse (four arcs at most
/// [`MAX_COORDINATE`]).
pub(super) const SHAPE_VERTICES: usize = 300;

/// Vertices per number in path data with arcs: an arc takes seven numbers
/// and becomes at most about 50 curves.
pub(super) const ARC_VERTICES_PER_NUMBER: usize = 10;

/// Most style parsing work (see [`css_work`]) over every `style` attribute
/// and sheet, counting every copy: a few tenths of a second.
pub(super) const MAX_STYLE_WORK: usize = 200_000_000;

/// Most style sheet matching work, counting every copy: `usvg` matches
/// every selector against each element it copies, and a selector with `k`
/// descendant or sibling combinators may try `depthᵏ` ancestors.
pub(super) const MAX_RULE_MATCHES: usize = 50_000_000;

/// Most characters of SVG text, counting every copy (`use`, markers,
/// patterns) and nested SVG images.
pub(super) const MAX_TEXT_CHARS: usize = 1_000_000;

/// Most path segments one SVG may draw, counting every copy
/// ([`drawn_segments`]). The PDF writer holds the segments and the glyphs
/// of the SVG's text, together, to the same number.
pub(crate) const MAX_SEGMENTS: usize = 5_000_000;

/// Most bytes the placeholders of one SVG document's text may take
/// (`svg_text/rewrite.rs`), each counted once: they are parsed again, so
/// the same as [`MAX_PARSED_BYTES`], but counted before they are written.
pub(super) const MAX_PROBE_BYTES: usize = MAX_PARSED_BYTES;

/// Most pixels of distinct raster images in one SVG, nested SVGs included
/// (the PDF writer decodes each once).
pub(super) const MAX_RASTER_PIXELS: u64 = 4 * crate::images::MAX_PIXELS;

/// Deepest element nesting, counting followed references and the elements
/// around nested SVG images (they are converted on the same stack).
pub(super) const MAX_DEPTH: usize = 100;

/// Deepest tag nesting the XML parser is given, which recurses per
/// element: far past [`MAX_DEPTH`], for elements `check` does not visit,
/// and within a small thread stack.
pub(super) const MAX_XML_DEPTH: usize = 256;

/// Deepest SVG-in-SVG nesting. Real files embed an icon or a chart one
/// level deep; each level adds a parse and a conversion to the stack on
/// top of what [`MAX_DEPTH`] allows.
pub(super) const MAX_NESTING: usize = 4;

/// XML nodes accepted by the parser.
pub(super) const MAX_XML_NODES: u32 = 1_000_000;

/// Largest intrinsic size, in CSS pixels per side.
pub(super) const MAX_SIDE: f32 = 100_000.0;

/// Smallest intrinsic size, in CSS pixels per side, and smallest `viewBox`
/// side, in user units: `1 / MAX_SIDE`. The PDF writer scales the SVG to
/// the unit square by the inverse of its size, and its `viewBox` up to
/// that size, so these bound the scale (at most `MAX_SIDE / MIN_SIDE`,
/// 1e10) to numbers PDF readers parse.
pub(super) const MIN_SIDE: f32 = 1e-5;

/// Why an SVG was rejected. A limit or unsupported content fails the
/// outermost SVG, so a nested SVG fails as it would on its own; anything
/// else only leaves a nested SVG out, as `usvg` does.
pub(super) enum Reject {
    Limit(String),
    Unsupported(String),
    Invalid(String),
}

impl Reject {
    pub(super) fn message(self) -> String {
        match self {
            Reject::Limit(e) | Reject::Unsupported(e) | Reject::Invalid(e) => e,
        }
    }
}

pub(super) fn too_complex(what: &str) -> Reject {
    Reject::Limit(format!("SVG is too complex: {what}"))
}

/// What a document expands to, its nested SVG images included.
#[derive(Clone, Copy, Default)]
pub(super) struct Cost {
    pub(super) elements: usize,
    pub(super) data_bytes: usize,
    pub(super) parsed_bytes: usize,
    pub(super) copied_bytes: usize,
    pub(super) style_work: usize,
    pub(super) rule_matches: usize,
    pub(super) text_chars: usize,
}

impl Cost {
    pub(super) fn add(&mut self, other: Cost) -> Result<(), Reject> {
        self.elements = self.elements.saturating_add(other.elements);
        self.data_bytes = self.data_bytes.saturating_add(other.data_bytes);
        self.parsed_bytes = self.parsed_bytes.saturating_add(other.parsed_bytes);
        self.copied_bytes = self.copied_bytes.saturating_add(other.copied_bytes);
        self.style_work = self.style_work.saturating_add(other.style_work);
        self.rule_matches = self.rule_matches.saturating_add(other.rule_matches);
        self.text_chars = self.text_chars.saturating_add(other.text_chars);
        self.check()
    }

    pub(super) fn check(&self) -> Result<(), Reject> {
        if self.elements > MAX_EXPANDED {
            return Err(too_complex(&format!(
                "more than {MAX_EXPANDED} elements once references and nested SVG images are \
                 expanded"
            )));
        }
        if self.data_bytes > MAX_DATA_BYTES {
            return Err(too_complex(&format!(
                "more than {} MiB of embedded images once references are expanded",
                MAX_DATA_BYTES >> 20
            )));
        }
        if self.parsed_bytes > MAX_PARSED_BYTES {
            return Err(too_complex(&format!(
                "more than {} MiB of path data, styles and other attributes once references \
                 are expanded",
                MAX_PARSED_BYTES >> 20
            )));
        }
        if self.copied_bytes > MAX_COPIED_BYTES {
            return Err(too_complex(&format!(
                "style sheets copy more than {} MiB onto elements once references are expanded",
                MAX_COPIED_BYTES >> 20
            )));
        }
        if self.style_work > MAX_STYLE_WORK {
            return Err(too_complex(
                "style attributes or sheets are too long once references are expanded",
            ));
        }
        if self.text_chars > MAX_TEXT_CHARS {
            return Err(too_complex(&format!(
                "more than {MAX_TEXT_CHARS} characters of text once references are expanded"
            )));
        }
        if self.rule_matches > MAX_RULE_MATCHES {
            return Err(too_complex(&format!(
                "more than {MAX_RULE_MATCHES} style rule matches once references are expanded \
                 (selectors with many descendant combinators count more)"
            )));
        }
        Ok(())
    }
}

/// What `usvg` resolves while converting one SVG and its nested SVGs: a
/// second count, in case it copies more than the checks expect. Atomics
/// and a mutex because `usvg` wants a `Sync` resolver; one thread uses it.
#[derive(Default)]
pub(super) struct Meter {
    pub(super) elements: AtomicUsize,
    pub(super) data_bytes: AtomicUsize,
    pub(super) pixels: AtomicU64,
    /// Raster images seen.
    pub(super) rasters: Mutex<HashSet<ContentKey<Arc<Vec<u8>>>>>,
    /// The first limit exceeded or unsupported content met. A nested SVG
    /// that fails is left out, so this carries the error out to the
    /// outermost SVG.
    pub(super) error: OnceLock<String>,
}

impl Meter {
    pub(super) fn charge(&self, elements: usize, data_bytes: usize) -> Result<(), Reject> {
        let add = |n: &AtomicUsize, k: usize| n.fetch_add(k, Ordering::Relaxed).saturating_add(k);
        Cost {
            elements: add(&self.elements, elements),
            data_bytes: add(&self.data_bytes, data_bytes),
            ..Cost::default()
        }
        .check()
    }

    /// Charges a raster image's pixels the first time its content is seen.
    pub(super) fn charge_raster(&self, data: &Arc<Vec<u8>>, pixels: u64) -> Result<(), Reject> {
        let mut rasters = self.rasters.lock().unwrap_or_else(PoisonError::into_inner);
        if !rasters.insert(ContentKey::new(data.clone())) {
            return Ok(());
        }
        if self
            .pixels
            .fetch_add(pixels, Ordering::Relaxed)
            .saturating_add(pixels)
            > MAX_RASTER_PIXELS
        {
            return Err(too_complex(&format!(
                "more than {} megapixels of embedded images",
                MAX_RASTER_PIXELS / 1_000_000
            )));
        }
        Ok(())
    }
}

/// The path segments the PDF writer draws for group `g` (`clip`: in a
/// clip path, where it only fills), counting every copy, as it counts them
/// against [`MAX_SEGMENTS`]: a path once for its fill and once for its
/// stroke, and the paths of clip paths, masks, patterns and nested SVG
/// images each time they are drawn. At least what the writer draws.
pub(super) fn drawn_segments(g: &usvg::Group, clip: bool) -> usize {
    let mut n = 0usize;
    let mut cp = g.clip_path();
    while let Some(c) = cp {
        n = n.saturating_add(drawn_segments(c.root(), true));
        cp = c.clip_path();
    }
    let mut mask = g.mask().filter(|_| !clip);
    while let Some(m) = mask {
        n = n.saturating_add(drawn_segments(m.root(), false));
        mask = m.mask();
    }
    for child in g.children() {
        let k = match child {
            usvg::Node::Group(c) => drawn_segments(c, clip),
            usvg::Node::Path(p) if !p.is_visible() => 0,
            usvg::Node::Path(p) if clip => p.fill().map_or(0, |_| p.data().len()),
            usvg::Node::Path(p) => {
                let fill = p.fill().map(|f| f.paint());
                let stroke = p.stroke().map(|s| s.paint());
                let mut k = 0usize;
                for paint in [fill, stroke].into_iter().flatten() {
                    k = k.saturating_add(p.data().len());
                    if let usvg::Paint::Pattern(pt) = paint {
                        k = k.saturating_add(drawn_segments(pt.root(), false));
                    }
                }
                k
            }
            usvg::Node::Image(img) if !clip && img.is_visible() => match img.kind() {
                usvg::ImageKind::SVG(tree) => drawn_segments(tree.root(), false),
                _ => 0,
            },
            usvg::Node::Image(_) => 0,
            usvg::Node::Text(t) => drawn_segments(t.flattened(), clip),
        };
        n = n.saturating_add(k);
    }
    n
}
