//! SVG parsing through `usvg`, with limits for untrusted input.
//!
//! The XML is parsed once with `roxmltree` (re-exported by `usvg`), checked,
//! then converted. The checks run before `usvg` builds its tree because
//! `usvg` expands `use` references, draws markers once per vertex and
//! clones paint servers, clip paths and masks per element, so a small file
//! can describe a huge tree.
//!
//! Nothing is read from disk: `href`s to files are ignored and only
//! `data:` URLs are resolved. The checks count a nested SVG image once per
//! copy, its own nested images included. `usvg` resolves the URL again
//! for most copies, so the resolver also counts what it actually decodes,
//! in one [`Meter`] for the whole SVG.
//!
//! Where mirroring `usvg` would be fragile, the input is refused instead:
//! entities that expand to more than a little text, `href` chains that
//! loop, and radii so large that an arc becomes millions of curves.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use usvg::roxmltree::{self, Document, Node, NodeId};

use crate::paint::Rect;

/// Most elements the converted tree may contain, counting every copy made
/// by `use`, markers and `url(#…)` references, nested SVG images included.
const MAX_EXPANDED: usize = 250_000;
/// Most `data:` URL bytes decoded, counting every copy.
const MAX_DATA_BYTES: usize = 128 << 20;
/// Most attribute text parsed, counting every copy: `usvg` parses an
/// element's attributes again for each copy, and stores inherited ones
/// again on every shape beneath them.
const MAX_PARSED_BYTES: usize = 32 << 20;
/// Most style sheet declaration text copied onto elements: `usvg` stores
/// the declarations of every matching rule on each element, counting every
/// copy.
const MAX_COPIED_BYTES: usize = 256 << 20;
/// Most text all entity references expand to.
const MAX_ENTITY_TEXT: usize = 1 << 20;
/// Largest number in path data and in radii (`r`, `rx`, `ry`): an arc is
/// split into more curves the larger its radius (about 50 at this size,
/// in inches).
const MAX_COORDINATE: f64 = 1e7;
/// Most vertices of a rectangle, circle or ellipse (four arcs at most
/// [`MAX_COORDINATE`]).
const SHAPE_VERTICES: usize = 300;
/// Vertices per number in path data with arcs: an arc takes seven numbers
/// and becomes at most about 50 curves.
const ARC_VERTICES_PER_NUMBER: usize = 10;
/// Most style parsing work (see [`css_work`]) over every `style` attribute
/// and sheet, counting every copy: a few tenths of a second.
const MAX_STYLE_WORK: usize = 200_000_000;
/// Most style sheet matching work, counting every copy: `usvg` matches
/// every selector against each element it copies, and a selector with `k`
/// descendant or sibling combinators may try `depthᵏ` ancestors.
const MAX_RULE_MATCHES: usize = 50_000_000;
/// Most pixels of distinct raster images in one SVG, nested SVGs included
/// (the PDF writer decodes each once).
const MAX_RASTER_PIXELS: u64 = 4 * super::MAX_PIXELS;
/// Deepest element nesting, counting followed references and the elements
/// around nested SVG images (they are converted on the same stack).
const MAX_DEPTH: usize = 100;
/// Deepest SVG-in-SVG nesting. Real files embed an icon or a chart one
/// level deep; each level adds a parse and a conversion to the stack on
/// top of what [`MAX_DEPTH`] allows.
const MAX_NESTING: usize = 4;
/// XML nodes accepted by the parser.
const MAX_XML_NODES: u32 = 1_000_000;
/// Largest intrinsic size, in CSS pixels per side.
const MAX_SIDE: f32 = 100_000.0;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Looks like SVG: optional BOM and whitespace, then `<`.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = b.iter().position(|c| !c.is_ascii_whitespace());
    start.is_some_and(|i| b[i] == b'<')
}

/// Why an SVG was rejected. A limit fails the outermost SVG; anything else
/// only leaves a nested SVG out, as `usvg` does.
enum Reject {
    Limit(String),
    Invalid(String),
}

impl Reject {
    fn message(self) -> String {
        match self {
            Reject::Limit(e) | Reject::Invalid(e) => e,
        }
    }
}

fn too_complex(what: &str) -> Reject {
    Reject::Limit(format!("SVG is too complex: {what}"))
}

/// What a document expands to, its nested SVG images included.
#[derive(Clone, Copy, Default)]
struct Cost {
    elements: usize,
    data_bytes: usize,
    parsed_bytes: usize,
    copied_bytes: usize,
    style_work: usize,
    rule_matches: usize,
}

impl Cost {
    fn add(&mut self, other: Cost) -> Result<(), Reject> {
        self.elements = self.elements.saturating_add(other.elements);
        self.data_bytes = self.data_bytes.saturating_add(other.data_bytes);
        self.parsed_bytes = self.parsed_bytes.saturating_add(other.parsed_bytes);
        self.copied_bytes = self.copied_bytes.saturating_add(other.copied_bytes);
        self.style_work = self.style_work.saturating_add(other.style_work);
        self.rule_matches = self.rule_matches.saturating_add(other.rule_matches);
        self.check()
    }

