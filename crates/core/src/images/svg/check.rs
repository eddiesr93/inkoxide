//! The checks of a parsed SVG document, before `usvg` converts it: what
//! its copies, markers and references expand to.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use usvg::roxmltree::{Document, Node, NodeId};

use super::limits::{
    too_complex, Cost, Reject, ARC_VERTICES_PER_NUMBER, MAX_COORDINATE, MAX_DEPTH, MAX_NESTING,
    MAX_PROBE_BYTES, SHAPE_VERTICES,
};
use super::text::TextSource;
use super::{embedded, parse_xml, too_deep, SVG_NS, XLINK_NS};
use crate::images::probe::ImageFormat;
use crate::images::svg_text::SvgTextDoc;
use crate::images::ContentKey;

/// What [`check`] found.
pub(super) struct Checked {
    /// The document, each copy of its nested SVG images included.
    pub(super) cost: Cost,
    /// Its own elements, nested SVG images left out.
    pub(super) own: usize,
    /// Depth of the deepest image with a `data:` URL.
    pub(super) image_depth: usize,
    /// Its text elements.
    pub(super) text: SvgTextDoc,
    /// Its nested SVG images with text, once each.
    pub(super) nested: Vec<TextSource>,
}

/// Elements by `id`, in document order. `use` takes the first element with
/// an id and every other reference the last (as `usvg` does); the checks
/// follow all of them for those, which counts at least as much.
pub(super) type Ids<'d> = HashMap<&'d str, Vec<NodeId>>;

/// Rejects input `usvg` would expand beyond the limits, and reads the
/// text, which this conversion cannot lay out (it has no fonts). `level`
/// and `depth` place the document inside the outermost SVG.
pub(super) fn check(doc: &Document<'_>, level: usize, depth: usize) -> Result<Checked, Reject> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Reject::Invalid("SVG: the root element is not <svg>".into()));
    }
    let mut ids: Ids<'_> = HashMap::new();
    let mut depths: HashMap<NodeId, usize> = HashMap::new();
    let mut deepest = 1;
    let mut sheet = Drawn::default();
    let mut text_found = false;
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
            text_found = true;
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
                    sheet.file(p, v, &mut rest);
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
        image_depth: 0,
        nested: HashMap::new(),
        nested_text: Vec::new(),
        nested_seen: HashSet::new(),
        painted: HashSet::new(),
        active: Vec::new(),
    };
    walk.cost.add(Cost {
        style_work: sheet_work,
        ..Cost::default()
    })?;
    let at = At {
        inherited: 0,
        depth,
        shared: false,
        in_text: false,
    };
    walk.visit(root, &mut Vec::new(), at)?;
    let mut text = SvgTextDoc::default();
    if text_found {
        // Reading the text matches its elements against the sheets once
        // more.
        let (read, work) =
            crate::images::svg_text::extract(doc, MAX_PROBE_BYTES).map_err(|e| match e {
                crate::images::svg_text::TextError::Unsupported(m) => Reject::Unsupported(m),
                crate::images::svg_text::TextError::Limit(m) => too_complex(&m),
            })?;
        walk.cost.add(Cost {
            rule_matches: work.matched.saturating_mul(rule_matches),
            ..Cost::default()
        })?;
        text = read;
    }
    Ok(Checked {
        cost: walk.cost,
        own: walk.own,
        image_depth: walk.image_depth,
        text,
        nested: walk.nested_text,
    })
}

