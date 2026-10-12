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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use usvg::roxmltree::{self, Document, Node, NodeId};

/// Most elements the converted tree may contain, counting every copy made
/// by `use`, markers and `url(#…)` references, nested SVG images included.
const MAX_EXPANDED: usize = 250_000;
/// Most `data:` URL bytes decoded, counting every copy.
const MAX_DATA_BYTES: usize = 128 << 20;
/// Most path data (`d`, `points`) and `style` attributes parsed,
/// counting every copy: `usvg` parses them again for each.
const MAX_PARSED_BYTES: usize = 32 << 20;
/// Most style parsing work, in declarations × bytes for each `style`
/// attribute and sheet, counting every copy: the CSS parser `usvg` uses
/// scans the text from its start again for each declaration.
const MAX_STYLE_WORK: usize = 2_000_000_000;
/// Most style sheet rules matched, counting every copy: `usvg` matches
/// every rule against each element it copies.
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
    style_work: usize,
    rule_matches: usize,
}

impl Cost {
    fn add(&mut self, other: Cost) -> Result<(), Reject> {
        self.elements = self.elements.saturating_add(other.elements);
        self.data_bytes = self.data_bytes.saturating_add(other.data_bytes);
        self.parsed_bytes = self.parsed_bytes.saturating_add(other.parsed_bytes);
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
                "more than {} MiB of path data and styles once references are expanded",
                MAX_PARSED_BYTES >> 20
            )));
        }
        if self.style_work > MAX_STYLE_WORK {
            return Err(too_complex(
                "style attributes or sheets are too long once references are expanded",
            ));
        }
        if self.rule_matches > MAX_RULE_MATCHES {
            return Err(too_complex(&format!(
                "more than {MAX_RULE_MATCHES} style rule matches once references are expanded"
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

/// Parses an SVG document into a `usvg` tree.
pub(crate) fn parse(bytes: &[u8]) -> Result<usvg::Tree, String> {
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
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        entity_resolver: None,
    };
    Document::parse_with_options(text, opts).map_err(|e| Reject::Invalid(format!("SVG: {e}")))
}

fn parse_nested(bytes: &[u8], at: Nesting<'_>) -> Result<usvg::Tree, Reject> {
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
    Ok(tree)
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
            Some(usvg::ImageKind::SVG(parse_nested(&data, at)?))
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

/// Rejects input `usvg` would expand beyond the limits, and text, which
/// needs fonts this conversion does not have. `level` and `depth` place
/// the document inside the outermost SVG.
fn check(doc: &Document<'_>, level: usize, depth: usize) -> Result<Checked, Reject> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Reject::Invalid("SVG: the root element is not <svg>".into()));
    }
    let mut ids: HashMap<&str, NodeId> = HashMap::new();
    let mut sheet = Drawn::default();
    let (mut rules, mut sheet_len, mut sheet_declarations) = (0, 0, 0);
    for n in root.descendants().filter(Node::is_element) {
        if let Some(id) = n.attribute("id") {
            ids.entry(id).or_insert(n.id());
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
                rules += t.matches('{').count();
                sheet_len += t.len();
                sheet_declarations += t.matches(':').count();
                for (p, v) in declarations(t) {
                    sheet.sort(p, v, &mut rest);
                }
            }
            // Any selector may apply them: charged to every shape.
            sheet.paint.extend(rest);
        }
    }
    sheet.resolve(doc, &ids);
    let infos: HashMap<NodeId, Info<'_>> = root
        .descendants()
        .filter(Node::is_element)
        .map(|n| (n.id(), Info::read(n, doc, &ids)))
        .collect();
    let mut walk = Walk {
        doc,
        ids: &ids,
        infos: &infos,
        sheet: &sheet,
        rules,
        level,
        cost: Cost::default(),
        own: 0,
        images: 0,
        nested: HashMap::new(),
        painted: HashSet::new(),
        active: Vec::new(),
    };
    walk.cost.add(Cost {
        style_work: sheet_len.saturating_mul(sheet_declarations),
        ..Cost::default()
    })?;
    walk.visit(root, &mut Vec::new(), depth, false)?;
    Ok(Checked {
        cost: walk.cost,
        own: walk.own,
        images: walk.images,
    })
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

/// Ids named by `url(#id)` in a value.
fn url_refs(v: &str) -> impl Iterator<Item = &str> {
    v.split("url(").skip(1).filter_map(|rest| {
        let inner = rest.split(')').next()?;
        let inner = inner.trim().trim_matches(|c| c == '"' || c == '\'');
        inner.strip_prefix('#')
    })
}

/// `property: value` pairs in a style attribute or style sheet.
fn declarations(css: &str) -> impl Iterator<Item = (&str, &str)> {
    css.split([';', '{', '}']).filter_map(|d| d.split_once(':'))
}

/// Numbers in path data or a point list: at least one per vertex `usvg`
/// draws markers at (an arc of five or more becomes at most four curves).
fn count_numbers(v: &str) -> usize {
    let (mut count, mut number, mut dot, mut exp) = (0, false, false, false);
    let mut prev = b' ';
    for &c in v.as_bytes() {
        match c {
            b'0'..=b'9' if !number => {
                (count, number, dot, exp) = (count + 1, true, false, false);
            }
            b'0'..=b'9' => {}
            b'.' if !number || dot || exp => {
                (count, number, dot, exp) = (count + 1, true, true, false);
            }
            b'.' => dot = true,
            b'e' | b'E' if number && !exp => exp = true,
            b'+' | b'-' if number && matches!(prev, b'e' | b'E') => {}
            _ => number = false,
        }
        prev = c;
    }
    count
}

const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "image",
];
/// Shapes that draw markers.
const MARKED: &[&str] = &["path", "line", "polyline", "polygon"];
const PAINT_SERVERS: &[&str] = &["linearGradient", "radialGradient", "pattern"];

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
    fn resolve(&mut self, doc: &Document<'_>, ids: &HashMap<&str, NodeId>) {
        let is = |r: &&str, tags: &[&str]| {
            ids.get(r)
                .and_then(|&id| doc.get_node(id))
                .is_some_and(|n| tags.contains(&n.tag_name().name()))
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
    drawn: Drawn<'d>,
    /// An `href` that is not a fragment, and whether `usvg` resolves it
    /// (`image` and `feImage`).
    url: Option<(&'d str, bool)>,
    /// Length of its path data and `style`.
    parsed_bytes: usize,
    /// Work to parse its `style` (see [`MAX_STYLE_WORK`]).
    style_work: usize,
    /// Most vertices it draws markers at.
    vertices: usize,
}

impl<'d> Info<'d> {
    fn read(n: Node<'d, '_>, doc: &Document<'_>, ids: &HashMap<&str, NodeId>) -> Self {
        let tag = n.tag_name().name();
        let mut info = Info::default();
        let mut styled = Vec::new();
        let mut vertices = 0;
        for a in n.attributes() {
            let v: &'d str = a.value();
            match a.name() {
                "style" => {
                    info.parsed_bytes += v.len();
                    info.style_work = v.len().saturating_mul(v.matches(':').count());
                    for (p, value) in declarations(v) {
                        info.drawn.sort(p, value, &mut styled);
                    }
                }
                "href" if matches!(a.namespace(), None | Some(XLINK_NS)) => {
                    match v.strip_prefix('#') {
                        Some(r) if tag == "use" => info.link = Some(r),
                        Some(r) => info.refs.push(r),
                        None => info.url = Some((v, matches!(tag, "image" | "feImage"))),
                    }
                }
                "d" | "points" => {
                    info.parsed_bytes += v.len();
                    vertices += count_numbers(v);
                    if tag == "path" {
                        vertices += v.bytes().filter(|c| matches!(c, b'z' | b'Z')).count();
                    }
                }
                name => info.drawn.sort(name, v, &mut info.refs),
            }
        }
        // Style references were always inherited, like paint.
        info.drawn.paint.extend(styled);
        info.drawn.resolve(doc, ids);
        info.refs.retain(|r| ids.contains_key(r));
        info.vertices = if MARKED.contains(&tag) {
            vertices + 2
        } else {
            0
        };
        info
    }
}

struct Walk<'d, 'i, 'w> {
    doc: &'d Document<'i>,
    ids: &'w HashMap<&'d str, NodeId>,
    infos: &'w HashMap<NodeId, Info<'d>>,
    /// References in style sheets.
    sheet: &'w Drawn<'d>,
    /// Style sheet rules, matched against every element.
    rules: usize,
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
    /// does).
    active: Vec<NodeId>,
}

impl<'d, 'i, 'w> Walk<'d, 'i, 'w> {
    /// Counts `n` and everything it pulls in. `chain` holds what its
    /// ancestors pass on to shapes; `shared` is set inside a paint server
    /// drawn before.
    fn visit(
        &mut self,
        n: Node<'d, 'i>,
        chain: &mut Vec<&'w Drawn<'d>>,
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
        if !shared {
            self.cost.add(Cost {
                parsed_bytes: info.parsed_bytes,
                style_work: info.style_work,
                rule_matches: self.rules,
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
                cost.data_bytes = 0;
                cost.parsed_bytes = 0;
                cost.style_work = 0;
                cost.rule_matches = 0;
            }
            self.cost.add(cost)?;
        }
        for r in &info.refs {
            self.follow(r, &mut Vec::new(), depth, shared)?;
        }
        let pushed = !info.drawn.is_empty();
        if pushed {
            chain.push(&info.drawn);
        }
        if let Some(r) = info.link {
            self.follow(r, chain, depth, shared)?;
        }
        let tag = n.tag_name().name();
        if SHAPES.contains(&tag) {
            for i in 0..=chain.len() {
                let drawn = chain.get(i).copied().unwrap_or(self.sheet);
                for r in &drawn.paint {
                    let painted = self.ids.get(r).is_some_and(|&id| {
                        let paint = self.doc.get_node(id);
                        paint.is_some_and(|p| PAINT_SERVERS.contains(&p.tag_name().name()))
                            && !self.painted.insert(id)
                    });
                    self.follow(r, &mut Vec::new(), depth, shared || painted)?;
                }
                if MARKED.contains(&tag) {
                    for r in &drawn.ends {
                        self.follow(r, &mut Vec::new(), depth, shared)?;
                    }
                    for r in &drawn.mids {
                        for _ in 0..info.vertices {
                            self.follow(r, &mut Vec::new(), depth, shared)?;
                        }
                    }
                }
            }
        }
        for c in n.children().filter(Node::is_element) {
            self.visit(c, chain, depth + 1, shared)?;
        }
        if pushed {
            chain.pop();
        }
        self.active.pop();
        Ok(())
    }

    fn follow(
        &mut self,
        r: &str,
        chain: &mut Vec<&'w Drawn<'d>>,
        depth: usize,
        shared: bool,
    ) -> Result<(), Reject> {
        let target = self.ids.get(r).and_then(|&id| self.doc.get_node(id));
        match target {
            Some(t) => self.visit(t, chain, depth + 1, shared),
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
        let t = parse(svg(r#"<rect width="5" height="5" fill="red"/>"#).as_bytes()).unwrap();
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
    fn ignores_files() {
        let t =
            parse(svg(r#"<image href="/etc/passwd" width="5" height="5"/>"#).as_bytes()).unwrap();
        assert!(t.root().children().is_empty());
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
        let t = parse(image_of(&inner, 3, 0).as_bytes()).unwrap();
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
            .spawn(move || parse(doc.as_bytes()).unwrap())
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
        let mut png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/images/logo.png"
        ))
        .unwrap();
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
        let sheet = format!("<style>{}</style>", ".a{fill:red}".repeat(2_000));
        let uses = r##"<use href="#r"/>"##.repeat(30_000);
        let e = rejected(&svg(&format!(
            r#"{sheet}<defs><rect id="r"/></defs>{uses}"#
        )));
        assert!(e.contains("rule matches"), "{e}");
        parse(svg(&format!("{sheet}<rect/>")).as_bytes()).unwrap();
    }
}
