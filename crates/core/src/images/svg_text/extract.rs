//! Reads `<text>` elements: the cascade of their properties, white space
//! and character positions. A port of what `usvg` does for text (its
//! `svgtree/parse.rs`, `svgtree/text.rs`, `parser/text.rs` and
//! `parser/units.rs`, version 0.48.1), on the XML instead of its tree.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::str::FromStr;
use std::sync::Arc;

use simplecss::{Declaration, DeclarationTokenizer, Element, StyleSheet};
use svgtypes::{FontFamily, FontShorthand, Length, LengthUnit};
use usvg::roxmltree::{Document, Node, NodeId};

use super::path::{self, TextPathGeom, MAX_TEXT_PATH_SEGMENTS};
use super::{
    Anchor, Baseline, LengthAdjust, Orientation, ProbeForm, ProbeShape, Shift, SvgChar, SvgSpan,
    SvgText, SvgTextDoc, TextPathSpec, Writing,
};
use crate::images::svg::{iri, SVG_NS, XLINK_NS};
use crate::schema::FontFeature;
use crate::validate::rules::{MAX_FEATURE_VALUE, MAX_FONT_FEATURES};

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
    /// Text this version does not lay out (vertical text on a path).
    Unsupported(String),
    /// Text whose properties take more than the bytes allowed.
    Limit(String),
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
/// the initial values it declares when a style sheet declares the property
/// (none: left out when not declared).
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
    "text-orientation",
    "glyph-orientation-vertical",
    "font-feature-settings",
    "font-kerning",
    "font-variant",
];

/// Most different sets of OpenType features the text of an SVG uses, as
/// in a document (`validate::rules::MAX_FEATURE_SETS`): each is shaped
/// with plans and words of its own.
const MAX_FEATURE_SETS: usize = 64;

/// Properties `usvg` reads only from `style` and style sheets.
const NOT_ATTRIBUTES: &[&str] = &["mix-blend-mode", "isolation"];

/// Attributes that decide which child of a `switch` is drawn.
const CONDITIONS: &[&str] = &["systemLanguage", "requiredFeatures", "requiredExtensions"];

/// Properties a placeholder sets so that markers are not drawn on it.
const MARKERS: &[&str] = &["marker-start", "marker-mid", "marker-end"];

/// The static name of a property a placeholder may declare.
fn placed(name: &str) -> Option<&'static str> {
    PAINT
        .iter()
        .copied()
        .chain(EFFECTS.iter().map(|e| e.0))
        .chain(MARKERS.iter().copied())
        .find(|k| *k == name)
}

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
/// (`images/svg/check.rs`): the checks charged each element's `style` already.
/// Stops once the texts keep more than `max_bytes` of property values
/// (font families, paint, the placeholders' attributes and effects).
pub(crate) fn extract(
    doc: &Document<'_>,
    max_bytes: usize,
) -> Result<(SvgTextDoc, Work), TextError> {
    let root = doc.root_element();
    let mut cascade = Cascade::new(doc, max_bytes);
    let form = cascade.form();
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
    Ok((SvgTextDoc { texts, form }, work))
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
    /// The properties of [`PAINT`] and [`EFFECTS`] some rule of the sheets
    /// declares: a placeholder declares the others only where the text
    /// does.
    ruled: HashSet<&'static str>,
    decls: HashMap<NodeId, Decls>,
    /// Font family lists by declared value: the spans that inherit one
    /// share it.
    families: HashMap<String, Arc<[String]>>,
    /// Bytes of property values the texts read so far keep, and the most
    /// they may.
    kept: usize,
    max_kept: usize,
    work: Work,
    root: Node<'a, 'i>,
    /// Elements by id, the last of each (as `usvg` follows a link), once
    /// a `textPath` needs them.
    ids: Option<HashMap<&'a str, Node<'a, 'i>>>,
    /// The geometry of each element a `textPath` refers to, by side.
    geoms: HashMap<(NodeId, bool), Option<Arc<TextPathGeom>>>,
    /// Segments the text paths may still have.
    segments: usize,
    /// Feature lists by the declared values they come from, and the
    /// different lists.
    features: HashMap<[Option<String>; 3], Arc<[FontFeature]>>,
    feature_sets: HashSet<Arc<[FontFeature]>>,
}