    fn check(&self) -> Result<(), Reject> {
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
struct Meter {
    elements: AtomicUsize,
    data_bytes: AtomicUsize,
    pixels: AtomicU64,
    /// Raster images seen, by content hash.
    rasters: Mutex<HashMap<u64, Vec<Arc<Vec<u8>>>>>,
    /// The first limit exceeded. A nested SVG that fails is left out, so
    /// this carries the error out to the outermost SVG.
    error: OnceLock<String>,
}

impl Meter {
    fn charge(&self, elements: usize, data_bytes: usize) -> Result<(), Reject> {
        let add = |n: &AtomicUsize, k: usize| n.fetch_add(k, Ordering::Relaxed).saturating_add(k);
        Cost {
            elements: add(&self.elements, elements),
            data_bytes: add(&self.data_bytes, data_bytes),
            ..Cost::default()
        }
        .check()
    }

    /// Charges a raster image's pixels the first time its content is seen.
    fn charge_raster(&self, data: &Arc<Vec<u8>>, pixels: u64) -> Result<(), Reject> {
        let hash = xxhash_rust::xxh3::xxh3_64(data);
        let mut rasters = self.rasters.lock().unwrap_or_else(PoisonError::into_inner);
        let same = rasters.entry(hash).or_default();
        if same.iter().any(|d| d[..] == data[..]) {
            return Ok(());
        }
        same.push(data.clone());
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

/// The root `viewBox` of an SVG and how it fits a viewport of another
/// shape (`preserveAspectRatio`). `usvg` folds both into the tree's root
/// transform for the SVG's own size, so they are read here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewBox {
    /// `x`, `y`, width and height (both > 0), in user units.
    pub rect: [f32; 4],
    /// Where the `viewBox` sits in spare room, as a fraction of it per
    /// axis (`xMin` 0, `xMid` 0.5, `xMax` 1); `None` stretches it
    /// (`none`).
    pub align: Option<(f32, f32)>,
    /// `slice` (cover the viewport) rather than `meet` (fit inside it).
    pub slice: bool,
}

impl ViewBox {
    /// Reads the root element's `viewBox` and `preserveAspectRatio`.
    /// `None` without a usable `viewBox`.
    fn of(svg: Node<'_, '_>) -> Option<Self> {
        let v: Vec<f32> = svg
            .attribute("viewBox")?
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let &[x, y, w, h] = v.as_slice() else {
            return None;
        };
        if !(v.iter().all(|n| n.is_finite()) && w > 0.0 && h > 0.0) {
            return None;
        }
        let mut words = svg
            .attribute("preserveAspectRatio")
            .unwrap_or("")
            .split_ascii_whitespace()
            .skip_while(|&w| w == "defer");
        let align = match words.next() {
            Some("none") => None,
            Some(a) if a.len() == 8 && a.starts_with('x') && a[4..].starts_with('Y') => {
                let frac = |s: &str| match s {
                    "Min" => Some(0.0),
                    "Mid" => Some(0.5),
                    "Max" => Some(1.0),
                    _ => None,
                };
                // An invalid value is the default, `xMidYMid meet`.
                let Some(pair) = frac(&a[1..4]).zip(frac(&a[5..8])) else {
                    return Some(Self::new([x, y, w, h], Some((0.5, 0.5)), false));
                };
                Some(pair)
            }
            _ => Some((0.5, 0.5)),
        };
        Some(Self::new(
            [x, y, w, h],
            align,
            words.next() == Some("slice"),
        ))
    }

    fn new(rect: [f32; 4], align: Option<(f32, f32)>, slice: bool) -> Self {
        Self { rect, align, slice }
    }

    /// The part of `size` (the viewport, from the origin) the `viewBox`
    /// covers.
    fn fit(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let [_, _, vw, vh] = self.rect;
        let Some((ax, ay)) = self.align else {
            return (x, y, w, h);
        };
        let (sx, sy) = (w / vw, h / vh);
        let s = if self.slice { sx.max(sy) } else { sx.min(sy) };
        let (fw, fh) = (vw * s, vh * s);
        (x + ax * (w - fw), y + ay * (h - fh), fw, fh)
    }

    /// Where to draw an SVG of `size` so that its `viewBox` fits `bx`, the
    /// SVG's viewport there (as CSS draws an SVG image with `object-fit:
    /// fill`), and whether to clip to `bx`.
    pub fn place(&self, size: (f32, f32), bx: Rect) -> (Rect, bool) {
        /// Overflow below this (points) is rounding, not worth a clip.
        const CLIP_EPS: f32 = 1e-3;
        let (w, h) = size;
        let (fx, fy, fw, fh) = self.fit((0.0, 0.0, w, h));
        let (tx, ty, tw, th) = self.fit((bx.x, bx.y, bx.w, bx.h));
        let (sx, sy) = (tw / fw, th / fh);
        let r = Rect::new(tx - fx * sx, ty - fy * sy, w * sx, h * sy);
        let rect = if [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) {
            r
        } else {
            bx
        };
        let clip = rect.x < bx.x - CLIP_EPS
            || rect.y < bx.y - CLIP_EPS
            || rect.x + rect.w > bx.x + bx.w + CLIP_EPS
            || rect.y + rect.h > bx.y + bx.h + CLIP_EPS;
        (rect, clip)
    }
}

/// Parses an SVG document into a `usvg` tree, with its root `viewBox`.
pub(crate) fn parse(bytes: &[u8]) -> Result<(usvg::Tree, Option<ViewBox>), String> {
    let meter = Meter::default();
    let at = Nesting {
        meter: &meter,
        level: 0,
        depth: 0,
    };
    let tree = parse_nested(bytes, at).map_err(Reject::message);
    match meter.error.get() {
        Some(e) => Err(e.clone()),
        None => tree,
    }
}

/// Where an SVG sits inside the outermost one.
#[derive(Clone, Copy)]
struct Nesting<'m> {
    meter: &'m Meter,
    /// SVG images around it.
    level: usize,
    /// Element depth of its root.
    depth: usize,
}

fn parse_xml(bytes: &[u8]) -> Result<Document<'_>, Reject> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Reject::Invalid("SVG is not valid UTF-8".into()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    check_entities(text)?;
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        entity_resolver: None,
    };
    Document::parse_with_options(text, opts).map_err(|e| Reject::Invalid(format!("SVG: {e}")))
}

/// Refuses entities that expand to more than [`MAX_ENTITY_TEXT`] in all,
/// before the XML parser expands them. Entities may not refer to other
/// entities, so each reference expands to its own value; the short
/// namespace entities of drawing programs pass.
fn check_entities(text: &str) -> Result<(), Reject> {
    if !text.contains("<!ENTITY") {
        return Ok(());
    }
    let mut values: HashMap<&str, usize> = HashMap::new();
    for decl in text.split("<!ENTITY").skip(1) {
        let decl = decl.trim_start();
        if decl.starts_with('%') {
            return Err(Reject::Invalid(
                "SVG: parameter entities are not supported".into(),
            ));
        }
        let name_end = decl
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(decl.len());
        let (name, rest) = decl.split_at(name_end);
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            // External entities are not resolved.
            continue;
        };
        let value = rest[1..].split(quote).next().unwrap_or("");
        if value.contains(['&', '%']) {
            return Err(too_complex("an entity refers to other entities"));
        }
        values.insert(name, value.len());
    }
    let mut total = 0usize;
    for r in text.split('&').skip(1) {
        let Some((name, _)) = r.split_once(';') else {
            continue;
        };
        if let Some(&len) = values.get(name) {
            total = total.saturating_add(len);
            if total > MAX_ENTITY_TEXT {
                return Err(too_complex(&format!(
                    "entities expand to more than {} MiB",
                    MAX_ENTITY_TEXT >> 20
                )));
            }
        }
    }
    Ok(())
}

fn parse_nested(bytes: &[u8], at: Nesting<'_>) -> Result<(usvg::Tree, Option<ViewBox>), Reject> {
    let doc = parse_xml(bytes)?;
    let checked = check(&doc, at.level, at.depth)?;
    at.meter.charge(checked.own, 0)?;
    // Nested SVGs start below the deepest image, wherever they are.
    let inner = Nesting {
        meter: at.meter,
        level: at.level + 1,
        depth: checked.images + 1,
    };
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(move |mime, data, _| resolve_data(mime, data, inner)),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_xmltree(&doc, &options)
        .map_err(|e| Reject::Invalid(format!("SVG: {e}")))?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(Reject::Invalid("SVG has no usable size".into()));
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(Reject::Invalid(format!(
            "SVG is {w}×{h} px; at most {MAX_SIDE} px per side"
        )));
    }
    Ok((tree, ViewBox::of(doc.root_element())))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Embedded {
    Jpeg,
    Png,
    Svg,
}

