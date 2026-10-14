//! Reads `<text>` elements: the cascade of their properties, white space
//! and character positions. A port of what `usvg` does for text (its
//! `svgtree/parse.rs`, `svgtree/text.rs`, `parser/text.rs` and
//! `parser/units.rs`, version 0.48.1), on the XML instead of its tree.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::str::FromStr;

use simplecss::{Declaration, DeclarationTokenizer, Element, StyleSheet};
use svgtypes::{FontFamily, FontShorthand, Length, LengthUnit};
use usvg::roxmltree::{Document, Node, NodeId};

use super::{Anchor, Baseline, LengthAdjust, Shift, SvgChar, SvgSpan, SvgText, SvgTextDoc};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// The font size where nothing sets one, as in browsers (`medium`). The
/// SVG conversion uses it too (`usvg::Options::font_size`), so `em`
/// lengths agree.
pub(crate) const DEFAULT_FONT_SIZE: f32 = 16.0;
/// CSS pixels per inch: SVG user units are CSS pixels.
const DPI: f32 = 96.0;

/// Why text cannot be read.
#[derive(Debug)]
pub(crate) enum TextError {
    /// Text this version does not lay out (`textPath`, vertical text).
    Unsupported(String),
}

/// Paint properties: declared on the text or its spans, they go on the
/// placeholder of each span style; otherwise it inherits them.
const PAINT: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "paint-order",
    "visibility",
    "color",
];

/// Effects of the text element itself, which the placeholder takes, with
/// their initial values (none: left out when not declared).
const EFFECTS: &[(&str, Option<&str>)] = &[
    ("transform", Some("matrix(1 0 0 1 0 0)")),
    ("transform-origin", None),
    ("opacity", Some("1")),
    ("clip-path", Some("none")),
    ("mask", Some("none")),
    ("filter", Some("none")),
    ("display", Some("inline")),
    ("mix-blend-mode", Some("normal")),
    ("isolation", Some("auto")),
];

/// The text properties this module resolves itself.
const TEXT: &[&str] = &[
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "letter-spacing",
    "word-spacing",
    "text-anchor",
    "dominant-baseline",
    "alignment-baseline",
    "baseline-shift",
    "text-decoration",
    "direction",
    "writing-mode",
];

/// Properties `usvg` reads only from `style` and style sheets.
const NOT_ATTRIBUTES: &[&str] = &["mix-blend-mode", "isolation"];

/// Attributes that decide which child of a `switch` is drawn.
const CONDITIONS: &[&str] = &["systemLanguage", "requiredFeatures", "requiredExtensions"];

/// The static name of a property this module reads.
fn known(name: &str) -> Option<&'static str> {
    PAINT
        .iter()
        .chain(TEXT)
        .copied()
        .chain(EFFECTS.iter().map(|e| e.0))
        .find(|k| *k == name)
}

/// Reads every `<text>` of `doc`. Returns them, and how many elements it
/// matched against the style sheets, for the caller to charge
/// (`images/svg.rs`): the checks charged each element's `style` already.
pub(crate) fn extract(doc: &Document<'_>) -> Result<(SvgTextDoc, Work), TextError> {
    let root = doc.root_element();
    let mut cascade = Cascade::new(doc);
    let viewport = viewport(root);
    let mut texts: Vec<SvgText> = Vec::new();
    for n in root.descendants().filter(|n| is_svg(*n, "text")) {
        // A text inside a text is not drawn.
        if n.ancestors().skip(1).any(|a| is_svg(a, "text")) {
            continue;
        }
        // Each text is replaced where it is in the source: one an entity
        // expands to is somewhere else.
        let after = texts.last().map_or(root.range().start, |t| t.range.end);
        if n.range().start < after {
            return Err(TextError::Unsupported(
                "SVG <text> from an entity is not supported; write it in the SVG itself".into(),
            ));
        }
        texts.push(read_text(n, &mut cascade, viewport)?);
    }
    let work = cascade.work;
    Ok((SvgTextDoc { texts }, work))
}

/// What reading the text cost.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Work {
    /// Elements matched against the style sheets, each once.
    pub matched: usize,
}