/// Descendant and sibling combinators in a selector (child combinators
/// counted too, which only overestimates).
pub(super) fn combinators(selector: &str) -> usize {
    selector
        .split(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~'))
        .filter(|part| !part.is_empty())
        .count()
        .saturating_sub(1)
}

/// Refuses `href` chains (gradients, patterns, filters) that loop: `usvg`
/// follows them until they come back to where they started, which a loop
/// further down never does.
pub(super) fn check_href_loops(
    infos: &HashMap<NodeId, Info<'_>>,
    ids: &Ids<'_>,
) -> Result<(), Reject> {
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

/// The cost of the SVG image in `url`, nothing for anything else, and
/// what converting its text needs when it has text.
pub(super) fn nested_cost(
    url: &str,
    level: usize,
    depth: usize,
) -> Result<(Cost, Option<TextSource>), Reject> {
    let Ok(url) = data_url::DataUrl::process(url) else {
        return Ok((Cost::default(), None));
    };
    let Ok((data, _)) = url.decode_to_vec() else {
        return Ok((Cost::default(), None));
    };
    let mime = url.mime_type();
    let mime = format!("{}/{}", mime.type_, mime.subtype);
    if embedded(&mime, &data) != Some(ImageFormat::Svg) {
        return Ok((Cost::default(), None));
    }
    if level > MAX_NESTING {
        return Err(too_deep());
    }
    match parse_xml(&data).and_then(|doc| check(&doc, level, depth)) {
        Ok(checked) => {
            let cost = checked.cost;
            Ok((cost, TextSource::new(Arc::new(data), checked)))
        }
        // Left out by the resolver too.
        Err(Reject::Invalid(_)) => Ok((Cost::default(), None)),
        Err(e) => Err(e),
    }
}

pub(super) fn is_svg_ns(n: Node<'_, '_>) -> bool {
    matches!(n.tag_name().namespace(), Some(SVG_NS) | None)
}

pub(super) fn has_text(n: &Node<'_, '_>) -> bool {
    n.is_text() && n.text().is_some_and(|t| !t.trim().is_empty())
}

/// XML white space, as the SVG value parsers skip it.
pub(super) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// The id an `href` names (`#id`), read as `usvg` reads an IRI.
pub(super) fn iri(v: &str) -> Option<&str> {
    let v = v.trim_start_matches(|c: char| c.is_ascii() && is_space(c as u8));
    let link = v.strip_prefix('#')?;
    let end = link.find(' ').unwrap_or(link.len());
    let (link, rest) = link.split_at(end);
    let rest_is_space = rest.bytes().all(is_space);
    (!link.is_empty() && rest_is_space).then_some(link)
}

/// Ids named by `url(#id)` in a value, read as `usvg` reads a `FuncIRI`
/// (spaces and quotes around the link).
pub(super) fn url_refs(v: &str) -> impl Iterator<Item = &str> {
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
pub(super) fn css_work(css: &str) -> usize {
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
pub(super) fn declarations(css: &str) -> impl Iterator<Item = (&str, &str)> {
    css.split([';', '{', '}']).filter_map(|d| d.split_once(':'))
}

/// Numbers in path data, a point list or a length: how many (at least one
/// per vertex `usvg` draws markers at, arcs aside), and the largest
/// magnitude (infinite for one too large for `f64`).
pub(super) fn scan_numbers(v: &str) -> (usize, f64) {
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

pub(super) fn too_large() -> Reject {
    too_complex(&format!(
        "a radius or path coordinate is larger than {MAX_COORDINATE}"
    ))
}

pub(super) const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "image",
];

pub(super) const PAINT_SERVERS: &[&str] = &["linearGradient", "radialGradient", "pattern"];

/// Attributes that are not inherited; every other one is stored again on
/// each shape beneath it (an overestimate for the rest).
pub(super) const NOT_INHERITED: &[&str] = &[
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
pub(super) struct Drawn<'d> {
    /// Paint servers and other style references, drawn once per shape.
    pub(super) paint: Vec<&'d str>,
    /// Markers drawn once per path (`marker-start`, `marker-end`).
    pub(super) ends: Vec<&'d str>,
    /// Markers drawn at every vertex (`marker-mid`, the `marker`
    /// shorthand and anything that might be either).
    pub(super) mids: Vec<&'d str>,
}

impl<'d> Drawn<'d> {
    /// Files the references in property `prop`; others go to `rest`.
    pub(super) fn file(&mut self, prop: &str, value: &'d str, rest: &mut Vec<&'d str>) {
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
    pub(super) fn resolve(&mut self, doc: &Document<'_>, ids: &Ids<'_>) {
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

    pub(super) fn is_empty(&self) -> bool {
        self.paint.is_empty() && self.ends.is_empty() && self.mids.is_empty()
    }
}

/// An element's references, read once however often it is copied.
#[derive(Default)]
pub(super) struct Info<'d> {
    /// Followed once per copy (`clip-path`, `mask`, `filter`, `href="#…"`).
    pub(super) refs: Vec<&'d str>,
    /// `use`: the copied element, which inherits from it.
    pub(super) link: Option<&'d str>,
    /// Any other element's `href="#…"`, which `usvg` follows as a chain.
    pub(super) chain: Option<&'d str>,
    pub(super) drawn: Drawn<'d>,
    /// An `href` that is not a fragment, and whether `usvg` resolves it
    /// (`image` and `feImage`).
    pub(super) url: Option<(&'d str, bool)>,
    /// Length of its attributes, `href` aside.
    pub(super) parsed_bytes: usize,
    /// Length of the attributes shapes beneath it inherit.
    pub(super) inherited_bytes: usize,
    /// Work to parse its `style` (see [`MAX_STYLE_WORK`]).
    pub(super) style_work: usize,
    /// Most vertices it draws markers at; `None`: it draws no markers.
    pub(super) vertices: Option<usize>,
    /// A `text`: the characters in it.
    pub(super) text_chars: usize,
    /// A `text`: the text nodes in it, which comments and other markup
    /// split; each is read like an element.
    pub(super) text_nodes: usize,
    /// Has text nodes of its own: inside a `text`, a span.
    pub(super) holds_text: bool,
}

impl<'d> Info<'d> {
    pub(super) fn read(n: Node<'d, '_>, doc: &Document<'_>, ids: &Ids<'_>) -> Result<Self, Reject> {
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
                        info.drawn.file(p, value, &mut styled);
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
                _ => info.drawn.file(name, v, &mut info.refs),
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
        info.vertices = marker_vertices(tag, vertices);
        info.holds_text = n.children().any(|c| c.is_text());
        if tag == "text" && is_svg_ns(n) {
            for t in n
                .descendants()
                .filter_map(|d| d.text().filter(|_| d.is_text()))
            {
                info.text_chars += t.chars().count();
                info.text_nodes += 1;
            }
        }
        Ok(info)
    }
}

/// The most vertices element `tag` draws markers at, given the numbers in
/// its path data or points (`counted`, arcs and closes included); `None`
/// for elements `usvg` draws no markers on.
pub(super) fn marker_vertices(tag: &str, counted: usize) -> Option<usize> {
    match tag {
        "path" | "line" | "polyline" | "polygon" => Some(counted + 2),
        "rect" | "circle" | "ellipse" => Some(SHAPE_VERTICES),
        _ => None,
    }
}

/// Where a [`Walk`] is, besides the chain of references the ancestors pass
/// on to shapes.
#[derive(Clone, Copy)]
pub(super) struct At {
    /// Length of the attributes the ancestors pass on to shapes.
    pub(super) inherited: usize,
    pub(super) depth: usize,
    /// Inside a paint server drawn before.
    pub(super) shared: bool,
    /// Inside a `text`.
    pub(super) in_text: bool,
}

impl At {
    /// The target of a reference followed from here: it inherits nothing
    /// (it starts a fresh chain).
    pub(super) fn fresh(self) -> Self {
        Self {
            inherited: 0,
            in_text: false,
            ..self
        }
    }

    pub(super) fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

pub(super) struct Walk<'d, 'i, 'w> {
    pub(super) doc: &'d Document<'i>,
    pub(super) ids: &'w Ids<'d>,
    pub(super) infos: &'w HashMap<NodeId, Info<'d>>,
    /// References in style sheets.
    pub(super) sheet: &'w Drawn<'d>,
    /// Style sheet matching work per element.
    pub(super) rule_matches: usize,
    /// Declaration text of the style sheets, copied onto the elements
    /// rules match (charged to each).
    pub(super) sheet_values: usize,
    /// SVG images around this document.
    pub(super) level: usize,
    pub(super) cost: Cost,
    pub(super) own: usize,
    /// Depth of the deepest image with a `data:` URL.
    pub(super) image_depth: usize,
    /// Cost of the nested SVG in each image element.
    pub(super) nested: HashMap<NodeId, Cost>,
    /// The nested SVG images with text, once each.
    pub(super) nested_text: Vec<TextSource>,
    pub(super) nested_seen: HashSet<ContentKey<Arc<Vec<u8>>>>,
    /// Paint servers drawn so far: `usvg` converts each once, and copies
    /// share its images and path data.
    pub(super) painted: HashSet<NodeId>,
    /// Elements being expanded (reference cycles are skipped, as `usvg`
    /// does for `use`, clip paths, masks and filters; `href` chains that
    /// loop are refused before).
    pub(super) active: Vec<NodeId>,
}

impl<'d, 'i, 'w> Walk<'d, 'i, 'w> {
    /// Counts `n` and everything it pulls in. `chain` holds what its
    /// ancestors pass on to shapes.
    pub(super) fn visit(
        &mut self,
        n: Node<'d, 'i>,
        chain: &mut Vec<&'w Drawn<'d>>,
        at: At,
    ) -> Result<(), Reject> {
        let At {
            inherited,
            depth,
            shared,
            in_text,
        } = at;
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
        let text = tag == "text" && is_svg_ns(n);
        let in_text = in_text || text;
        // Each element of a text that holds characters is a span, which
        // keeps the text properties it inherits.
        let span = in_text && info.holds_text;
        if !shared {
            self.cost.add(Cost {
                parsed_bytes: info.parsed_bytes.saturating_add(if shape || span {
                    inherited
                } else {
                    0
                }),
                copied_bytes: self.sheet_values,
                style_work: info.style_work,
                rule_matches: self.rule_matches,
                ..Cost::default()
            })?;
        }
        // Every copy of a text is laid out and drawn.
        self.cost.add(Cost {
            elements: info.text_nodes,
            text_chars: info.text_chars,
            ..Cost::default()
        })?;
        if let Some((url, resolved)) = info.url {
            let mut cost = Cost::default();
            if resolved {
                self.image_depth = self.image_depth.max(depth);
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
            self.follow(r, false, &mut Vec::new(), at.fresh())?;
        }
        let pushed = !info.drawn.is_empty();
        if pushed {
            chain.push(&info.drawn);
        }
        let at = At {
            inherited: inherited.saturating_add(info.inherited_bytes),
            in_text,
            ..at
        };
        if let Some(r) = info.link {
            self.follow(r, true, chain, at)?;
        }
        if shape {
            for i in 0..=chain.len() {
                let drawn = chain.get(i).copied().unwrap_or(self.sheet);
                for r in &drawn.paint {
                    for t in self.targets(r, false) {
                        let painted = self.doc.get_node(t).is_some_and(|p| {
                            PAINT_SERVERS.contains(&p.tag_name().name()) && !self.painted.insert(t)
                        });
                        let at = At {
                            shared: shared || painted,
                            ..at.fresh()
                        };
                        self.visit_target(t, &mut Vec::new(), at)?;
                    }
                }
                if let Some(vertices) = info.vertices {
                    for r in &drawn.ends {
                        self.follow(r, false, &mut Vec::new(), at.fresh())?;
                    }
                    for r in &drawn.mids {
                        for _ in 0..vertices {
                            self.follow(r, false, &mut Vec::new(), at.fresh())?;
                        }
                    }
                }
            }
        }
        for c in n.children().filter(Node::is_element) {
            self.visit(c, chain, at.deeper())?;
        }
        if pushed {
            chain.pop();
        }
        self.active.pop();
        Ok(())
    }

    /// The elements reference `r` may name: the first for `use`, all of
    /// them otherwise.
    pub(super) fn targets(&self, r: &str, first: bool) -> Vec<NodeId> {
        match self.ids.get(r) {
            Some(all) if first => all.first().copied().into_iter().collect(),
            Some(all) => all.clone(),
            None => Vec::new(),
        }
    }

    pub(super) fn follow(
        &mut self,
        r: &str,
        first: bool,
        chain: &mut Vec<&'w Drawn<'d>>,
        at: At,
    ) -> Result<(), Reject> {
        for t in self.targets(r, first) {
            self.visit_target(t, chain, at)?;
        }
        Ok(())
    }

    pub(super) fn visit_target(
        &mut self,
        t: NodeId,
        chain: &mut Vec<&'w Drawn<'d>>,
        at: At,
    ) -> Result<(), Reject> {
        match self.doc.get_node(t) {
            Some(t) => self.visit(t, chain, at.deeper()),
            None => Ok(()),
        }
    }

    /// The cost of one copy of the nested SVG in image `id`, checked once.
    pub(super) fn nested(&mut self, id: NodeId, url: &str, depth: usize) -> Result<Cost, Reject> {
        if let Some(&cost) = self.nested.get(&id) {
            return Ok(cost);
        }
        let (cost, text) = nested_cost(url, self.level + 1, depth + 1)?;
        if let Some(text) = text {
            if self.nested_seen.insert(ContentKey::new(text.bytes.clone())) {
                self.nested_text.push(text);
            }
        }
        self.nested.insert(id, cost);
        Ok(cost)
    }
}