/// What a decoded `data:` URL holds, as the resolver reads it.
fn embedded(mime: &str, data: &[u8]) -> Option<Embedded> {
    match mime {
        "image/jpg" | "image/jpeg" => Some(Embedded::Jpeg),
        "image/png" => Some(Embedded::Png),
        "image/svg+xml" => Some(Embedded::Svg),
        "text/plain" | "" => {
            if data.starts_with(b"\x89PNG") {
                Some(Embedded::Png)
            } else if data.starts_with(&[0xFF, 0xD8]) {
                Some(Embedded::Jpeg)
            } else if is_svg(data) {
                Some(Embedded::Svg)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn too_deep() -> Reject {
    too_complex(&format!(
        "SVG images nest more than {MAX_NESTING} levels deep"
    ))
}

/// `data:` URLs inside an SVG: JPEG and PNG are kept for the PDF writer,
/// nested SVGs are parsed at `at` with the same limits, other formats are
/// dropped. Everything is charged to the meter, every time.
fn resolve_data(mime: &str, data: Arc<Vec<u8>>, at: Nesting<'_>) -> Option<usvg::ImageKind> {
    // Over a limit: the SVG fails anyway, so skip the remaining copies.
    if at.meter.error.get().is_some() {
        return None;
    }
    let kind = at
        .meter
        .charge(0, data.len())
        .and_then(|()| resolve(mime, data, at));
    match kind {
        Ok(kind) => kind,
        Err(Reject::Limit(e)) => {
            let _ = at.meter.error.set(e);
            None
        }
        Err(Reject::Invalid(_)) => None,
    }
}

fn resolve(
    mime: &str,
    data: Arc<Vec<u8>>,
    at: Nesting<'_>,
) -> Result<Option<usvg::ImageKind>, Reject> {
    Ok(match embedded(mime, &data) {
        Some(Embedded::Jpeg) => raster(&data, at.meter)?.then_some(usvg::ImageKind::JPEG(data)),
        Some(Embedded::Png) => raster(&data, at.meter)?.then_some(usvg::ImageKind::PNG(data)),
        Some(Embedded::Svg) => {
            if at.level > MAX_NESTING {
                return Err(too_deep());
            }
            Some(usvg::ImageKind::SVG(parse_nested(&data, at)?.0))
        }
        None => None,
    })
}

/// Reads a raster image's header and charges its pixels. `false` leaves
/// out what the PDF writer could not decode either.
fn raster(data: &Arc<Vec<u8>>, meter: &Meter) -> Result<bool, Reject> {
    // `probe` would parse an SVG; only JPEG and PNG are drawn.
    if !(data.starts_with(b"\x89PNG") || data.starts_with(&[0xFF, 0xD8])) {
        return Ok(false);
    }
    let Ok(info) = super::probe(data) else {
        return Ok(false);
    };
    meter.charge_raster(data, u64::from(info.width) * u64::from(info.height))?;
    Ok(true)
}

/// What [`check`] found.
struct Checked {
    /// The document, each copy of its nested SVG images included.
    cost: Cost,
    /// Its own elements, nested SVG images left out.
    own: usize,
    /// Depth of the deepest image with a `data:` URL.
    images: usize,
}

/// Elements by `id`, in document order. `use` takes the first element with
/// an id and every other reference the last (as `usvg` does); the checks
/// follow all of them for those, which counts at least as much.
type Ids<'d> = HashMap<&'d str, Vec<NodeId>>;

/// Rejects input `usvg` would expand beyond the limits, and text, which
/// needs fonts this conversion does not have. `level` and `depth` place
/// the document inside the outermost SVG.
fn check(doc: &Document<'_>, level: usize, depth: usize) -> Result<Checked, Reject> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Reject::Invalid("SVG: the root element is not <svg>".into()));
    }
    let mut ids: Ids<'_> = HashMap::new();
    let mut depths: HashMap<NodeId, usize> = HashMap::new();
    let mut deepest = 1;
    let mut sheet = Drawn::default();
    let mut selectors: Vec<usize> = Vec::new();
    let (mut sheet_work, mut sheet_values) = (0usize, 0);
    for n in root.descendants().filter(Node::is_element) {
        let d = n
            .parent_element()
            .and_then(|p| depths.get(&p.id()))
            .map_or(1, |d| d + 1);
        depths.insert(n.id(), d);
        deepest = deepest.max(d);
        if let Some(id) = n.attribute("id") {
            ids.entry(id).or_default().push(n.id());
        }
        let name = n.tag_name().name();
        if is_svg_ns(n) && name == "text" && n.descendants().any(|d| has_text(&d)) {
            return Err(Reject::Invalid(
                "SVG <text> is not supported; convert the text to outlines (paths)".into(),
            ));
        }
        if name == "style" {
            let mut rest = Vec::new();
            for t in n.descendants().filter_map(|d| d.text()) {
                sheet_work = sheet_work.saturating_add(css_work(t));
                for block in t.split('}') {
                    let (selector, body) = block.split_once('{').unwrap_or(("", block));
                    selectors.extend(selector.split(',').map(combinators));
                    sheet_values += body.len();
                }
                for (p, v) in declarations(t) {
                    sheet.sort(p, v, &mut rest);
                }
            }
            // Any selector may apply them: charged to every shape.
            sheet.paint.extend(rest);
        }
    }
    sheet.resolve(doc, &ids);
    // Matching a selector may try `deepest` ancestors per combinator.
    let rule_matches = selectors.iter().fold(0usize, |sum, &k| {
        sum.saturating_add(deepest.saturating_pow(u32::try_from(k).unwrap_or(u32::MAX)))
    });
    let mut infos: HashMap<NodeId, Info<'_>> = HashMap::new();
    for n in root.descendants().filter(Node::is_element) {
        infos.insert(n.id(), Info::read(n, doc, &ids)?);
    }
    check_href_loops(&infos, &ids)?;
    let mut walk = Walk {
        doc,
        ids: &ids,
        infos: &infos,
        sheet: &sheet,
        rule_matches,
        sheet_values,
        level,
        cost: Cost::default(),
        own: 0,
        images: 0,
        nested: HashMap::new(),
        painted: HashSet::new(),
        active: Vec::new(),
    };
    walk.cost.add(Cost {
        style_work: sheet_work,
        ..Cost::default()
    })?;
    walk.visit(root, &mut Vec::new(), 0, depth, false)?;
    Ok(Checked {
        cost: walk.cost,
        own: walk.own,
        images: walk.images,
    })
}

/// Descendant and sibling combinators in a selector (child combinators
/// counted too, which only overestimates).
fn combinators(selector: &str) -> usize {
    selector
        .split(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~'))
        .filter(|part| !part.is_empty())
        .count()
        .saturating_sub(1)
}

/// Refuses `href` chains (gradients, patterns, filters) that loop: `usvg`
/// follows them until they come back to where they started, which a loop
/// further down never does.
fn check_href_loops(infos: &HashMap<NodeId, Info<'_>>, ids: &Ids<'_>) -> Result<(), Reject> {
    let next = |n: NodeId| -> &[NodeId] {
        infos
            .get(&n)
            .and_then(|i| i.chain)
            .and_then(|r| ids.get(r))
            .map_or(&[], Vec::as_slice)
    };
    // 1: on the current path, 2: done.
    let mut state: HashMap<NodeId, u8> = HashMap::new();
    for &start in infos.keys() {
        if state.contains_key(&start) {
            continue;
        }
        state.insert(start, 1);
        let mut path = vec![(start, 0usize)];
        while let Some(top) = path.last_mut() {
            let (n, i) = *top;
            let Some(&t) = next(n).get(i) else {
                state.insert(n, 2);
                path.pop();
                continue;
            };
            top.1 += 1;
            if t == n {
                // `usvg` stops at an element that names itself.
                continue;
            }
            match state.get(&t) {
                Some(1) => {
                    return Err(Reject::Invalid("SVG: `href` references form a loop".into()));
                }
                Some(_) => {}
                None => {
                    state.insert(t, 1);
                    path.push((t, 0));
                }
            }
        }
    }
    Ok(())
}

/// The cost of the SVG image in `url`, nothing for anything else.
fn nested_cost(url: &str, level: usize, depth: usize) -> Result<Cost, Reject> {
    let Ok(url) = data_url::DataUrl::process(url) else {
        return Ok(Cost::default());
    };
    let Ok((data, _)) = url.decode_to_vec() else {
        return Ok(Cost::default());
    };
    let mime = url.mime_type();
    let mime = format!("{}/{}", mime.type_, mime.subtype);
    if embedded(&mime, &data) != Some(Embedded::Svg) {
        return Ok(Cost::default());
    }
    if level > MAX_NESTING {
        return Err(too_deep());
    }
    match parse_xml(&data).and_then(|doc| check(&doc, level, depth)) {
        Ok(checked) => Ok(checked.cost),
        // Left out by the resolver too.
        Err(Reject::Invalid(_)) => Ok(Cost::default()),
        Err(e) => Err(e),
    }
}

fn is_svg_ns(n: Node<'_, '_>) -> bool {
    matches!(n.tag_name().namespace(), Some(SVG_NS) | None)
}

fn has_text(n: &Node<'_, '_>) -> bool {
    n.is_text() && n.text().is_some_and(|t| !t.trim().is_empty())
}

/// XML white space, as the SVG value parsers skip it.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// The id an `href` names (`#id`), read as `usvg` reads an IRI.
fn iri(v: &str) -> Option<&str> {
    let v = v.trim_start_matches(|c: char| c.is_ascii() && is_space(c as u8));
    let link = v.strip_prefix('#')?;
    let end = link.find(' ').unwrap_or(link.len());
    let (link, rest) = link.split_at(end);
    let rest_is_space = rest.bytes().all(is_space);
    (!link.is_empty() && rest_is_space).then_some(link)
}

/// Ids named by `url(#id)` in a value, read as `usvg` reads a `FuncIRI`
/// (spaces and quotes around the link).
fn url_refs(v: &str) -> impl Iterator<Item = &str> {
    v.split("url(").skip(1).filter_map(|rest| {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii() && is_space(c as u8));
        let (quote, rest) = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => (
                Some(q),
                rest[1..].trim_start_matches(|c: char| c.is_ascii() && is_space(c as u8)),
            ),
            _ => (None, rest),
        };
        let rest = rest.strip_prefix('#')?;
        let link = match quote {
            Some(q) => rest.split(q).next()?.trim_end(),
            None => rest.split([' ', ')']).next()?,
        };
        (!link.is_empty() && !link.contains(['"', '\''])).then_some(link)
    })
}

/// Work for the CSS parser `usvg` uses to read a `style` attribute or
/// sheet. Each time it fails to read a name, which happens at least once
/// per declaration, after every unitless number and on `#` before a digit,
/// it scans the text from its start to report a line and column: the sum
/// of those offsets. Counting every `:`, `#` and digit run overestimates.
fn css_work(css: &str) -> usize {
    let bytes = css.as_bytes();
    let fails = |&(i, &c): &(usize, &u8)| {
        matches!(c, b':' | b'#')
            || (c.is_ascii_digit() && !bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
    };
    bytes
        .iter()
        .enumerate()
        .filter(fails)
        .fold(0, |sum, (i, _)| sum.saturating_add(i))
}

/// `property: value` pairs in a style attribute or style sheet.
fn declarations(css: &str) -> impl Iterator<Item = (&str, &str)> {
    css.split([';', '{', '}']).filter_map(|d| d.split_once(':'))
}

/// Numbers in path data, a point list or a length: how many (at least one
/// per vertex `usvg` draws markers at, arcs aside), and the largest
/// magnitude (infinite for one too large for `f64`).
fn scan_numbers(v: &str) -> (usize, f64) {
    let bytes = v.as_bytes();
    let (mut count, mut largest) = (0, 0f64);
    let mut start = None;
    let (mut dot, mut exp) = (false, false);
    let mut end_number = |from: usize, to: usize, count: &mut usize| {
        *count += 1;
        let text = v[from..to].trim_end_matches(['e', 'E', '+', '-']);
        if let Ok(x) = text.parse::<f64>() {
            largest = largest.max(x.abs());
        }
    };
    for (i, &c) in bytes.iter().enumerate() {
        let in_number = start.is_some();
        match c {
            b'0'..=b'9' if !in_number => (start, dot, exp) = (Some(i), false, false),
            b'0'..=b'9' => {}
            b'.' if !in_number || dot || exp => {
                if let Some(s) = start {
                    end_number(s, i, &mut count);
                }
                (start, dot, exp) = (Some(i), true, false);
            }
            b'.' => dot = true,
            b'e' | b'E' if in_number && !exp => exp = true,
            b'+' | b'-' if in_number && matches!(bytes[i - 1], b'e' | b'E') => {}
            _ => {
                if let Some(s) = start.take() {
                    end_number(s, i, &mut count);
                }
            }
        }
    }
    if let Some(s) = start {
        end_number(s, bytes.len(), &mut count);
    }
    (count, largest)
}

fn too_large() -> Reject {
    too_complex(&format!(
        "a radius or path coordinate is larger than {MAX_COORDINATE}"
    ))
}

const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "image",
];
/// Shapes `usvg` draws markers on, and the most vertices of the basic
/// ones (path data is counted).
const MARKED: &[&str] = &[
    "path", "line", "polyline", "polygon", "rect", "circle", "ellipse",
];
const PAINT_SERVERS: &[&str] = &["linearGradient", "radialGradient", "pattern"];
/// Attributes that are not inherited; every other one is stored again on
/// each shape beneath it (an overestimate for the rest).
const NOT_INHERITED: &[&str] = &[
    "id",
    "class",
    "href",
    "d",
    "points",
    "transform",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "viewBox",
    "preserveAspectRatio",
    "offset",
    "gradientTransform",
    "patternTransform",
    "clip-path",
    "mask",
    "filter",
    "opacity",
];

/// `url(#…)` references the shapes inside an element draw.
#[derive(Default)]
struct Drawn<'d> {
    /// Paint servers and other style references, drawn once per shape.
    paint: Vec<&'d str>,
    /// Markers drawn once per path (`marker-start`, `marker-end`).
    ends: Vec<&'d str>,
    /// Markers drawn at every vertex (`marker-mid`, the `marker`
    /// shorthand and anything that might be either).
    mids: Vec<&'d str>,
}

impl<'d> Drawn<'d> {
    /// Files the references in property `prop`; others go to `rest`.
    fn sort(&mut self, prop: &str, value: &'d str, rest: &mut Vec<&'d str>) {
        let prop = prop.trim().to_ascii_lowercase();
        // The last word, after any comment.
        let name = prop.rsplit(char::is_whitespace).next().unwrap_or("");
        let list = match name {
            "fill" | "stroke" => &mut self.paint,
            "marker-start" | "marker-end" => &mut self.ends,
            _ if prop.contains("marker") => &mut self.mids,
            _ => rest,
        };
        list.extend(url_refs(value));
    }

    /// Keeps references `usvg` follows (to elements of the right kind),
    /// once each.
    fn resolve(&mut self, doc: &Document<'_>, ids: &Ids<'_>) {
        let is = |r: &&str, tags: &[&str]| {
            ids.get(r).is_some_and(|all| {
                all.iter()
                    .filter_map(|&id| doc.get_node(id))
                    .any(|n| tags.contains(&n.tag_name().name()))
            })
        };
        // Other references in styles (`clip-path`, `mask`, `filter`) are
        // kept with the paint.
        self.paint.retain(|r| ids.contains_key(r));
        self.ends.retain(|r| is(r, &["marker"]));
        self.mids.retain(|r| is(r, &["marker"]));
        for list in [&mut self.paint, &mut self.ends, &mut self.mids] {
            list.sort_unstable();
            list.dedup();
        }
    }

    fn is_empty(&self) -> bool {
        self.paint.is_empty() && self.ends.is_empty() && self.mids.is_empty()
    }
}

/// An element's references, read once however often it is copied.
#[derive(Default)]
struct Info<'d> {
    /// Followed once per copy (`clip-path`, `mask`, `filter`, `href="#…"`).
    refs: Vec<&'d str>,
    /// `use`: the copied element, which inherits from it.
    link: Option<&'d str>,
    /// Any other element's `href="#…"`, which `usvg` follows as a chain.
    chain: Option<&'d str>,
    drawn: Drawn<'d>,
    /// An `href` that is not a fragment, and whether `usvg` resolves it
    /// (`image` and `feImage`).
    url: Option<(&'d str, bool)>,
    /// Length of its attributes, `href` aside.
    parsed_bytes: usize,
    /// Length of the attributes shapes beneath it inherit.
    inherited_bytes: usize,
    /// Work to parse its `style` (see [`MAX_STYLE_WORK`]).
    style_work: usize,
    /// Most vertices it draws markers at.
    vertices: usize,
}

impl<'d> Info<'d> {
    fn read(n: Node<'d, '_>, doc: &Document<'_>, ids: &Ids<'_>) -> Result<Self, Reject> {
        let tag = n.tag_name().name();
        let mut info = Info::default();
        let mut styled = Vec::new();
        let mut vertices = 0;
        // A plain `href` wins over `xlink:href`, wherever it is.
        let mut href = None;
        for a in n.attributes() {
            let v: &'d str = a.value();
            let name = a.name();
            if name == "href" {
                match a.namespace() {
                    None => href = Some(v),
                    Some(XLINK_NS) => href = href.or(Some(v)),
                    _ => {}
                }
                continue;
            }
            info.parsed_bytes += v.len();
            if !NOT_INHERITED.contains(&name) {
                info.inherited_bytes += v.len();
            }
            match name {
                "style" => {
                    info.style_work = css_work(v);
                    for (p, value) in declarations(v) {
                        let prop = p.trim().to_ascii_lowercase();
                        if matches!(prop.as_str(), "r" | "rx" | "ry" | "d")
                            && scan_numbers(value).1 > MAX_COORDINATE
                        {
                            return Err(too_large());
                        }
                        info.drawn.sort(p, value, &mut styled);
                    }
                }
                "d" | "points" => {
                    let (numbers, largest) = scan_numbers(v);
                    if largest > MAX_COORDINATE {
                        return Err(too_large());
                    }
                    vertices += numbers;
                    if tag == "path" {
                        if v.contains(['a', 'A']) {
                            vertices += numbers.saturating_mul(ARC_VERTICES_PER_NUMBER);
                        }
                        vertices += v.bytes().filter(|c| matches!(c, b'z' | b'Z')).count();
                    }
                }
                "r" | "rx" | "ry" => {
                    if scan_numbers(v).1 > MAX_COORDINATE {
                        return Err(too_large());
                    }
                }
                _ => info.drawn.sort(name, v, &mut info.refs),
            }
        }
        if let Some(v) = href {
            match iri(v) {
                Some(r) if tag == "use" => info.link = Some(r),
                Some(r) => {
                    info.refs.push(r);
                    info.chain = Some(r);
                }
                None => info.url = Some((v, matches!(tag, "image" | "feImage"))),
            }
        }
        // Style references were always inherited, like paint.
        info.drawn.paint.extend(styled);
        info.drawn.resolve(doc, ids);
        info.refs.retain(|r| ids.contains_key(r));
        info.vertices = match tag {
            "path" | "line" | "polyline" | "polygon" => vertices + 2,
            "rect" | "circle" | "ellipse" => SHAPE_VERTICES,
            _ => 0,
        };
        Ok(info)
    }
}

struct Walk<'d, 'i, 'w> {
    doc: &'d Document<'i>,
    ids: &'w Ids<'d>,
    infos: &'w HashMap<NodeId, Info<'d>>,
    /// References in style sheets.
    sheet: &'w Drawn<'d>,
    /// Style sheet matching work per element.
    rule_matches: usize,
    /// Declaration text of the style sheets, copied onto the elements
    /// rules match (charged to each).
    sheet_values: usize,
    /// SVG images around this document.
    level: usize,
    cost: Cost,
    own: usize,
    images: usize,
    /// Cost of the nested SVG in each image element.
    nested: HashMap<NodeId, Cost>,
    /// Paint servers drawn so far: `usvg` converts each once, and copies
    /// share its images and path data.
    painted: HashSet<NodeId>,
    /// Elements being expanded (reference cycles are skipped, as `usvg`
    /// does for `use`, clip paths, masks and filters; `href` chains that
    /// loop are refused before).
    active: Vec<NodeId>,
}