fn is_svg(n: Node<'_, '_>, name: &str) -> bool {
    n.is_element()
        && n.tag_name().name() == name
        && matches!(n.tag_name().namespace(), None | Some(SVG_NS))
}

/// The user space `%` lengths refer to: the root `viewBox`, or its size.
fn viewport(root: Node<'_, '_>) -> (f32, f32) {
    if let Some(v) = super::super::svg::ViewBox::of(root) {
        return (v.rect[2], v.rect[3]);
    }
    let side = |name: &str| {
        root.attribute(name)
            .and_then(|v| Length::from_str(v).ok())
            .filter(|l| l.unit != LengthUnit::Percent)
            .map(|l| absolute(l, DEFAULT_FONT_SIZE))
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(100.0)
    };
    (side("width"), side("height"))
}

/// Declared values of one element, by property: the value and whether
/// it is `!important`.
type Decls = HashMap<&'static str, (String, bool)>;

/// The style sheets of a document and the declared values of the
/// elements read so far.
struct Cascade<'a, 'i> {
    sheet: StyleSheet<'a>,
    decls: HashMap<NodeId, Decls>,
    work: Work,
    _doc: std::marker::PhantomData<&'i ()>,
}

impl<'a, 'i: 'a> Cascade<'a, 'i> {
    fn new(doc: &'a Document<'i>) -> Self {
        // As `usvg` collects them (`resolve_css`).
        let mut sheet = StyleSheet::new();
        for n in doc.descendants().filter(|n| n.has_tag_name("style")) {
            if !matches!(n.attribute("type"), None | Some("text/css")) {
                continue;
            }
            if let Some(text) = n.text() {
                sheet.parse_more(text);
            }
        }
        Self {
            sheet,
            decls: HashMap::new(),
            work: Work::default(),
            _doc: std::marker::PhantomData,
        }
    }

    /// The declared values of `n`: presentation attributes, then the rules
    /// that match it in order, then its `style`, with `!important` as
    /// `usvg` applies it.
    fn decls(&mut self, n: Node<'a, 'i>) -> &Decls {
        if !self.decls.contains_key(&n.id()) {
            let mut d = Decls::new();
            for a in n.attributes() {
                if !matches!(a.namespace(), None | Some(SVG_NS)) {
                    continue;
                }
                if let Some(k) = known(a.name()).filter(|k| !NOT_ATTRIBUTES.contains(k)) {
                    d.insert(k, (a.value().to_string(), false));
                }
            }
            self.work.matched += 1;
            for rule in &self.sheet.rules {
                if rule.selector.matches(&XmlNode(n)) {
                    for decl in &rule.declarations {
                        declare(&mut d, decl);
                    }
                }
            }
            if let Some(style) = n.attribute("style") {
                for decl in DeclarationTokenizer::from(style) {
                    declare(&mut d, &decl);
                }
            }
            self.decls.insert(n.id(), d);
        }
        &self.decls[&n.id()]
    }

    /// The value `n` declares for `prop`, `inherit` aside.
    fn own(&mut self, n: Node<'a, 'i>, prop: &str) -> Option<String> {
        self.decls(n)
            .get(prop)
            .map(|v| v.0.trim())
            .filter(|v| *v != "inherit")
            .map(str::to_string)
    }

    /// An inherited property: from `n` or its nearest ancestor that sets it.
    fn inherited(&mut self, n: Node<'a, 'i>, prop: &str) -> Option<String> {
        n.ancestors()
            .filter(Node::is_element)
            .find_map(|a| self.own(a, prop))
    }

    /// A property that is not inherited, but that `usvg` takes from the
    /// direct parent when `n` does not set it.
    fn own_or_parent(&mut self, n: Node<'a, 'i>, prop: &str) -> Option<String> {
        self.own(n, prop)
            .or_else(|| n.parent_element().and_then(|p| self.own(p, prop)))
    }

    /// `n`'s font size in user units (`usvg`'s `resolve_font_size`).
    fn font_size(&mut self, n: Node<'a, 'i>) -> f32 {
        let chain: Vec<_> = n.ancestors().filter(Node::is_element).collect();
        let mut size = DEFAULT_FONT_SIZE;
        for a in chain.into_iter().rev() {
            let Some(v) = self.own(a, "font-size") else {
                continue;
            };
            size = match Length::from_str(&v) {
                Ok(l) if l.unit == LengthUnit::Percent => l.number as f32 * size / 100.0,
                Ok(l) => absolute(l, size),
                Err(_) => named_size(&v, size),
            };
        }
        size
    }

    /// `n`'s font weight (`usvg`'s `resolve_font_weight`, with any number
    /// from 1 to 1000).
    fn font_weight(&mut self, n: Node<'a, 'i>) -> u16 {
        let chain: Vec<_> = n.ancestors().filter(Node::is_element).collect();
        let mut w: u16 = 400;
        for a in chain.into_iter().rev() {
            let Some(v) = self.own(a, "font-weight") else {
                continue;
            };
            w = match v.as_str() {
                "normal" => 400,
                "bold" => 700,
                // As Chrome does: 400 becomes 700 and 200.
                "bolder" => (w + if w == 400 { 300 } else { 100 }).min(900),
                "lighter" => w.saturating_sub(if w == 400 { 200 } else { 100 }).max(100),
                s => s
                    .parse::<f32>()
                    .ok()
                    .filter(|v| (1.0..=1000.0).contains(v))
                    .map_or(w, |v| v.round() as u16),
            };
        }
        w
    }
}