impl<'a, 'i: 'a> Cascade<'a, 'i> {
    fn new(doc: &'a Document<'i>, max_kept: usize) -> Self {
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
        let mut ruled = HashSet::new();
        for decl in sheet.rules.iter().flat_map(|r| &r.declarations) {
            ruled.extend(placed(decl.name));
        }
        Self {
            sheet,
            ruled,
            decls: HashMap::new(),
            families: HashMap::new(),
            kept: 0,
            max_kept,
            work: Work::default(),
            root: doc.root_element(),
            ids: None,
            geoms: HashMap::new(),
            segments: MAX_TEXT_PATH_SEGMENTS,
            features: HashMap::new(),
            feature_sets: HashSet::new(),
        }
    }

    /// The OpenType features of `n`: `font-kerning` and `font-variant`,
    /// then `font-feature-settings` over them (CSS Fonts 3, 7.2), read
    /// once per distinct declaration.
    fn features(&mut self, n: Node<'a, 'i>) -> Result<Arc<[FontFeature]>, TextError> {
        let key =
            ["font-kerning", "font-variant", "font-feature-settings"].map(|p| self.inherited(n, p));
        if let Some(f) = self.features.get(&key) {
            return Ok(f.clone());
        }
        let mut tags: Vec<([u8; 4], u32)> = Vec::new();
        let mut set = |tag: &[u8; 4], value: u32| match tags.iter_mut().find(|t| &t.0 == tag) {
            Some(t) => t.1 = value,
            None => tags.push((*tag, value)),
        };
        if key[0].as_deref() == Some("none") {
            set(b"kern", 0);
        }
        for word in key[1].as_deref().unwrap_or("").split_ascii_whitespace() {
            for (tag, value) in variant(word) {
                set(tag, *value);
            }
        }
        if let Some(list) = key[2].as_deref().and_then(feature_settings) {
            for (tag, value) in list {
                set(&tag, value);
            }
        }
        tags.sort_unstable();
        let list: Arc<[FontFeature]> = tags
            .into_iter()
            .take(MAX_FONT_FEATURES)
            .map(|(tag, value)| FontFeature {
                tag: String::from_utf8_lossy(&tag).into_owned(),
                value,
            })
            .collect();
        let list = match self.feature_sets.get(&list) {
            Some(l) => l.clone(),
            None if list.is_empty() => list,
            None => {
                if self.feature_sets.len() >= MAX_FEATURE_SETS {
                    return Err(TextError::Limit(format!(
                        "its text uses more than {MAX_FEATURE_SETS} different sets of font features"
                    )));
                }
                self.keep(list.len() * (size_of::<FontFeature>() + 4))?;
                self.feature_sets.insert(list.clone());
                list
            }
        };
        let bytes = key.iter().flatten().map(String::len).sum();
        self.keep(bytes)?;
        self.features.insert(key, list.clone());
        Ok(list)
    }

    /// The element with id `id`.
    fn element(&mut self, id: &str) -> Option<Node<'a, 'i>> {
        let root = self.root;
        self.ids
            .get_or_insert_with(|| {
                root.descendants()
                    .filter_map(|n| Some((n.attribute("id")?, n)))
                    .collect()
            })
            .get(id)
            .copied()
    }

    /// The path of `textPath` element `n` and where its text starts on it:
    /// its own path data (`path`, SVG 2), or the `path` or basic shape its
    /// `href` names, after that element's `transform`; `None` when there
    /// is none.
    fn text_path(
        &mut self,
        n: Node<'a, 'i>,
        viewport: (f32, f32),
    ) -> Result<Option<TextPathSpec>, TextError> {
        let reverse = n.attribute("side") == Some("right");
        let geom = if let Some(d) = n.attribute("path") {
            path::data_geometry(d, reverse, &mut self.segments)?.map(Arc::new)
        } else {
            let href = n
                .attribute("href")
                .or_else(|| n.attribute((XLINK_NS, "href")));
            let Some(target) = href.and_then(iri).and_then(|id| self.element(id)) else {
                return Ok(None);
            };
            if !matches!(target.tag_name().namespace(), None | Some(SVG_NS)) {
                return Ok(None);
            }
            let key = (target.id(), reverse);
            if let Some(g) = self.geoms.get(&key) {
                g.clone()
            } else {
                let ts = self.own(target, "transform");
                let g =
                    path::geometry(target, ts.as_deref(), reverse, viewport, &mut self.segments)?
                        .map(Arc::new);
                self.geoms.insert(key, g.clone());
                g
            }
        };
        let Some(geom) = geom else {
            return Ok(None);
        };
        let start = n
            .attribute("startOffset")
            .and_then(|v| Length::from_str(v).ok())
            .map_or(0.0, |l| match l.unit {
                LengthUnit::Percent => geom.length() * l.number as f32 / 100.0,
                _ => absolute(l, self.font_size(n)),
            });
        Ok(Some(TextPathSpec {
            geom,
            start: if start.is_finite() { start } else { 0.0 },
        }))
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

    /// Counts `bytes` more kept, failing past the most allowed.
    fn keep(&mut self, bytes: usize) -> Result<(), TextError> {
        self.kept = self.kept.saturating_add(bytes);
        if self.kept > self.max_kept {
            return Err(TextError::Limit(format!(
                "its text properties take more than {} MiB",
                self.max_kept >> 20
            )));
        }
        Ok(())
    }

    /// The font families `value` lists, read once per distinct value.
    fn families(&mut self, value: String) -> Result<Arc<[String]>, TextError> {
        if let Some(f) = self.families.get(&value) {
            return Ok(f.clone());
        }
        let list: Arc<[String]> = svgtypes::parse_font_families(&value)
            .unwrap_or_default()
            .into_iter()
            .map(|f| match f {
                FontFamily::Named(s) => s,
                generic => generic.to_string(),
            })
            .collect();
        let bytes = list
            .iter()
            .fold(value.len(), |sum, f| sum + f.len() + size_of::<String>());
        self.keep(bytes)?;
        self.families.insert(value, list.clone());
        Ok(list)
    }

    /// How this document's placeholders are written: as elements no
    /// `!important` rule names by their type (see [`ProbeForm`]), with
    /// `inherit` on a group for the paint the sheets declare.
    fn form(&self) -> ProbeForm {
        // A rule `usvg` applies over a placeholder's `style`: it declares
        // something a placeholder sets, `!important`.
        let overrides = |rule: &simplecss::Rule<'_>| {
            rule.declarations
                .iter()
                .any(|d| d.important && (placed(d.name).is_some() || d.name == "marker"))
        };
        let rules: Vec<_> = self.sheet.rules.iter().filter(|r| overrides(r)).collect();
        // A rule that names element `tag`, not one any element would match.
        let names = |tag: &'static str, attrs: &'static [&'static str]| {
            rules.iter().any(|r| {
                r.selector.matches(&Placeholder::Element(tag, attrs))
                    && !r.selector.matches(&Placeholder::Element("", &[]))
            })
        };
        let shape = [ProbeShape::Path, ProbeShape::Polygon]
            .into_iter()
            .find(|s| !names(s.tag(), s.attributes()))
            .unwrap_or(ProbeShape::Path);
        let group = ["a", "g"]
            .into_iter()
            .find(|&g| !names(g, &[]))
            .unwrap_or("a");
        let mut group_paint = String::new();
        for &prop in PAINT.iter().filter(|p| self.ruled.contains(*p)) {
            let _ = write!(group_paint, "{prop}:inherit;");
        }
        ProbeForm {
            shape,
            group,
            group_paint,
        }
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

/// The features a `font-variant` keyword turns on or off (CSS Fonts 3,
/// 6.4-6.8); `font-variant` is a shorthand in CSS and SVG 2.
fn variant(word: &str) -> &'static [(&'static [u8; 4], u32)] {
    match word {
        "none" | "no-common-ligatures" => &[(b"liga", 0), (b"clig", 0)],
        "common-ligatures" => &[(b"liga", 1), (b"clig", 1)],
        "discretionary-ligatures" => &[(b"dlig", 1)],
        "no-discretionary-ligatures" => &[(b"dlig", 0)],
        "historical-ligatures" => &[(b"hlig", 1)],
        "contextual" => &[(b"calt", 1)],
        "no-contextual" => &[(b"calt", 0)],
        "small-caps" => &[(b"smcp", 1)],
        "all-small-caps" => &[(b"smcp", 1), (b"c2sc", 1)],
        "petite-caps" => &[(b"pcap", 1)],
        "all-petite-caps" => &[(b"pcap", 1), (b"c2pc", 1)],
        "unicase" => &[(b"unic", 1)],
        "titling-caps" => &[(b"titl", 1)],
        "lining-nums" => &[(b"lnum", 1)],
        "oldstyle-nums" => &[(b"onum", 1)],
        "proportional-nums" => &[(b"pnum", 1)],
        "tabular-nums" => &[(b"tnum", 1)],
        "diagonal-fractions" => &[(b"frac", 1)],
        "stacked-fractions" => &[(b"afrc", 1)],
        "ordinal" => &[(b"ordn", 1)],
        "slashed-zero" => &[(b"zero", 1)],
        "sub" => &[(b"subs", 1)],
        "super" => &[(b"sups", 1)],
        _ => &[],
    }
}