impl<'d, 'i, 'w> Walk<'d, 'i, 'w> {
    /// Counts `n` and everything it pulls in. `chain` holds what its
    /// ancestors pass on to shapes and `inherited` the length of their
    /// inherited attributes; `shared` is set inside a paint server drawn
    /// before.
    fn visit(
        &mut self,
        n: Node<'d, 'i>,
        chain: &mut Vec<&'w Drawn<'d>>,
        inherited: usize,
        depth: usize,
        shared: bool,
    ) -> Result<(), Reject> {
        if depth > MAX_DEPTH {
            return Err(Reject::Limit(format!(
                "SVG nests deeper than {MAX_DEPTH} levels"
            )));
        }
        // Charged even when skipped, so that following references is
        // never free.
        self.cost.add(Cost {
            elements: 1,
            ..Cost::default()
        })?;
        let infos = self.infos;
        let Some(info) = infos.get(&n.id()) else {
            return Ok(());
        };
        if self.active.contains(&n.id()) {
            return Ok(());
        }
        self.active.push(n.id());
        self.own += 1;
        let tag = n.tag_name().name();
        let shape = SHAPES.contains(&tag);
        if !shared {
            self.cost.add(Cost {
                parsed_bytes: info
                    .parsed_bytes
                    .saturating_add(if shape { inherited } else { 0 }),
                copied_bytes: self.sheet_values,
                style_work: info.style_work,
                rule_matches: self.rule_matches,
                ..Cost::default()
            })?;
        }
        if let Some((url, resolved)) = info.url {
            let mut cost = Cost::default();
            if resolved {
                self.images = self.images.max(depth);
                cost = self.nested(n.id(), url, depth)?;
            }
            cost.data_bytes = cost.data_bytes.saturating_add(url.len());
            if shared {
                // Converted once; only the tree is copied.
                cost = Cost {
                    elements: cost.elements,
                    ..Cost::default()
                };
            }
            self.cost.add(cost)?;
        }
        for r in &info.refs {
            self.follow(r, false, &mut Vec::new(), 0, depth, shared)?;
        }
        let pushed = !info.drawn.is_empty();
        if pushed {
            chain.push(&info.drawn);
        }
        let inherited = inherited.saturating_add(info.inherited_bytes);
        if let Some(r) = info.link {
            self.follow(r, true, chain, inherited, depth, shared)?;
        }
        if shape {
            for i in 0..=chain.len() {
                let drawn = chain.get(i).copied().unwrap_or(self.sheet);
                for r in &drawn.paint {
                    for t in self.targets(r, false) {
                        let painted = self.doc.get_node(t).is_some_and(|p| {
                            PAINT_SERVERS.contains(&p.tag_name().name()) && !self.painted.insert(t)
                        });
                        self.visit_target(t, &mut Vec::new(), 0, depth, shared || painted)?;
                    }
                }
                if MARKED.contains(&tag) {
                    for r in &drawn.ends {
                        self.follow(r, false, &mut Vec::new(), 0, depth, shared)?;
                    }
                    for r in &drawn.mids {
                        for _ in 0..info.vertices {
                            self.follow(r, false, &mut Vec::new(), 0, depth, shared)?;
                        }
                    }
                }
            }
        }
        for c in n.children().filter(Node::is_element) {
            self.visit(c, chain, inherited, depth + 1, shared)?;
        }
        if pushed {
            chain.pop();
        }
        self.active.pop();
        Ok(())
    }