/// Files a declaration: the `font` shorthand as its parts, known
/// properties as they are. A value declared `!important` before is kept.
fn declare(d: &mut Decls, decl: &Declaration<'_>) {
    let mut put = |k: &'static str, v: &str| {
        if d.get(k).is_some_and(|e| e.1) {
            return;
        }
        d.insert(k, (v.to_string(), decl.important));
    };
    if decl.name == "font" {
        if let Ok(f) = FontShorthand::from_str(decl.value) {
            put("font-style", f.font_style.unwrap_or("normal"));
            put("font-weight", f.font_weight.unwrap_or("normal"));
            put("font-size", f.font_size);
            put("font-family", f.font_family);
        }
    } else if let Some(k) = known(decl.name) {
        put(k, decl.value);
    }
}

/// A length in user units, `%` aside: `em` and `ex` relative to
/// `font_size`.
fn absolute(l: Length, font_size: f32) -> f32 {
    let n = l.number as f32;
    match l.unit {
        LengthUnit::None | LengthUnit::Px => n,
        LengthUnit::Em => n * font_size,
        LengthUnit::Ex => n * font_size / 2.0,
        LengthUnit::In => n * DPI,
        LengthUnit::Cm => n * DPI / 2.54,
        LengthUnit::Mm => n * DPI / 25.4,
        LengthUnit::Pt => n * DPI / 72.0,
        LengthUnit::Pc => n * DPI / 6.0,
        LengthUnit::Percent => 0.0,
    }
}

/// A length in user units, `%` of `base`.
fn length(l: Length, font_size: f32, base: f32) -> f32 {
    match l.unit {
        LengthUnit::Percent => base * l.number as f32 / 100.0,
        _ => absolute(l, font_size),
    }
}

/// `font-size` keywords, as `usvg` reads them: steps of 1.2 from the
/// parent's size.
fn named_size(name: &str, parent: f32) -> f32 {
    let step = match name {
        "xx-small" => -3,
        "x-small" => -2,
        "small" | "smaller" => -1,
        "large" | "larger" => 1,
        "x-large" => 2,
        "xx-large" => 3,
        _ => 0,
    };
    parent * 1.2f32.powi(step)
}

/// A roxmltree element as `simplecss` matches it (as in `usvg`).
struct XmlNode<'a, 'i>(Node<'a, 'i>);

impl Element for XmlNode<'_, '_> {
    fn parent_element(&self) -> Option<Self> {
        self.0.parent_element().map(XmlNode)
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.0.prev_sibling_element().map(XmlNode)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.0.tag_name().name() == local_name
    }

    fn attribute_matches(&self, local_name: &str, operator: simplecss::AttributeOperator) -> bool {
        self.0
            .attribute(local_name)
            .is_some_and(|v| operator.matches(v))
    }