/// `font-feature-settings`: `normal`, or a comma-separated list of a
/// quoted four-character tag and a value (`on`, `off` or a number, 1 when
/// left out). `None` for `normal` and for a list with an error, which
/// CSS ignores whole.
fn feature_settings(v: &str) -> Option<Vec<([u8; 4], u32)>> {
    if v.trim() == "normal" {
        return None;
    }
    v.split(',')
        .map(|item| {
            let item = item.trim();
            let quote = item.chars().next().filter(|q| matches!(q, '"' | '\''))?;
            let rest = &item[1..];
            let end = rest.find(quote)?;
            let tag: [u8; 4] = rest.as_bytes()[..end].try_into().ok()?;
            if !tag.iter().all(|b| (b' '..=b'~').contains(b)) {
                return None;
            }
            let value = match rest[end + 1..].trim() {
                "" | "on" => 1,
                "off" => 0,
                n => n.parse::<u32>().ok().filter(|v| *v <= MAX_FEATURE_VALUE)?,
            };
            Some((tag, value))
        })
        .collect()
}

/// A length in user units, `%` aside: `em` and `ex` relative to
/// `font_size`.
pub(super) fn absolute(l: Length, font_size: f32) -> f32 {
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

/// A placeholder as a selector sees it, wherever it is: an element named
/// `tag` with attributes `attrs` (and those every placeholder may have),
/// among ancestors and siblings that match anything.
#[derive(Clone, Copy)]
enum Placeholder {
    Element(&'static str, &'static [&'static str]),
    Any,
}

impl Element for Placeholder {
    fn parent_element(&self) -> Option<Self> {
        Some(Placeholder::Any)
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        Some(Placeholder::Any)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        match self {
            Placeholder::Element(tag, _) => *tag == local_name,
            Placeholder::Any => true,
        }
    }

    fn attribute_matches(&self, local_name: &str, _: simplecss::AttributeOperator) -> bool {
        match self {
            Placeholder::Element(_, attrs) => {
                ["id", "style"].contains(&local_name)
                    || CONDITIONS.contains(&local_name)
                    || attrs.contains(&local_name)
            }
            Placeholder::Any => true,
        }
    }

    fn pseudo_class_matches(&self, _: simplecss::PseudoClass) -> bool {
        true
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

/// An element of a text (the text, a `tspan`, an `a` or a `textPath`),
/// and the text nodes inside it.
struct Elem<'a, 'i> {
    node: Node<'a, 'i>,
    nodes: std::ops::Range<usize>,
    text_path: bool,
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
        text_path: is_svg(parent, "textPath"),
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
            "tspan" | "a" | "textPath" => {
                collect(child, depth + 1, space_of(child, space), nodes, elems)?;
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

/// A value that may go into a placeholder's `style` as it is: one
/// declaration's worth, nothing that ends it or starts another.
fn safe(v: &str) -> Option<&str> {
    let v = v.trim();
    let bad = |c: char| matches!(c, ';' | '{' | '}' | '\\' | '!') || c.is_control();
    (!v.is_empty() && !v.contains(bad) && !v.contains("/*")).then_some(v)
}

/// `v` for an XML attribute in double quotes.
fn escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    super::push_escaped(&mut out, v);
    out
}

fn read_text<'a, 'i>(
    text: Node<'a, 'i>,
    cascade: &mut Cascade<'a, 'i>,
    (vw, vh): (f32, f32),
) -> Result<SvgText, TextError> {
    // One-line text: the vertical modes are all the same.
    let writing = match cascade.inherited(text, "writing-mode").as_deref() {
        Some("tb" | "tb-rl" | "vertical-rl" | "vertical-lr") => Writing::Vertical,
        Some("sideways-rl") => Writing::SidewaysRl,
        Some("sideways-lr") => Writing::SidewaysLr,
        _ => Writing::Horizontal,
    };
    let space = text
        .ancestors()
        .find(|a| a.attribute((XML_NS, "space")).is_some())
        .map_or(Space::Default, |a| space_of(a, Space::Default));
    let mut nodes = Vec::new();
    let mut elems = Vec::new();
    collect(text, 0, space, &mut nodes, &mut elems)?;
    trim_text_nodes(&mut nodes, space);

    // Character ranges of the elements.
    let mut starts = Vec::with_capacity(nodes.len() + 1);
    starts.push(0);
    for n in &nodes {
        starts.push(starts[starts.len() - 1] + n.text.chars().count());
    }
    let range = |e: &Elem<'_, '_>| starts[e.nodes.start]..starts[e.nodes.end];
    let total = starts[nodes.len()];

    // Text paths: a `textPath` that is a child of the text takes its
    // characters along its path; any other, or one without a path, draws
    // none of them (they still take positions).
    let mut paths: Vec<TextPathSpec> = Vec::new();
    let mut path_of: Vec<Option<u16>> = vec![None; total];
    let mut off_path = vec![false; total];
    for e in elems.iter().filter(|e| e.text_path) {
        let direct = e.node.parent_element() == Some(text);
        if direct && writing != Writing::Horizontal {
            return Err(TextError::Unsupported(
                "vertical SVG text on a path is not supported; convert the text to outlines (paths)"
                    .into(),
            ));
        }
        let spec = if direct {
            cascade.text_path(e.node, (vw, vh))?
        } else {
            None
        };
        let index = spec.and_then(|spec| {
            let i = u16::try_from(paths.len()).ok()?;
            paths.push(spec);
            Some(i)
        });
        for i in range(e) {
            path_of[i] = index;
            off_path[i] = index.is_none();
        }
    }

    // Positions and rotations (`resolve_positions_list` and
    // `resolve_rotate_list`): inner elements win; a rotation repeats the
    // last one given.
    let mut x = vec![None; total];
    let mut y = vec![None; total];
    let mut dx = vec![0.0; total];
    let mut dy = vec![0.0; total];
    let mut rotate = vec![0.0; total];
    let mut last_rotate = 0.0;
    // A `textPath` has no positions of its own.
    for e in elems.iter().filter(|e| !e.text_path) {
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
    let mut paint_of: HashMap<String, u32> = HashMap::new();
    let mut span_of: HashMap<NodeId, (u32, bool)> = HashMap::new();
    let mut chars = Vec::with_capacity(total);
    for (i, n) in nodes.iter().enumerate() {
        let (span, hidden) = if let Some(&s) = span_of.get(&n.parent.id()) {
            s
        } else {
            let props = span_props(n.parent, text, cascade, vw.hypot(vh) / 2f32.sqrt(), writing)?;
            let paint = paint_style(n.parent, text, cascade);
            let p = if let Some(&p) = paint_of.get(&paint) {
                p
            } else {
                cascade.keep(paint.len())?;
                paints.push(paint.clone());
                let p = (paints.len() - 1) as u32;
                paint_of.insert(paint, p);
                p
            };
            spans.push(SvgSpan { paint: p, ..props });
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
                hidden: hidden || off_path[at],
                path: path_of[at],
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
        let initial = initial.filter(|_| cascade.ruled.contains(prop));
        if let Some(v) = declared.as_deref().or(initial) {
            let _ = write!(effects, "{prop}:{v};");
        }
    }
    cascade.keep(attrs.len() + effects.len())?;
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
        writing,
        paths,
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
    writing: Writing,
) -> Result<SvgSpan, TextError> {
    let size = cascade.font_size(n);
    let families = match cascade.inherited(n, "font-family") {
        Some(v) => cascade.families(v)?,
        None => Arc::default(),
    };
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
    // Vertical text sits on its central baseline unless told otherwise.
    let auto = if writing == Writing::Horizontal {
        Baseline::Alphabetic
    } else {
        Baseline::Central
    };
    let baseline = match alignment.as_deref() {
        None | Some("auto" | "baseline") => baseline(dominant.as_deref(), auto),
        Some(a) => baseline(Some(a), auto),
    };
    // `glyph-orientation-vertical` is the SVG 1.1 name (SVG 2, 11.10.1.3).
    let orientation = match cascade.inherited(n, "text-orientation").as_deref() {
        Some("upright") => Some(Orientation::Upright),
        Some("sideways" | "sideways-right") => Some(Orientation::Sideways),
        Some("mixed") => Some(Orientation::Mixed),
        _ => None,
    };
    let orientation = orientation.unwrap_or_else(|| {
        match cascade
            .inherited(n, "glyph-orientation-vertical")
            .as_deref()
        {
            Some("0" | "0deg") => Orientation::Upright,
            Some("90" | "90deg") => Orientation::Sideways,
            _ => Orientation::Mixed,
        }
    });
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
    Ok(SvgSpan {
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
        orientation,
        features: cascade.features(n)?,
        paint: 0,
    })
}

/// The baseline `v` names; `auto` where it names none.
fn baseline(v: Option<&str>, auto: Baseline) -> Baseline {
    match v {
        Some("middle") => Baseline::Middle,
        Some("central") => Baseline::Central,
        Some("hanging") => Baseline::Hanging,
        Some("mathematical") => Baseline::Mathematical,
        Some("text-before-edge" | "before-edge") => Baseline::TextBeforeEdge,
        Some("text-after-edge" | "after-edge") => Baseline::TextAfterEdge,
        Some("ideographic") => Baseline::Ideographic,
        Some("alphabetic") => Baseline::Alphabetic,
        _ => auto,
    }
}

/// The paint declarations for the spans of element `n`: for each paint
/// property, the value the nearest element from `n` up to `text`
/// declares, or `inherit` when a style sheet declares the property, so
/// that it inherits through the placeholder (and `use` copies) whatever
/// an author rule says about its element.
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
        match v {
            Some(v) => {
                let _ = write!(out, "{prop}:{};", v.trim());
            }
            // Inherited anyway, unless a rule says otherwise.
            None if cascade.ruled.contains(prop) => {
                let _ = write!(out, "{prop}:inherit;");
            }
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests;