    /// The elements reference `r` may name: the first for `use`, all of
    /// them otherwise.
    fn targets(&self, r: &str, first: bool) -> Vec<NodeId> {
        match self.ids.get(r) {
            Some(all) if first => all.first().copied().into_iter().collect(),
            Some(all) => all.clone(),
            None => Vec::new(),
        }
    }

    fn follow(
        &mut self,
        r: &str,
        first: bool,
        chain: &mut Vec<&'w Drawn<'d>>,
        inherited: usize,
        depth: usize,
        shared: bool,
    ) -> Result<(), Reject> {
        for t in self.targets(r, first) {
            self.visit_target(t, chain, inherited, depth, shared)?;
        }
        Ok(())
    }

    fn visit_target(
        &mut self,
        t: NodeId,
        chain: &mut Vec<&'w Drawn<'d>>,
        inherited: usize,
        depth: usize,
        shared: bool,
    ) -> Result<(), Reject> {
        match self.doc.get_node(t) {
            Some(t) => self.visit(t, chain, inherited, depth + 1, shared),
            None => Ok(()),
        }
    }

    /// The cost of one copy of the nested SVG in image `id`, checked once.
    fn nested(&mut self, id: NodeId, url: &str, depth: usize) -> Result<Cost, Reject> {
        if let Some(&cost) = self.nested.get(&id) {
            return Ok(cost);
        }
        let cost = nested_cost(url, self.level + 1, depth + 1)?;
        self.nested.insert(id, cost);
        Ok(cost)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">{body}</svg>"#)
    }