    fn pseudo_class_matches(&self, class: simplecss::PseudoClass) -> bool {
        matches!(class, simplecss::PseudoClass::FirstChild) && self.prev_sibling_element().is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Space {
    Default,
    Preserve,
}

/// `n`'s own `xml:space`, else `default`.
fn space_of(n: Node<'_, '_>, default: Space) -> Space {
    match n.attribute((XML_NS, "space")) {
        Some("preserve") => Space::Preserve,
        Some(_) => Space::Default,
        None => default,
    }
}

/// A text node of a text element, after `trim_text`.
struct Chars<'a, 'i> {
    text: String,
    /// Elements between it and the text element.
    depth: usize,
    /// The element holding it.
    parent: Node<'a, 'i>,
}

/// An element of a text (the text, a `tspan` or an `a`), and the text
/// nodes inside it.
struct Elem<'a, 'i> {
    node: Node<'a, 'i>,
    nodes: std::ops::Range<usize>,
}

/// Collects the text nodes and elements under `parent`, as `usvg`'s
/// `parse_svg_text_element_impl` keeps them.
fn collect<'a, 'i>(
    parent: Node<'a, 'i>,
    depth: usize,
    space: Space,
    nodes: &mut Vec<Chars<'a, 'i>>,
    elems: &mut Vec<Elem<'a, 'i>>,
) -> Result<(), TextError> {
    let at = elems.len();
    elems.push(Elem {
        node: parent,
        nodes: nodes.len()..nodes.len(),
    });
    for child in parent.children() {
        if child.is_text() {
            nodes.push(Chars {
                text: trim_text(child.text().unwrap_or(""), space),
                depth,
                parent,
            });
            continue;
        }
        if !child.is_element() || !matches!(child.tag_name().namespace(), None | Some(SVG_NS)) {
            continue;
        }
        match child.tag_name().name() {
            "tspan" | "a" => collect(child, depth + 1, space_of(child, space), nodes, elems)?,
            "textPath" => {
                return Err(TextError::Unsupported(
                    "SVG <textPath> is not supported; convert the text to outlines (paths)".into(),
                ))
            }
            // `tref` and anything else draw nothing.
            _ => {}
        }
    }
    elems[at].nodes.end = nodes.len();
    Ok(())
}

/// `usvg`'s `trim_text`: line breaks and tabs become spaces, and runs of
/// spaces collapse unless `xml:space` is `preserve`.
fn trim_text(text: &str, space: Space) -> String {
    let mut s = String::with_capacity(text.len());
    let mut prev = '0';
    for c in text.chars() {
        let c = match c {
            '\r' | '\n' | '\t' => ' ',
            _ => c,
        };
        if space == Space::Default && c == ' ' && c == prev {
            continue;
        }
        prev = c;
        s.push(c);
    }
    s
}

/// `usvg`'s `trim_text_nodes`: drops the spaces at the ends of the text
/// and one of two spaces that meet across nodes, as Chrome does.
fn trim_text_nodes(nodes: &mut [Chars<'_, '_>], space: Space) {
    let space_of_node = |n: &Chars<'_, '_>| space_of(n.parent, space);
    if let [only] = nodes {
        if space == Space::Default {
            let t = &mut only.text;
            if t == " " {
                t.clear();
            } else if t.len() > 1 {
                if t.starts_with(' ') {
                    t.remove(0);
                }
                if t.ends_with(' ') {
                    t.pop();
                }
            }
        }
        return;
    }
    if nodes.len() < 2 {
        return;
    }
    let len = nodes.len() - 1;
    let mut last_non_empty: Option<usize> = None;
    for i in 0..len {
        let mut n1 = i;
        let n2 = i + 1;
        if nodes[n1].text.is_empty() {
            if let Some(n) = last_non_empty {
                n1 = n;
            }
        }
        let (space1, space2) = (space_of_node(&nodes[n1]), space_of_node(&nodes[n2]));
        let (depth1, depth2) = (nodes[n1].depth, nodes[n2].depth);
        let first = |t: &str| t.as_bytes().first().copied();
        let last = |t: &str| t.as_bytes().last().copied();
        let (c1, c2) = (first(&nodes[n1].text), last(&nodes[n1].text));
        let (c3, c4) = (first(&nodes[n2].text), last(&nodes[n2].text));
        if depth1 < depth2 {
            if c3 == Some(b' ') && space2 == Space::Default {
                nodes[n2].text.remove(0);
            }
        } else if c2 == Some(b' ') && c2 == c3 {
            if space1 == Space::Default && space2 == Space::Default {
                nodes[n1].text.pop();
            } else if space1 == Space::Preserve && space2 == Space::Default {
                nodes[n2].text.remove(0);
            }
        }
        let is_first = i == 0;
        let is_last = i == len - 1;
        if is_first && c1 == Some(b' ') && space1 == Space::Default && !nodes[n1].text.is_empty() {
            nodes[n1].text.remove(0);
        } else if is_last
            && c4 == Some(b' ')
            && !nodes[n2].text.is_empty()
            && space2 == Space::Default
        {
            nodes[n2].text.pop();
        }
        if is_last
            && c2 == Some(b' ')
            && !nodes[n1].text.is_empty()
            && nodes[n2].text.is_empty()
            && nodes[n1].text.ends_with(' ')
        {
            nodes[n1].text.pop();
        }
        if !nodes[n1].text.trim().is_empty() {
            last_non_empty = Some(n1);
        }
    }
}

/// Characters of text nodes `range`.
fn count(nodes: &[Chars<'_, '_>], range: std::ops::Range<usize>) -> usize {
    nodes[range].iter().map(|n| n.text.chars().count()).sum()
}

/// A value that may go into a placeholder's `style` as it is: one
/// declaration's worth, nothing that ends it or starts another.
fn safe(v: &str) -> Option<&str> {
    let v = v.trim();
    let bad = |c: char| matches!(c, ';' | '{' | '}' | '\\' | '!') || c.is_control();
    (!v.is_empty() && !v.contains(bad) && !v.contains("/*")).then_some(v)
}

/// `v` for an XML attribute in double quotes.
pub(crate) fn escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn read_text<'a, 'i>(
    text: Node<'a, 'i>,
    cascade: &mut Cascade<'a, 'i>,
    (vw, vh): (f32, f32),
) -> Result<SvgText, TextError> {
    if let Some(mode) = cascade.inherited(text, "writing-mode") {
        if matches!(
            mode.as_str(),
            "tb" | "tb-rl" | "vertical-rl" | "vertical-lr"
        ) {
            return Err(TextError::Unsupported(
                "vertical SVG text is not supported; convert the text to outlines (paths)".into(),
            ));
        }
    }
    let space = text
        .ancestors()
        .find(|a| a.attribute((XML_NS, "space")).is_some())
        .map_or(Space::Default, |a| space_of(a, Space::Default));
    let mut nodes = Vec::new();
    let mut elems = Vec::new();
    collect(text, 0, space, &mut nodes, &mut elems)?;
    trim_text_nodes(&mut nodes, space);

    // Character ranges of the elements.
    let starts: Vec<usize> = (0..=nodes.len()).map(|i| count(&nodes, 0..i)).collect();
    let range = |e: &Elem<'_, '_>| starts[e.nodes.start]..starts[e.nodes.end];
    let total = starts[nodes.len()];

    // Positions and rotations (`resolve_positions_list` and
    // `resolve_rotate_list`): inner elements win; a rotation repeats the
    // last one given.
    let mut x = vec![None; total];
    let mut y = vec![None; total];
    let mut dx = vec![0.0; total];
    let mut dy = vec![0.0; total];
    let mut rotate = vec![0.0; total];
    let mut last_rotate = 0.0;
    for e in &elems {
        let r = range(e);
        let size = cascade.font_size(e.node);
        let list = |name: &str, base: f32| -> Option<Vec<f32>> {
            let v = e.node.attribute(name)?;
            Some(
                svgtypes::LengthListParser::from(v)
                    .flatten()
                    .map(|l| length(l, size, base))
                    .collect(),
            )
        };
        let put = |values: Option<Vec<f32>>, out: &mut dyn FnMut(usize, f32)| {
            if let Some(values) = values {
                for (i, v) in values.into_iter().take(r.len()).enumerate() {
                    out(r.start + i, v);
                }
            }
        };
        put(list("x", vw), &mut |i, v| x[i] = Some(v));
        put(list("y", vh), &mut |i, v| y[i] = Some(v));
        put(list("dx", vw), &mut |i, v| dx[i] = v);
        put(list("dy", vh), &mut |i, v| dy[i] = v);
        if let Some(v) = e.node.attribute("rotate") {
            let angles: Vec<f32> = svgtypes::NumberListParser::from(v)
                .flatten()
                .map(|a| a as f32)
                .collect();
            for i in 0..r.len() {
                if let Some(&a) = angles.get(i) {
                    last_rotate = a;
                }
                rotate[r.start + i] = last_rotate;
            }
        }
    }

    // `textLength`, innermost first.
    let mut adjust = Vec::new();
    for e in elems.iter().rev() {
        let Some(l) = e
            .node
            .attribute("textLength")
            .and_then(|v| Length::from_str(v).ok())
        else {
            continue;
        };
        let target = length(l, cascade.font_size(e.node), vw);
        if !(target.is_finite() && target >= 0.0) {
            continue;
        }
        adjust.push(LengthAdjust {
            chars: range(e),
            length: target,
            glyphs: e.node.attribute("lengthAdjust") == Some("spacingAndGlyphs"),
        });
    }

    // Spans: the elements that hold text, with their properties.
    let mut spans: Vec<SvgSpan> = Vec::new();
    let mut paints: Vec<String> = Vec::new();
    let mut span_of: HashMap<NodeId, (u32, bool)> = HashMap::new();
    let mut chars = Vec::with_capacity(total);
    for (i, n) in nodes.iter().enumerate() {
        let (span, hidden) = if let Some(&s) = span_of.get(&n.parent.id()) {
            s
        } else {
            let props = span_props(n.parent, text, cascade, vw.hypot(vh) / 2f32.sqrt());
            let paint = paint_style(n.parent, text, cascade);
            let p = paints.iter().position(|q| *q == paint).unwrap_or_else(|| {
                paints.push(paint);
                paints.len() - 1
            });
            spans.push(SvgSpan {
                paint: p as u32,
                ..props
            });
            let hidden = cascade.own(n.parent, "display").as_deref() == Some("none");
            let s = ((spans.len() - 1) as u32, hidden);
            span_of.insert(n.parent.id(), s);
            s
        };
        for (k, c) in n.text.chars().enumerate() {
            let at = starts[i] + k;
            chars.push(SvgChar {
                c,
                span,
                x: x[at],
                y: y[at],
                dx: dx[at],
                dy: dy[at],
                rotate: rotate[at],
                hidden,
            });
        }
    }

    // The placeholder: what the element keeps, and its effects.
    let mut attrs = String::new();
    for name in std::iter::once("id").chain(CONDITIONS.iter().copied()) {
        if let Some(v) = text.attribute(name) {
            let _ = write!(attrs, " {name}=\"{}\"", escape(v));
        }
    }
    let mut effects = String::new();
    for &(prop, initial) in EFFECTS {
        let declared = cascade
            .decls(text)
            .get(prop)
            .and_then(|v| safe(&v.0).map(str::to_string));
        if let Some(v) = declared.as_deref().or(initial) {
            let _ = write!(effects, "{prop}:{v};");
        }
    }
    let rtl = cascade.inherited(text, "direction").as_deref() == Some("rtl");
    Ok(SvgText {
        range: text.range(),
        attrs,
        effects,
        in_clip: text.ancestors().any(|a| is_svg(a, "clipPath")),
        chars,
        spans,
        paints,
        adjust,
        rtl,
    })
}

/// The text properties of `n`, an element of `text` that holds
/// characters. `diagonal` is what `%` lengths other than positions
/// refer to.
fn span_props<'a, 'i>(
    n: Node<'a, 'i>,
    text: Node<'a, 'i>,
    cascade: &mut Cascade<'a, 'i>,
    diagonal: f32,
) -> SvgSpan {
    let size = cascade.font_size(n);
    let families = cascade
        .inherited(n, "font-family")
        .and_then(|v| svgtypes::parse_font_families(&v).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|f| match f {
            FontFamily::Named(s) => s,
            generic => generic.to_string(),
        })
        .collect();
    let italic = matches!(
        cascade.inherited(n, "font-style").as_deref(),
        Some("italic" | "oblique")
    );
    let mut spacing = |prop: &str| {
        cascade
            .inherited(n, prop)
            .and_then(|v| Length::from_str(&v).ok())
            .map_or(0.0, |l| length(l, size, diagonal))
    };
    let letter_spacing = spacing("letter-spacing");
    let word_spacing = spacing("word-spacing");
    let anchor = match cascade.inherited(n, "text-anchor").as_deref() {
        Some("middle") => Anchor::Middle,
        Some("end") => Anchor::End,
        _ => Anchor::Start,
    };
    let mut dominant = cascade.own_or_parent(n, "dominant-baseline");
    if dominant.as_deref() == Some("no-change") {
        dominant = n
            .parent_element()
            .and_then(|p| cascade.own_or_parent(p, "dominant-baseline"));
    }
    let alignment = cascade.own_or_parent(n, "alignment-baseline");
    let baseline = match alignment.as_deref() {
        None | Some("auto" | "baseline") => baseline(dominant.as_deref()),
        Some(a) => baseline(Some(a)),
    };
    let mut shift = Vec::new();
    for a in n.ancestors().take_while(|a| *a != text) {
        let Some(v) = cascade.own(a, "baseline-shift") else {
            continue;
        };
        match v.as_str() {
            "sub" => shift.push(Shift::Sub),
            "super" => shift.push(Shift::Super),
            v => {
                if let Ok(l) = Length::from_str(v) {
                    let base = cascade.font_size(a);
                    shift.push(Shift::Length(length(l, base, base)));
                }
            }
        }
    }
    // A decoration set on any ancestor applies (`resolve_decoration`).
    let weight = cascade.font_weight(n);
    let mut decoration = |kind: &str| {
        n.ancestors().filter(Node::is_element).any(|a| {
            cascade
                .own(a, "text-decoration")
                .is_some_and(|v| v.split(' ').any(|w| w == kind))
        })
    };
    SvgSpan {
        families,
        weight,
        italic,
        size,
        letter_spacing,
        word_spacing,
        anchor,
        baseline,
        shift,
        underline: decoration("underline"),
        overline: decoration("overline"),
        line_through: decoration("line-through"),
        paint: 0,
    }
}

fn baseline(v: Option<&str>) -> Baseline {
    match v {
        Some("middle") => Baseline::Middle,
        Some("central") => Baseline::Central,
        Some("hanging") => Baseline::Hanging,
        Some("mathematical") => Baseline::Mathematical,
        Some("text-before-edge" | "before-edge") => Baseline::TextBeforeEdge,
        Some("text-after-edge" | "after-edge") => Baseline::TextAfterEdge,
        Some("ideographic") => Baseline::Ideographic,
        _ => Baseline::Alphabetic,
    }
}

/// The paint declarations for the spans of element `n`: for each paint
/// property, the value the nearest element from `n` up to `text`
/// declares, or `inherit`, so that it inherits through the placeholder
/// (and `use` copies) whatever an author rule says about paths.
fn paint_style<'a, 'i>(
    n: Node<'a, 'i>,
    text: Node<'a, 'i>,
    cascade: &mut Cascade<'a, 'i>,
) -> String {
    let chain: Vec<_> = n
        .ancestors()
        .take_while(|a| *a != text)
        .chain(std::iter::once(text))
        .collect();
    let mut out = String::new();
    for prop in PAINT {
        let v = chain
            .iter()
            .find_map(|a| cascade.own(*a, prop).filter(|v| safe(v).is_some()));
        let _ = write!(out, "{prop}:{};", v.as_deref().map_or("inherit", str::trim));
    }
    out
}

#[cfg(test)]
mod tests;