    /// The error for `doc` (`unwrap_err` would print a huge tree).
    fn rejected(doc: &str) -> String {
        match parse(doc.as_bytes()) {
            Ok(_) => panic!("accepted {}", &doc[doc.len().saturating_sub(120)..]),
            Err(e) => e,
        }
    }

    #[test]
    fn parses_and_sizes() {
        let t = parse(svg(r#"<rect width="5" height="5" fill="red"/>"#).as_bytes())
            .unwrap()
            .0;
        assert_eq!((t.size().width(), t.size().height()), (20.0, 10.0));
        assert!(is_svg(b"\xEF\xBB\xBF  <?xml version=\"1.0\"?><svg/>"));
        assert!(!is_svg(b"\x89PNG"));
    }

    #[test]
    fn rejects_text_and_garbage() {
        let e = parse(svg("<text x='1' y='5'>Hi</text>").as_bytes()).unwrap_err();
        assert!(e.contains("<text>"), "{e}");
        // Empty text elements are harmless.
        parse(svg("<text x='1' y='5'> </text>").as_bytes()).unwrap();
        assert!(parse(b"<svg").is_err());
        assert!(parse(b"<html/>").is_err());
        assert!(parse(&[0xFF, 0xFE, 0x3C]).is_err());
    }

    #[test]
    fn rejects_reference_bombs() {
        // Ten levels of ten `use`s: 10^10 elements once expanded.
        let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
        for i in 1..=10 {
            let _ = write!(body, r#"<g id="l{i}">"#);
            for _ in 0..10 {
                let _ = write!(body, r##"<use href="#l{}"/>"##, i - 1);
            }
            body.push_str("</g>");
        }
        body.push_str(r##"</defs><use href="#l10"/>"##);
        let e = parse(svg(&body).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");

        // Self-reference is not a bomb.
        parse(svg(r##"<g id="a"><use href="#a"/></g>"##).as_bytes()).unwrap();

        let mut deep = String::new();
        for _ in 0..(MAX_DEPTH + 5) {
            deep.push_str("<g>");
        }
        for _ in 0..(MAX_DEPTH + 5) {
            deep.push_str("</g>");
        }
        let e = parse(svg(&deep).as_bytes()).unwrap_err();
        assert!(e.contains("deeper"), "{e}");
    }

    #[test]
    fn a_view_box_fits_the_box_it_fills_like_css() {
        let view_box = |attrs: &str| {
            let doc = format!(r#"<svg xmlns="{SVG_NS}" {attrs}/>"#);
            ViewBox::of(Document::parse(&doc).unwrap().root_element())
        };
        assert_eq!(view_box(r#"width="10" height="10""#), None);
        assert_eq!(view_box(r#"viewBox="0 0 0 10""#), None);
        let vb = view_box(r#"viewBox="0,0 100 100""#).unwrap();
        assert_eq!((vb.align, vb.slice), (Some((0.5, 0.5)), false));
        let bx = Rect::new(10.0, 20.0, 100.0, 40.0);
        let near = |(r, clip): (Rect, bool), want: [f32; 4], clipped: bool| {
            let got = [r.x, r.y, r.w, r.h];
            let close = got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3);
            assert!(close && clip == clipped, "{got:?} {clip}, want {want:?}");
        };
        // A square drawing in a wide box: a 40 pt square, centred.
        near(
            vb.place((100.0, 100.0), bx),
            [40.0, 20.0, 40.0, 40.0],
            false,
        );
        let place = |attrs: &str, size| view_box(attrs).unwrap().place(size, bx);
        near(
            place(
                r#"viewBox="0 0 100 100" preserveAspectRatio="xMinYMax""#,
                (100.0, 100.0),
            ),
            [10.0, 20.0, 40.0, 40.0],
            false,
        );
        near(
            place(
                r#"viewBox="0 0 100 100" preserveAspectRatio="none""#,
                (100.0, 100.0),
            ),
            [10.0, 20.0, 100.0, 40.0],
            false,
        );
        // slice covers the box: 100 × 100, the middle 40 shown.
        near(
            place(
                r#"viewBox="0 0 100 100" preserveAspectRatio="defer xMidYMid slice""#,
                (100.0, 100.0),
            ),
            [10.0, -10.0, 100.0, 100.0],
            true,
        );
        // An SVG whose size is wider than its viewBox: the viewBox, not the
        // size, fits the box.
        near(
            place(r#"viewBox="0 0 100 100""#, (200.0, 100.0)),
            [20.0, 20.0, 80.0, 40.0],
            false,
        );
        // An invalid value is the default.
        near(
            place(
                r#"viewBox="0 0 100 100" preserveAspectRatio="xFooYMin""#,
                (100.0, 100.0),
            ),
            [40.0, 20.0, 40.0, 40.0],
            false,
        );
    }

    #[test]
    fn ignores_files() {
        let t =
            parse(svg(r#"<image href="/etc/passwd" width="5" height="5"/>"#).as_bytes()).unwrap();
        assert!(t.0.root().children().is_empty());
    }

    /// `doc` as a percent-encoded `data:` URL, which nests without escaping.
    fn data_url(doc: &str) -> String {
        url_of("image/svg+xml", doc.as_bytes())
    }

    /// `bytes` as a percent-encoded `data:` URL.
    fn url_of(mime: &str, bytes: &[u8]) -> String {
        let mut url = format!("data:{mime},");
        for &b in bytes {
            if b.is_ascii_alphanumeric() {
                url.push(char::from(b));
            } else {
                let _ = write!(url, "%{b:02X}");
            }
        }
        url
    }

    /// An SVG drawing `copies` copies of `inner` as an `<image>`, inside
    /// `groups` nested groups.
    fn image_of(inner: &str, copies: usize, groups: usize) -> String {
        let mut body = format!(
            r#"<defs><image id="i" width="4" height="4" href="{}"/></defs>"#,
            data_url(inner)
        );
        body.push_str(&"<g>".repeat(groups));
        body.push_str(&r##"<use href="#i"/>"##.repeat(copies));
        body.push_str(&"</g>".repeat(groups));
        svg(&body)
    }

    /// `levels` of `<use>` bombs: about `10^levels` elements once expanded.
    fn bomb(levels: usize) -> String {
        let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
        for i in 1..=levels {
            let _ = write!(body, r#"<g id="l{i}">"#);
            body.push_str(&format!(r##"<use href="#l{}"/>"##, i - 1).repeat(10));
            body.push_str("</g>");
        }
        let _ = write!(body, r##"</defs><use href="#l{levels}"/>"##);
        svg(&body)
    }

    /// Nested SVG images: (copies at the top level, deepest level).
    fn nested_images(g: &usvg::Group) -> (usize, usize) {
        let (mut copies, mut deepest) = (0, 0);
        for n in g.children() {
            match n {
                usvg::Node::Group(g) => {
                    let (c, d) = nested_images(g);
                    copies += c;
                    deepest = deepest.max(d);
                }
                usvg::Node::Image(img) => {
                    if let usvg::ImageKind::SVG(t) = img.kind() {
                        copies += 1;
                        deepest = deepest.max(1 + nested_images(t.root()).1);
                    }
                }
                _ => {}
            }
        }
        (copies, deepest)
    }

    #[test]
    fn nested_svgs_share_the_budget() {
        // Each copy of a nested SVG is parsed again: a thousand copies of
        // 47k elements must not get a fresh budget each.
        let inner = bomb(4);
        let t = std::time::Instant::now();
        let e = parse(image_of(&inner, 1000, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());

        // Nor each level: 50 copies of 50 copies of 4.7k elements.
        let mid = image_of(&bomb(3), 50, 0);
        let e = parse(image_of(&mid, 50, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");

        // A few copies fit.
        let t = parse(image_of(&inner, 3, 0).as_bytes()).unwrap().0;
        assert_eq!(nested_images(t.root()), (3, 1));
    }

    #[test]
    fn caps_svg_in_svg_nesting() {
        let chain = |levels: usize, groups: usize| {
            let mut doc = svg(r#"<rect width="5" height="5"/>"#);
            for _ in 0..levels {
                doc = image_of(&doc, 1, groups);
            }
            doc
        };
        let e = parse(chain(MAX_NESTING + 1, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex") && e.contains("nest"), "{e}");

        // Element depth adds up across levels: each level is its groups,
        // root, `use` and image deep, and the innermost has a rect.
        let groups = (MAX_DEPTH - 1) / MAX_NESTING - 3;
        let e = parse(chain(MAX_NESTING, groups + 1).as_bytes()).unwrap_err();
        assert!(e.contains("deeper"), "{e}");

        // The deepest accepted chain converts on a small stack.
        let doc = chain(MAX_NESTING, groups);
        let t = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || parse(doc.as_bytes()).unwrap().0)
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(nested_images(t.root()).1, MAX_NESTING);
    }

    #[test]
    fn limits_embedded_image_bytes() {
        // `usvg` decodes the URL again for every copy.
        let png = format!("data:image/png;base64,{}", "A".repeat(1 << 20));
        let body = format!(r#"<defs><image id="i" width="4" height="4" href="{png}"/></defs>"#);
        let e = parse(svg(&(body.clone() + &r##"<use href="#i"/>"##.repeat(200))).as_bytes())
            .unwrap_err();
        assert!(e.contains("MiB"), "{e}");
        parse(svg(&(body + &r##"<use href="#i"/>"##.repeat(2))).as_bytes()).unwrap();
    }

    #[test]
    fn counts_markers_per_vertex() {
        let marker = |content: &str| format!(r#"<defs><marker id="m">{content}</marker></defs>"#);
        let dot = marker(r#"<circle r="1"/>"#);
        let long = format!("M0 0{}", "h1".repeat(200_000));
        let t = std::time::Instant::now();
        // A dot at each of 200k vertices, however the marker is set.
        for shape in [
            format!(r#"<path d="{long}" marker-mid="url(#m)"/>"#),
            format!(r#"<path d="{long}" style="MARKER-MID: url(#m)"/>"#),
            format!(r#"<path d="{long}" style="/**/marker: url(#m)"/>"#),
            format!(r#"<g marker-mid="url(#m)"><path d="{long}"/></g>"#),
            format!(r#"<style>path {{ marker-mid: url(#m) }}</style><path d="{long}"/>"#),
            format!(r##"<path id="p" d="{long}"/><use href="#p" marker-mid="url(#m)"/>"##),
        ] {
            let e = rejected(&svg(&(dot.clone() + &shape)));
            assert!(e.contains("too complex"), "{shape:.60}: {e}");
        }
        // An embedded image in a marker is decoded at every vertex.
        let png = format!("data:image/png;base64,{}", "A".repeat(1 << 20));
        let image = marker(&format!(r#"<image width="1" height="1" href="{png}"/>"#));
        let path = format!(
            r#"<path d="M0 0{}" marker-mid="url(#m)"/>"#,
            "h1".repeat(200)
        );
        let e = rejected(&svg(&(image + &path)));
        assert!(e.contains("MiB"), "{e}");
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());

        // Arrowheads at the ends of a long path, dots along a short line.
        let arrows = format!(r#"<path d="{long}" marker-start="url(#m)" marker-end="url(#m)"/>"#);
        parse(svg(&(dot.clone() + &arrows)).as_bytes()).unwrap();
        let dots = format!(
            r#"<polyline points="{}" marker-mid="url(#m)"/>"#,
            "1,1 ".repeat(100)
        );
        parse(svg(&(dot + &dots)).as_bytes()).unwrap();
    }

    /// A PNG header declaring `w`×`h` pixels, made distinct by `salt`.
    fn png_url(w: u32, h: u32, salt: u8) -> String {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(w.to_be_bytes());
        png.extend(h.to_be_bytes());
        png.extend([8, 2, 0, 0, 0, salt, 0, 0, 0]);
        url_of("image/png", &png)
    }

    #[test]
    fn limits_embedded_pixels() {
        let image = |salt| {
            let url = png_url(7000, 7000, salt);
            format!(r#"<image id="i{salt}" width="1" height="1" href="{url}"/>"#)
        };
        // An image is decoded once, however many copies draw it.
        let copies = r##"<use href="#i0"/>"##.repeat(1000);
        parse(svg(&format!("<defs>{}</defs>{copies}", image(0))).as_bytes()).unwrap();
        // Five 49-megapixel images are too many.
        let five: String = (0..5).map(image).collect();
        let e = rejected(&svg(&five));
        assert!(e.contains("megapixels"), "{e}");
    }

    #[test]
    fn limits_path_data() {
        // `usvg` parses path data again for every copy.
        let d = format!("M0 0{}", "h1".repeat(1 << 17));
        let body = format!(r#"<defs><path id="p" d="{d}"/></defs>"#);
        let e = rejected(&svg(&(body.clone() + &r##"<use href="#p"/>"##.repeat(130))));
        assert!(e.contains("path data"), "{e}");
        parse(svg(&(body + &r##"<use href="#p"/>"##.repeat(2))).as_bytes()).unwrap();
    }

    #[test]
    fn charges_shared_paint_servers_once() {
        let pattern = |href: &str| {
            format!(
                r#"<defs><pattern id="p" width="1" height="1"><image width="1" height="1" href="{href}"/></pattern></defs>"#
            )
        };
        let rects = |n| r#"<rect width="5" height="5" fill="url(#p)"/>"#.repeat(n);
        // `usvg` converts a pattern once: its image is decoded once.
        let mut png = crate::test_fixtures::fixture("images/logo.png");
        png.resize(1 << 20, 0);
        let png = url_of("image/png", &png);
        parse(svg(&(pattern(&png) + &rects(200))).as_bytes()).unwrap();
        // Its content is still copied for every shape.
        let e = rejected(&svg(&(pattern(&data_url(&bomb(4))) + &rects(10))));
        assert!(e.contains("too complex"), "{e}");
    }

    #[test]
    fn limits_styles() {
        // The CSS parser scans a style from its start for every
        // declaration, and `usvg` parses a copied element's style again.
        let t = std::time::Instant::now();
        let long = "fill:none;".repeat(100_000);
        let e = rejected(&svg(&format!(r#"<g style="{long}"/>"#)));
        assert!(e.contains("too long"), "{e}");
        // It also scans after every unitless number, in sheets too.
        let dashes = format!("stroke-dasharray:{}", "1 ".repeat(40_000));
        let e = rejected(&svg(&format!(r#"<rect style="{dashes}"/>"#)));
        assert!(e.contains("too long"), "{e}");
        let e = rejected(&svg(&format!("<style>rect{{{dashes}}}</style><rect/>")));
        assert!(e.contains("too long"), "{e}");
        let dashes = format!("stroke-dasharray:{}", "1 ".repeat(1_000));
        parse(svg(&format!(r#"<rect style="{dashes}"/>"#)).as_bytes()).unwrap();
        let style = "fill:none;".repeat(100);
        let doc = |copies| {
            let uses = r##"<use href="#a"/>"##.repeat(copies);
            svg(&format!(
                r#"<defs><g id="a" style="{style}"/></defs>{uses}"#
            ))
        };
        let e = rejected(&doc(30_000));
        assert!(e.contains("too long"), "{e}");
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
        parse(doc(100).as_bytes()).unwrap();

        // Every style sheet rule is matched against each copy.
        let sheet = format!("<style>{}</style>", ".a{}".repeat(2_000));
        let uses = r##"<use href="#r"/>"##.repeat(30_000);
        let e = rejected(&svg(&format!(
            r#"{sheet}<defs><rect id="r"/></defs>{uses}"#
        )));
        assert!(e.contains("rule matches"), "{e}");
        parse(svg(&format!("{sheet}<rect/>")).as_bytes()).unwrap();

        // Declarations are copied onto every element they match.
        let sheet = format!("<style>{}</style>", ".a{fill:red}".repeat(2_000));
        let e = rejected(&svg(&format!(
            r#"{sheet}<defs><rect id="r"/></defs>{uses}"#
        )));
        assert!(e.contains("copy more than"), "{e}");
    }

    #[test]
    fn weighs_selectors_by_their_combinators() {
        // Each descendant combinator may try every ancestor.
        let groups = 60;
        let deep = format!("{}<rect/>{}", "<g>".repeat(groups), "</g>".repeat(groups));
        let selector = vec!["*"; 30].join(" ");
        let t = std::time::Instant::now();
        let e = rejected(&svg(&format!("<style>{selector} {{}}</style>{deep}")));
        assert!(e.contains("rule matches"), "{e}");
        // A selector list is one rule per selector.
        let list = vec!["g rect"; 20_000].join(",");
        let flat = "<rect/>".repeat(3_000);
        let e = rejected(&svg(&format!("<style>{list} {{}}</style>{flat}")));
        assert!(e.contains("rule matches"), "{e}");
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
        // Ordinary sheets fit.
        let sheet = "<style>.st0{fill:#fff} svg g .st1{stroke:red} #a > path{fill:none}</style>";
        parse(svg(&format!("{sheet}{deep}{flat}")).as_bytes()).unwrap();
    }

    #[test]
    fn refuses_huge_radii() {
        for shape in [
            r#"<path d="M0 0A1e60 1e60 0 1 1 1e60 0"/>"#,
            r#"<path d="M0 0a1e30,1e30 0 1,1 1,0"/>"#,
            r#"<circle r="3e38"/>"#,
            r#"<ellipse rx="1" ry="1e300"/>"#,
            r#"<rect width="5" height="5" rx="1e9"/>"#,
            r#"<circle style="r: 1e20"/>"#,
        ] {
            let t = std::time::Instant::now();
            let e = rejected(&svg(shape));
            assert!(e.contains("larger than"), "{shape}: {e}");
            assert!(t.elapsed().as_secs() < 2, "{shape} took {:?}", t.elapsed());
        }
        parse(svg(r#"<path d="M0 0A5 5 0 1 1 10 0"/><circle r="4"/>"#).as_bytes()).unwrap();
    }

    #[test]
    fn counts_markers_on_every_shape() {
        let marker = r#"<defs><marker id="m"><path d="M0 0h1"/></marker></defs>"#;
        let shapes = r#"<circle r="2" marker-mid="url(#m)"/>"#.repeat(2_000);
        let e = rejected(&svg(&format!("{marker}{shapes}")));
        assert!(e.contains("too complex"), "{e}");
        // Arcs in path data become several curves, each with a vertex.
        let arcs = format!(
            r#"<path d="M0 0{}" marker-mid="url(#m)"/>"#,
            "A1 1 0 0 1 1 1".repeat(3_000)
        );
        let e = rejected(&svg(&format!("{marker}{arcs}")));
        assert!(e.contains("too complex"), "{e}");
        let few = r#"<circle r="2" marker-mid="url(#m)"/>"#.repeat(10);
        parse(svg(&format!("{marker}{few}")).as_bytes()).unwrap();
    }

    #[test]
    fn follows_links_as_usvg_reads_them() {
        // Spaces around a link, quotes in `url()`, and a plain `href` that
        // wins over `xlink:href` all reach the referenced element.
        let bomb = bomb(5);
        let body = &bomb[bomb.find("<defs>").unwrap()..bomb.rfind("<use").unwrap()];
        for user in [
            r#"<use href=" #l5 "/>"#,
            r##"<use xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="#l0" href="#l5"/>"##,
            r#"<rect width="1" height="1" fill="url( '#p ' )"/>"#,
        ] {
            let pattern = r##"<pattern id="p" width="1" height="1"><use href="#l5"/></pattern>"##;
            let doc = svg(&format!("{body}<defs>{pattern}</defs>{}", user.repeat(3)));
            let e = rejected(&doc);
            assert!(e.contains("too complex"), "{user}: {e}");
        }
        // The last of two elements with one id is followed too.
        let doc = svg(&format!(
            r##"{body}<defs><marker id="m"/><marker id="m"><use href="#l5"/></marker></defs><path d="M0 0h1h1" marker-mid="url(#m)"/>"##
        ));
        let e = rejected(&doc);
        assert!(e.contains("too complex"), "{e}");
    }

    #[test]
    fn refuses_href_loops() {
        let e = rejected(&svg(
            r##"<defs><linearGradient id="a" href="#b"/><linearGradient id="b" href="#c"/><linearGradient id="c" href="#b"/></defs><rect width="5" height="5" fill="url(#a)"/>"##,
        ));
        assert!(e.contains("loop"), "{e}");
        // A chain that ends, or an element naming itself, is fine.
        parse(
            svg(r##"<defs><linearGradient id="a" href="#b"/><linearGradient id="b" href="#b"><stop offset="0"/></linearGradient></defs><rect width="5" height="5" fill="url(#a)"/>"##)
                .as_bytes(),
        )
        .unwrap();
    }

    #[test]
    fn limits_entities() {
        let entity = |value: &str, uses: usize| {
            format!(
                r#"<!DOCTYPE svg [<!ENTITY e "{value}">]><svg xmlns="http://www.w3.org/2000/svg" width="5" height="5"><g class="{}"/></svg>"#,
                "&e;".repeat(uses)
            )
        };
        let e = rejected(&entity(&"x".repeat(1024), 2_000));
        assert!(e.contains("entities expand"), "{e}");
        let e = rejected(&entity("&amp;&amp;", 1));
        assert!(e.contains("other entities"), "{e}");
        // The namespace entities of drawing programs.
        let illustrator = r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [
            <!ENTITY ns_svg "http://www.w3.org/2000/svg">
            <!ENTITY ns_xlink "http://www.w3.org/1999/xlink">
        ]><svg xmlns="&ns_svg;" xmlns:xlink="&ns_xlink;" width="5" height="5"><rect width="5" height="5"/></svg>"#;
        parse(illustrator.as_bytes()).unwrap();
    }

    #[test]
    fn charges_inherited_attributes_per_shape() {
        // An inherited attribute is stored again on every shape.
        let dashes = "1 ".repeat(100_000);
        let rects = r#"<rect width="1" height="1"/>"#.repeat(300);
        let e = rejected(&svg(&format!(
            r#"<g stroke="red" stroke-dasharray="{dashes}">{rects}</g>"#
        )));
        assert!(e.contains("attributes"), "{e}");
        parse(
            svg(&format!(
                r#"<g stroke="red" stroke-dasharray="1 2">{rects}</g>"#
            ))
            .as_bytes(),
        )
        .unwrap();
    }
}
