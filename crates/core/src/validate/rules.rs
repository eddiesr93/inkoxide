//! Semantic checks on a parsed document: font and image ids, anchors,
//! value ranges, and where each node type and style property applies.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use super::parse::{push_key, unsupported_version};
use crate::date::parse_rfc3339;
use crate::error::{ValidationCode as Code, ValidationError};
use crate::images::ImageCrateDecoder;
use crate::schema::{
    is_area_name, parse_areas, Bookmark, BorderCollapse, BorderRadius, Break, BreakMode, Color,
    ColumnWidth, Document, Field, FieldKind, FontFeature, GridTrack, ImageSource, Length,
    LengthPercent, Node, NodeKind, ObjectPosition, OptEdges, PageSpec, Position, RepeatCount,
    Style, Table, TextRun, TrackBreadth, TrackSize, TransformOp, SCHEMA_VERSION,
};
use crate::{FontStore, ImageStore};

/// Stop collecting after this many problems; the rest is usually noise.
const MAX_ERRORS: usize = 100;

/// Largest page edge PDF viewers accept (200 in, PDF 1.6+ user unit aside).
const MAX_PAGE_EDGE: f32 = 14_400.0;

/// Largest percentage either way, as a fraction (10000 %), wherever a
/// percentage is accepted. Far past any useful layout, and small enough
/// that a percentage of any length stays finite: an unbounded one (say
/// `"9999999999999999999999999999999999999%"`) overflowed `f32` and wrote
/// `inf` into the content stream.
const MAX_PERCENT: f32 = 100.0;

/// Largest magnitude of a length in points (about 350 m), and of the other
/// numbers that have no tighter bound of their own: flex factors, `fr`,
/// angles, scale and matrix entries. Sums over a whole document stay far
/// from overflowing `f32`.
const MAX_NUMBER: f32 = 1_000_000.0;

/// A length as the range checks see it.
#[derive(Clone, Copy)]
enum Len {
    Points(f32),
    /// A fraction (`"50%"` is 0.5).
    Percent(f32),
    Fr(f32),
    /// `auto` and the content keywords: nothing to bound.
    Keyword,
}

impl From<Length> for Len {
    fn from(v: Length) -> Self {
        match v {
            Length::Points(p) => Self::Points(p),
            Length::Percent(f) => Self::Percent(f),
            Length::Auto => Self::Keyword,
        }
    }
}

impl From<LengthPercent> for Len {
    fn from(v: LengthPercent) -> Self {
        match v {
            LengthPercent::Points(p) => Self::Points(p),
            LengthPercent::Percent(f) => Self::Percent(f),
        }
    }
}

impl From<TrackBreadth> for Len {
    fn from(v: TrackBreadth) -> Self {
        match v {
            TrackBreadth::Points(p) => Self::Points(p),
            TrackBreadth::Percent(f) => Self::Percent(f),
            TrackBreadth::Fr(f) => Self::Fr(f),
            TrackBreadth::Auto | TrackBreadth::MinContent | TrackBreadth::MaxContent => {
                Self::Keyword
            }
        }
    }
}

/// Checks `doc`. Without `images`, image ids are not looked up (inline
/// `data` is still checked).
pub(crate) fn check(
    doc: &Document,
    fonts: &FontStore,
    images: Option<&ImageStore>,
) -> Vec<ValidationError> {
    let mut c = Checker {
        path: String::with_capacity(128),
        errors: Vec::new(),
        declared: BTreeSet::new(),
        fonts,
        images,
        doc,
        anchors: None,
        in_fixed: false,
        fields: HashMap::new(),
        feature_sets: HashSet::new(),
    };
    c.document(doc);
    c.errors
}

struct Checker<'a> {
    path: String,
    errors: Vec<ValidationError>,
    declared: BTreeSet<&'a str>,
    fonts: &'a FontStore,
    images: Option<&'a ImageStore>,
    doc: &'a Document,
    /// Ids that `#id` links can reach, collected on the first such link.
    anchors: Option<HashSet<&'a str>>,
    /// Inside a header or footer (at any depth).
    in_fixed: bool,
    /// Form field names seen so far, with their kind.
    fields: HashMap<String, FieldKind>,
    /// The different `fontFeatures` of runs seen so far, sorted by tag.
    feature_sets: HashSet<Vec<FontFeature>>,
}

/// Longest link target accepted.
const MAX_LINK: usize = 2048;

/// Longest `Id` (node, font and image ids), in characters.
const MAX_ID: usize = 256;

// The bounds below are the schema's; `schema::parity_tests` reads each
// from `document.v1.json` and checks that validation enforces it.

/// Font weights are `1..=MAX_WEIGHT` (`FontDecl.weight`).
pub(crate) const MAX_WEIGHT: u16 = 1000;
/// Longest form field `name`, in characters.
pub(crate) const MAX_FIELD_NAME: usize = 128;
/// Largest font size of a text run or a form field, in points.
pub(crate) const MAX_FONT_SIZE: f32 = 1000.0;
/// Longest bookmark title, in characters.
pub(crate) const MAX_BOOKMARK_TITLE: usize = 1024;
/// Deepest bookmark `level`.
pub(crate) const MAX_BOOKMARK_LEVEL: u8 = 32;
/// Thickest `decorationThickness`, in points.
pub(crate) const MAX_DECORATION_THICKNESS: f32 = 100.0;
/// Largest `letterSpacing` either way, in points.
pub(crate) const MAX_LETTER_SPACING: f32 = 1000.0;
/// Largest `lineHeight` multiplier.
pub(crate) const MAX_LINE_HEIGHT: f32 = 100.0;
/// Largest `textIndent` either way, in points.
pub(crate) const MAX_TEXT_INDENT: f32 = 10_000.0;
/// Largest `orphans` and `widows`.
pub(crate) const MAX_ORPHANS: u32 = 100;
/// Most `fontFeatures` of a run.
pub(crate) const MAX_FONT_FEATURES: usize = 32;
/// Largest value of a font feature.
pub(crate) const MAX_FEATURE_VALUE: u32 = 65_535;

/// Most different sets of `fontFeatures` in a document: each is shaped
/// with plans and words of its own.
pub(crate) const MAX_FEATURE_SETS: usize = 64;

/// Ids of the nodes that get a destination: the page body (flow and
/// overlays) that is displayed (not headers or footers, not under
/// `display: none`). `node` is displayed.
fn collect_anchors<'a>(node: &'a Node, out: &mut HashSet<&'a str>) {
    if let Some(id) = &node.id {
        out.insert(id);
    }
    for c in node.shown_children() {
        collect_anchors(c, out);
    }
}

/// Error message for a forced page break in [`Ctx::Whole`].
const WHOLE: &str = "a forced page break does nothing inside a row, grid, wrapping or reversed box, or inside a positioned, rotated, transformed or fixed node";

/// Where a node sits; some node types are only valid in the flow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// A child of the page: in the flow, or an overlay.
    Page,
    Flow,
    /// In flowing content that does not split at forced breaks: inside a
    /// row, grid, wrapping or reversed box, or inside a positioned,
    /// rotated, transformed or fixed node.
    Whole,
    /// In a header or footer.
    Fixed,
    /// In an overlay: a fixed, absolutely positioned page child, laid out
    /// on its own and painted whole on every page, like a header.
    Overlay,
    Cell,
}

impl<'a> Checker<'a> {
    fn err(&mut self, code: Code, field: Option<&str>, message: impl Into<String>) {
        if self.errors.len() >= MAX_ERRORS {
            return;
        }
        let mut path = self.path.clone();
        if let Some(f) = field {
            push_key(&mut path, f);
        }
        self.errors.push(ValidationError {
            code,
            path,
            message: message.into(),
        });
    }

    /// Checks an `Id`: 1 to [`MAX_ID`] characters. False when it is not one.
    fn id(&mut self, field: Option<&str>, id: &str) -> bool {
        let ok = (1..=MAX_ID).contains(&id.chars().count());
        if !ok {
            self.err(
                Code::InvalidValue,
                field,
                format!("an id is 1 to {MAX_ID} characters"),
            );
        }
        ok
    }

    /// Checks a reference to a font: an `Id` that `fonts` declares.
    fn font_ref(&mut self, field: Option<&str>, id: &str) {
        if self.id(field, id) && !self.declared.contains(id) {
            self.err(
                Code::UnknownFont,
                field,
                format!("font `{id}` is not declared in `fonts`"),
            );
        }
    }

    /// Runs `f` with the property `key` appended to the current path
    /// (`.key`).
    fn at<R>(&mut self, key: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        let len = self.path.len();
        push_key(&mut self.path, key);
        let r = f(self);
        self.path.truncate(len);
        r
    }

    /// Runs `f` with the array index `i` appended to the current path
    /// (`[i]`).
    fn at_index<R>(&mut self, i: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        let len = self.path.len();
        let _ = write!(self.path, "[{i}]");
        let r = f(self);
        self.path.truncate(len);
        r
    }

    fn document(&mut self, doc: &'a Document) {
        if doc.version != SCHEMA_VERSION {
            self.errors
                .push(unsupported_version(&doc.version.to_string()));
            return;
        }
        if let Some(date) = &doc.meta.creation_date {
            if parse_rfc3339(date).is_none() {
                self.at("meta", |c| {
                    c.err(
                        Code::InvalidValue,
                        Some("creationDate"),
                        format!("`{date}` is not an RFC 3339 timestamp"),
                    );
                });
            }
        }

        let mut seen = BTreeMap::new();
        for (i, f) in doc.fonts.iter().enumerate() {
            self.at("fonts", |c| {
                c.at_index(i, |c| {
                    c.id(Some("id"), &f.id);
                    if f.family.is_empty() {
                        c.err(Code::InvalidValue, Some("family"), "a font family is at least 1 character");
                    }
                    if let Some(prev) = seen.insert(f.id.as_str(), i) {
                        c.err(Code::DuplicateId, Some("id"), format!("font id `{}` is already declared at fonts[{prev}]", f.id));
                    }
                    if !(1..=MAX_WEIGHT).contains(&f.weight) {
                        c.err(Code::OutOfRange, Some("weight"), format!("font weight must be within 1..={MAX_WEIGHT}"));
                    }
                    if !c.fonts.contains(&f.id) {
                        c.err(
                            Code::FontNotRegistered,
                            Some("id"),
                            format!("font `{}` is declared but no font data was registered under that id", f.id),
                        );
                    }
                });
            });
            self.declared.insert(&f.id);
        }
        for (i, id) in doc.fallback_fonts.iter().enumerate() {
            self.at("fallbackFonts", |c| {
                c.at_index(i, |c| c.font_ref(None, id));
            });
        }
        if doc.pages.is_empty() {
            self.err(
                Code::InvalidValue,
                Some("pages"),
                "a document needs at least one page",
            );
        }
        for (i, page) in doc.pages.iter().enumerate() {
            self.at("pages", |c| c.at_index(i, |c| c.page(page)));
        }
    }

    fn page(&mut self, page: &PageSpec) {
        let s = page.size;
        for (name, v) in [("width", s.width), ("height", s.height)] {
            if !(v > 0.0 && v <= MAX_PAGE_EDGE) {
                self.at("size", |c| {
                    c.err(
                        Code::OutOfRange,
                        Some(name),
                        format!("page {name} must be within (0, {MAX_PAGE_EDGE}] points"),
                    );
                });
            }
        }
        let m = page.margin;
        for (name, v) in [
            ("top", m.top),
            ("right", m.right),
            ("bottom", m.bottom),
            ("left", m.left),
        ] {
            if !(v >= 0.0) {
                self.at("margin", |c| {
                    c.err(Code::OutOfRange, Some(name), "margin must be non-negative");
                });
            }
        }
        if m.left + m.right >= s.width || m.top + m.bottom >= s.height {
            self.err(
                Code::OutOfRange,
                Some("margin"),
                "margins leave no room for content",
            );
        }
        if let Some(bg) = &page.background {
            self.color(Some("background"), bg);
        }
        self.in_fixed = true;
        if let Some(h) = &page.header {
            self.at("header", |c| c.node(h, Ctx::Fixed));
        }
        if let Some(f) = &page.footer {
            self.at("footer", |c| c.node(f, Ctx::Fixed));
        }
        self.in_fixed = false;
        self.children(None, &page.children, Ctx::Page);
    }

    fn children(&mut self, parent: Option<&Style>, children: &[Node], ctx: Ctx) {
        // Names `gridArea` may use: the parent's valid area template.
        let areas = parent
            .and_then(|p| p.grid_template_areas.as_deref())
            .and_then(|rows| parse_areas(rows).ok());
        self.at("children", |c| {
            for (i, n) in children.iter().enumerate() {
                c.at_index(i, |c| {
                    c.node(n, ctx);
                    let Some(name) = &n.style.grid_area else {
                        return;
                    };
                    let st = &n.style;
                    let reported =
                        !is_area_name(name) || st.grid_row.is_some() || st.grid_column.is_some();
                    if !reported && !areas.as_ref().is_some_and(|a| a.has(name)) {
                        c.at("style", |c| {
                            c.err(
                                Code::InvalidValue,
                                Some("gridArea"),
                                format!("the parent's `gridTemplateAreas` has no area `{name}`"),
                            );
                        });
                    }
                });
            }
        });
    }

    /// Reports the properties of `style` that only tables take, for a
    /// node or cell that is not a table.
    fn table_only(&mut self, style: &Style) {
        for (p, set) in [
            ("borderCollapse", style.border_collapse.is_some()),
            ("borderSpacing", style.border_spacing.is_some()),
        ] {
            if set {
                self.err(
                    Code::StyleNotApplicable,
                    Some(p),
                    format!("`{p}` only applies to tables"),
                );
            }
        }
    }

    fn node(&mut self, node: &Node, ctx: Ctx) {
        if self.errors.len() >= MAX_ERRORS {
            return;
        }
        if let Some(id) = &node.id {
            self.id(Some("id"), id);
        }
        let is_text = matches!(node.kind, NodeKind::Text { .. });
        self.at("style", |c| c.style(&node.style, is_text, false));
        let is_table = matches!(node.kind, NodeKind::Table(_));
        if !is_table {
            self.at("style", |c| c.table_only(&node.style));
        }
        if is_table
            && node.style.border_spacing.is_some()
            && node.style.border_collapse != Some(BorderCollapse::Separate)
        {
            self.at("style", |c| {
                c.err(
                    Code::InvalidValue,
                    Some("borderSpacing"),
                    "`borderSpacing` requires `borderCollapse: \"separate\"`",
                );
            });
        }
        if let v @ Some(_) = node.brk.min_presence_ahead {
            self.at("break", |c| c.non_neg("minPresenceAhead", v));
        }
        let ctx = match ctx {
            Ctx::Page if crate::pipeline::is_overlay(node) => {
                // Its own `fixed` makes it an overlay; nothing else applies.
                let brk = Break {
                    fixed: false,
                    ..node.brk
                };
                if brk != Break::default() {
                    self.err(Code::InvalidValue, Some("break"), "only `fixed` applies to a fixed, absolutely positioned page child: it is laid out on its own and painted whole on every page");
                }
                Ctx::Overlay
            }
            Ctx::Page => Ctx::Flow,
            ctx => {
                if !matches!(ctx, Ctx::Flow | Ctx::Whole) && node.brk != Break::default() {
                    let message = if ctx == Ctx::Overlay {
                        "pagination controls do not apply inside a fixed, absolutely positioned page child: it is painted whole on every page"
                    } else {
                        "pagination controls only apply to flowing content, not inside headers, footers or table cells"
                    };
                    self.err(Code::InvalidValue, Some("break"), message);
                }
                ctx
            }
        };
        let absolute = node.style.position == Some(Position::Absolute);
        let forced = node.brk.before == BreakMode::Page || node.brk.after == BreakMode::Page;
        if forced && (ctx == Ctx::Whole || (ctx == Ctx::Flow && absolute)) {
            let message = if ctx == Ctx::Whole {
                WHOLE
            } else {
                "a forced page break does nothing on an absolutely positioned node: it is out of the flow"
            };
            self.err(Code::InvalidValue, Some("break"), message);
        }
        // Inside these, forced breaks would do nothing.
        let inner = if ctx == Ctx::Flow
            && (absolute
                || node.brk.fixed
                || node.style.rotate.is_some_and(|r| r % 360.0 != 0.0)
                || node.style.transform.as_ref().is_some_and(|t| !t.is_empty())
                || crate::paginate::banded(node))
        {
            Ctx::Whole
        } else {
            ctx
        };
        if let Some(crate::schema::PageFilter::List(list)) = node.pages.as_deref() {
            if list.is_empty() || list.contains(&0) {
                self.err(
                    Code::OutOfRange,
                    Some("pages"),
                    "page numbers are 1-based (negative: from the end) and the list is non-empty",
                );
            }
        }
        if let Some(link) = &node.link {
            self.link(link);
        }
        if let Some(b) = &node.bookmark {
            self.bookmark(b);
        }

        match &node.kind {
            NodeKind::Box { children } => self.children(Some(&node.style), children, inner),
            NodeKind::Text { runs } => {
                self.at("runs", |c| {
                    for (i, r) in runs.iter().enumerate() {
                        c.at_index(i, |c| c.run(r));
                    }
                });
                if node.style.ellipsis == Some(true) && node.style.max_lines.is_none() {
                    self.at("style", |c| {
                        c.err(
                            Code::InvalidValue,
                            Some("ellipsis"),
                            "`ellipsis` requires `maxLines`",
                        );
                    });
                }
            }
            NodeKind::Image {
                source,
                object_position,
                ..
            } => {
                self.object_position(object_position);
                self.image_source(source);
            }
            NodeKind::Table(t) => self.table(t),
            NodeKind::PageBreak => {
                if ctx == Ctx::Whole {
                    self.err(Code::InvalidNode, None, WHOLE);
                } else if ctx != Ctx::Flow {
                    self.err(
                        Code::InvalidNode,
                        None,
                        "`pageBreak` is only allowed in flowing content",
                    );
                }
            }
            NodeKind::Spacer => {}
            NodeKind::Field(f) => self.at("field", |c| c.field(f)),
        }
    }

    /// An image's bytes must decode: its header first, which is cheap and
    /// tells a too large image apart, then the image itself. The decode is
    /// cached, so the render that follows reuses it.
    fn image_source(&mut self, source: &ImageSource) {
        let (field, data, id) = match source {
            ImageSource::Ref(id) if !self.id(Some("src"), id) => return, // reported by `id`
            ImageSource::Ref(id) => {
                let Some(images) = self.images else { return };
                let Some(data) = images.get(id) else {
                    self.err(
                        Code::UnknownImage,
                        Some("src"),
                        format!("image `{id}` was not provided in the render options"),
                    );
                    return;
                };
                ("src", data.clone(), Some(id))
            }
            ImageSource::Inline(inline) => match inline.data() {
                Ok(data) => ("data", data.clone(), None),
                Err(e) => {
                    self.err(Code::InvalidImage, Some("data"), e);
                    return;
                }
            },
        };
        let message = |e: &dyn std::fmt::Display| match id {
            Some(id) => format!("image `{id}`: {e}"),
            None => e.to_string(),
        };
        if let Err(e) = crate::images::probe(&data.bytes) {
            self.err(probe_code(&e), Some(field), message(&e));
        } else if let Err(e) = data.decoded(&ImageCrateDecoder) {
            self.err(Code::InvalidImage, Some(field), message(&e));
        }
    }

    /// Bounds both axes of an image's `objectPosition` (untrusted input: a
    /// huge percentage times the free space would overflow `f32`).
    fn object_position(&mut self, p: &ObjectPosition) {
        self.at("objectPosition", |c| {
            for (axis, v) in [("x", p.x), ("y", p.y)] {
                match v {
                    LengthPercent::Points(v) if !(v.abs() <= MAX_PAGE_EDGE) => c.err(
                        Code::OutOfRange,
                        Some(axis),
                        format!("points must be within [-{MAX_PAGE_EDGE}, {MAX_PAGE_EDGE}]"),
                    ),
                    LengthPercent::Percent(f) if !(f.abs() <= MAX_PERCENT) => {
                        let max = MAX_PERCENT * 100.0;
                        c.err(
                            Code::OutOfRange,
                            Some(axis),
                            format!("percentages must be within [-{max}%, {max}%]"),
                        );
                    }
                    _ => {}
                }
            }
        });
    }

    fn field(&mut self, f: &Field) {
        if f.name.is_empty() || f.name.contains('.') || f.name.chars().count() > MAX_FIELD_NAME {
            self.err(
                Code::InvalidValue,
                Some("name"),
                format!("a field name is 1 to {MAX_FIELD_NAME} characters, without `.`"),
            );
        }
        match self.fields.get(&f.name) {
            Some(&k) if k != FieldKind::Radio || f.kind != FieldKind::Radio => self.err(
                Code::DuplicateId,
                Some("name"),
                format!(
                    "field name `{}` is already used (only radio buttons share a name)",
                    f.name
                ),
            ),
            _ => {
                self.fields.insert(f.name.clone(), f.kind);
            }
        }
        let text_like = matches!(f.kind, FieldKind::Text | FieldKind::Select);
        match (&f.font, text_like) {
            (Some(font), _) => self.font_ref(Some("font"), font),
            (None, true) => self.err(
                Code::MissingField,
                Some("font"),
                "text and select fields require `font`",
            ),
            _ => {}
        }
        self.font_size(f.size);
        if let Some(c) = &f.color {
            self.color(Some("color"), c);
        }
        let only = |c: &mut Self, set: bool, field: &str, kinds: &str| {
            if set {
                c.err(
                    Code::InvalidValue,
                    Some(field),
                    format!("`{field}` only applies to {kinds} fields"),
                );
            }
        };
        let is = |k: FieldKind| f.kind == k;
        only(
            self,
            !is(FieldKind::Text) && (f.multiline || f.password || f.max_length.is_some()),
            if f.multiline {
                "multiline"
            } else if f.password {
                "password"
            } else {
                "maxLength"
            },
            "text",
        );
        only(
            self,
            !is(FieldKind::Select) && !f.options.is_empty(),
            "options",
            "select",
        );
        only(
            self,
            !matches!(f.kind, FieldKind::Checkbox | FieldKind::Radio) && f.checked,
            "checked",
            "checkbox and radio",
        );
        only(
            self,
            is(FieldKind::Checkbox) && f.value.is_some(),
            "value",
            "text, select and radio",
        );
        if f.max_length == Some(0) {
            self.err(Code::OutOfRange, Some("maxLength"), "must be at least 1");
        }
        if let (Some(max), Some(v)) = (f.max_length, &f.value) {
            if v.chars().count() > max as usize {
                self.err(
                    Code::OutOfRange,
                    Some("value"),
                    "is longer than `maxLength`",
                );
            }
        }
        match f.kind {
            FieldKind::Radio if f.value.as_deref().is_none_or(str::is_empty) => self.err(
                Code::MissingField,
                Some("value"),
                "a radio button requires a non-empty `value` (what it exports)",
            ),
            FieldKind::Select => {
                if f.options.is_empty() {
                    self.err(
                        Code::MissingField,
                        Some("options"),
                        "a select field requires `options`",
                    );
                } else if let Some(v) = &f.value {
                    if !f.options.contains(v) {
                        self.err(Code::InvalidValue, Some("value"), "is not one of `options`");
                    }
                }
            }
            _ => {}
        }
    }

    fn link(&mut self, link: &str) {
        if link.is_empty() || link.chars().count() > MAX_LINK {
            self.err(
                Code::InvalidValue,
                Some("link"),
                format!("a link must be 1 to {MAX_LINK} characters long"),
            );
            return;
        }
        if let Some(id) = link.strip_prefix('#') {
            let doc = self.doc;
            let anchors = self.anchors.get_or_insert_with(|| {
                let mut out = HashSet::new();
                for n in doc.pages.iter().flat_map(|p| &p.children) {
                    if !n.hidden() {
                        collect_anchors(n, &mut out);
                    }
                }
                out
            });
            if !anchors.contains(id) {
                self.err(
                    Code::UnknownAnchor,
                    Some("link"),
                    format!("no displayed node in the page body (flow and overlays) has id `{id}`"),
                );
            }
            return;
        }
        // RFC 3986 scheme: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
        let scheme = link.split_once(':').map(|(s, _)| s).filter(|s| {
            let mut c = s.chars();
            c.next().is_some_and(|c| c.is_ascii_alphabetic())
                && c.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
        match scheme {
            None => self.err(
                Code::InvalidValue,
                Some("link"),
                "a link must be `#id` or an absolute URI with a scheme (e.g. `https:`)",
            ),
            Some(s) if s.eq_ignore_ascii_case("javascript") => self.err(
                Code::InvalidValue,
                Some("link"),
                "`javascript:` links are not allowed",
            ),
            Some(_) => {}
        }
    }

    fn bookmark(&mut self, b: &Bookmark) {
        self.at("bookmark", |c| {
            if c.in_fixed {
                c.err(
                    Code::InvalidValue,
                    None,
                    "bookmarks only apply to flowing content, not inside headers or footers",
                );
            }
            if b.title.is_empty() || b.title.chars().count() > MAX_BOOKMARK_TITLE {
                c.err(
                    Code::InvalidValue,
                    Some("title"),
                    format!("a bookmark title must be 1 to {MAX_BOOKMARK_TITLE} characters long"),
                );
            }
            if !(1..=MAX_BOOKMARK_LEVEL).contains(&b.level) {
                c.err(
                    Code::OutOfRange,
                    Some("level"),
                    format!("must be within 1..={MAX_BOOKMARK_LEVEL}"),
                );
            }
        });
    }

    fn run(&mut self, r: &TextRun) {
        if let Some(link) = &r.link {
            self.link(link);
        }
        match (&r.text, &r.placeholder) {
            (Some(_), Some(_)) => self.err(
                Code::InvalidValue,
                Some("placeholder"),
                "a run takes either `text` or `placeholder`, not both",
            ),
            (None, None) => self.err(
                Code::MissingField,
                Some("text"),
                "a run requires `text` (or a `placeholder`)",
            ),
            _ => {}
        }
        self.font_ref(Some("font"), &r.font);
        self.font_size(r.size);
        if let Some(t) = r.decoration_thickness {
            if !(t > 0.0 && t <= MAX_DECORATION_THICKNESS) {
                self.err(
                    Code::OutOfRange,
                    Some("decorationThickness"),
                    format!("must be within (0, {MAX_DECORATION_THICKNESS}]"),
                );
            }
        }
        if let Some(col) = &r.decoration_color {
            self.color(Some("decorationColor"), col);
        }
        if let Some(col) = &r.color {
            self.color(Some("color"), col);
        }
        if let Some(ls) = r.letter_spacing {
            if !(ls.abs() <= MAX_LETTER_SPACING) {
                self.err(
                    Code::OutOfRange,
                    Some("letterSpacing"),
                    format!("letter spacing must be within ±{MAX_LETTER_SPACING} points"),
                );
            }
        }
        if let Some(features) = &r.font_features {
            self.font_features(features);
        }
    }

    fn font_features(&mut self, features: &[FontFeature]) {
        let field = Some("fontFeatures");
        if features.len() > MAX_FONT_FEATURES {
            self.err(
                Code::OutOfRange,
                field,
                format!("a run takes at most {MAX_FONT_FEATURES} font features"),
            );
            return;
        }
        let mut sorted = features.to_vec();
        sorted.sort_by(|a, b| a.tag.cmp(&b.tag));
        for (i, f) in sorted.iter().enumerate() {
            if !(f.tag.len() == 4 && f.tag.bytes().all(|b| (b' '..=b'~').contains(&b))) {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!(
                        "{:?} is not a feature tag: four printable ASCII characters, such as \"tnum\"",
                        f.tag
                    ),
                );
                return;
            }
            if f.value > MAX_FEATURE_VALUE {
                self.err(
                    Code::OutOfRange,
                    field,
                    format!(
                        "the value of {:?} must be within 0..={MAX_FEATURE_VALUE}",
                        f.tag
                    ),
                );
                return;
            }
            if i > 0 && sorted[i - 1].tag == f.tag {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!("{:?} is set twice", f.tag),
                );
                return;
            }
        }
        if sorted.is_empty() || self.feature_sets.contains(&sorted) {
            return;
        }
        if self.feature_sets.len() >= MAX_FEATURE_SETS {
            self.err(
                Code::OutOfRange,
                field,
                format!(
                    "a document uses at most {MAX_FEATURE_SETS} different sets of font features"
                ),
            );
            return;
        }
        self.feature_sets.insert(sorted);
    }

    fn table(&mut self, t: &Table) {
        let ncols = t.columns.len();
        if ncols == 0 {
            self.err(
                Code::InvalidTable,
                Some("columns"),
                "a table needs at least one column",
            );
            return;
        }
        self.at("columns", |c| {
            for (i, col) in t.columns.iter().enumerate() {
                let width = match col.width {
                    ColumnWidth::Points(p) => Len::Points(p),
                    ColumnWidth::Fr(f) => Len::Fr(f),
                    ColumnWidth::Auto => continue,
                };
                c.at_index(i, |c| c.len(Some("width"), width, false));
            }
        });
        if t.header_rows as usize > t.rows.len() {
            self.err(
                Code::InvalidTable,
                Some("headerRows"),
                format!(
                    "headerRows ({}) exceeds the number of rows ({})",
                    t.header_rows,
                    t.rows.len()
                ),
            );
        }
        // Occupancy: how many more rows each column is covered by a rowSpan from above.
        let mut carry = vec![0u32; ncols];
        let nrows = t.rows.len();
        self.at("rows", |c| {
            for (ri, row) in t.rows.iter().enumerate() {
                c.at_index(ri, |c| {
                    if let Some(id) = &row.id {
                        c.id(Some("id"), id);
                    }
                    c.non_neg("minHeight", row.min_height);
                    if let Some(bg) = &row.background_color {
                        c.color(Some("backgroundColor"), bg);
                    }
                    let mut col = 0usize;
                    for (ci, cell) in row.cells.iter().enumerate() {
                        c.at("cells", |c| {
                            c.at_index(ci, |c| {
                                if let Some(id) = &cell.id {
                                    c.id(Some("id"), id);
                                }
                                while col < ncols && carry[col] > 0 {
                                    col += 1;
                                }
                                let span = cell.col_span.max(1) as usize;
                                if cell.col_span == 0 || cell.row_span == 0 {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        "colSpan and rowSpan must be at least 1",
                                    );
                                }
                                if col + span > ncols || (col..col + span).any(|k| carry[k] > 0) {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        format!("cell does not fit: row has {ncols} columns"),
                                    );
                                } else if ri + cell.row_span as usize > nrows {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "rowSpan extends past the last row",
                                    );
                                } else if ri < t.header_rows as usize
                                    && ri + cell.row_span as usize > t.header_rows as usize
                                {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "a header cell cannot span into body rows",
                                    );
                                } else {
                                    carry[col..col + span].fill(cell.row_span);
                                }
                                col += span;
                                c.at("style", |c| {
                                    c.style(&cell.style, false, true);
                                    c.table_only(&cell.style);
                                });
                                c.children(Some(&cell.style), &cell.children, Ctx::Cell);
                            });
                        });
                    }
                    while col < ncols && carry[col] > 0 {
                        col += 1;
                    }
                    if col < ncols {
                        c.err(
                            Code::InvalidTable,
                            Some("cells"),
                            format!("row covers {col} of {ncols} columns"),
                        );
                    }
                    for v in &mut carry {
                        *v = v.saturating_sub(1);
                    }
                });
            }
        });
    }

    /// The `size` of a run or a form field.
    fn font_size(&mut self, size: f32) {
        if !(size > 0.0 && size <= MAX_FONT_SIZE) {
            self.err(
                Code::OutOfRange,
                Some("size"),
                format!("font size must be within (0, {MAX_FONT_SIZE}]"),
            );
        }
    }

    fn color(&mut self, field: Option<&str>, c: &Color) {
        if c.iter().any(|v| !(0.0..=1.0).contains(v)) {
            self.err(
                Code::OutOfRange,
                field,
                "colour components must be within [0, 1]",
            );
        }
    }

    /// A non-negative number of at most [`MAX_NUMBER`].
    fn non_neg(&mut self, field: &str, v: Option<f32>) {
        if let Some(v) = v {
            if !(v >= 0.0) {
                self.err(Code::OutOfRange, Some(field), "must be non-negative");
            } else if !(v <= MAX_NUMBER) {
                self.err(
                    Code::OutOfRange,
                    Some(field),
                    format!("must be at most {MAX_NUMBER}"),
                );
            }
        }
    }

    /// A number within ±[`MAX_NUMBER`] (an angle, a scale factor).
    fn signed(&mut self, field: Option<&str>, v: f32) {
        if !(v.abs() <= MAX_NUMBER) {
            self.err(
                Code::OutOfRange,
                field,
                format!("must be within [-{MAX_NUMBER}, {MAX_NUMBER}]"),
            );
        }
    }

    /// Bounds a length, the one check for every length in the document:
    /// points (and `fr`) within ±[`MAX_NUMBER`], percentages within
    /// ±[`MAX_PERCENT`], neither negative unless `negative` allows it.
    /// Untrusted input: past these, layout overflows `f32`.
    fn len(&mut self, field: Option<&str>, v: impl Into<Len>, negative: bool) {
        let (v, max, unit, what) = match v.into() {
            Len::Points(p) => (p, MAX_NUMBER, "", "points"),
            Len::Percent(f) => (f, MAX_PERCENT, "%", "percentages"),
            Len::Fr(f) => (f, MAX_NUMBER, "fr", "flexible sizes"),
            Len::Keyword => return,
        };
        if !negative && v < 0.0 {
            self.err(Code::OutOfRange, field, "must be non-negative");
        } else if !(v.abs() <= max) {
            // Percentages are stored as fractions; report them as written.
            let max = if unit == "%" { max * 100.0 } else { max };
            let min = if negative {
                format!("-{max}")
            } else {
                "0".into()
            };
            self.err(
                Code::OutOfRange,
                field,
                format!("{what} must be within [{min}{unit}, {max}{unit}]"),
            );
        }
    }

    /// [`Self::len`] on each side that is set.
    fn edges<T: Copy + Into<Len>>(&mut self, field: &str, e: Option<&OptEdges<T>>, negative: bool) {
        let Some(e) = e else { return };
        self.at(field, |c| {
            for (k, v) in e.iter() {
                c.len(Some(k), *v, negative);
            }
        });
    }

    /// A grid track: its breadth, or each bound of `{min, max}` or
    /// `{fitContent}`.
    fn track(&mut self, t: &TrackSize) {
        match *t {
            TrackSize::Breadth(b) => self.len(None, b, false),
            TrackSize::MinMax(min, max) => {
                self.len(Some("min"), min, false);
                self.len(Some("max"), max, false);
            }
            TrackSize::FitContent(f) => self.len(Some("fitContent"), f, false),
        }
    }

    fn style(&mut self, s: &Style, is_text: bool, is_cell: bool) {
        if !is_text {
            let props: Vec<_> = s.text_props_set().collect();
            for p in props {
                self.err(
                    Code::StyleNotApplicable,
                    Some(p),
                    format!("`{p}` only applies to text nodes"),
                );
            }
        }
        if !is_cell && s.vertical_align.is_some() {
            self.err(
                Code::StyleNotApplicable,
                Some("verticalAlign"),
                "`verticalAlign` only applies to table cells",
            );
        }
        self.non_neg("flexGrow", s.flex_grow);
        self.non_neg("flexShrink", s.flex_shrink);
        match s.border_radius {
            Some(BorderRadius::Uniform(r)) => self.non_neg("borderRadius", Some(r)),
            Some(BorderRadius::Corners(c)) => {
                for (k, v) in [
                    ("topLeft", c.top_left),
                    ("topRight", c.top_right),
                    ("bottomRight", c.bottom_right),
                    ("bottomLeft", c.bottom_left),
                ] {
                    self.non_neg(&format!("borderRadius.{k}"), v);
                }
            }
            None => {}
        }
        if is_cell {
            // The table places and sizes a cell and paints its background
            // and borders: its own box placement, size within the row and
            // visual effects would be dropped.
            for (name, set) in [
                ("transform", s.transform.is_some()),
                ("transformOrigin", s.transform_origin.is_some()),
                ("zIndex", s.z_index.is_some()),
                ("rotate", s.rotate.is_some()),
                ("opacity", s.opacity.is_some()),
                ("overflow", s.overflow.is_some()),
                ("margin", s.margin.is_some()),
                ("position", s.position.is_some()),
                ("inset", s.inset.is_some()),
                ("width", s.width.is_some()),
                ("minWidth", s.min_width.is_some()),
                ("maxWidth", s.max_width.is_some()),
                ("maxHeight", s.max_height.is_some()),
                ("alignSelf", s.align_self.is_some()),
                ("justifySelf", s.justify_self.is_some()),
                ("flexGrow", s.flex_grow.is_some()),
                ("flexShrink", s.flex_shrink.is_some()),
                ("flexBasis", s.flex_basis.is_some()),
                ("gridColumn", s.grid_column.is_some()),
                ("gridRow", s.grid_row.is_some()),
                ("gridArea", s.grid_area.is_some()),
            ] {
                if set {
                    self.err(
                        Code::StyleNotApplicable,
                        Some(name),
                        format!("`{name}` does not apply to table cells"),
                    );
                }
            }
        }
        if let Some(ops) = &s.transform {
            self.at("transform", |c| {
                for (k, op) in ops.iter().enumerate() {
                    c.at_index(k, |c| c.transform_op(op));
                }
            });
        }
        if let Some(origin) = &s.transform_origin {
            self.at("transformOrigin", |c| {
                for (i, v) in origin.iter().enumerate() {
                    c.at_index(i, |c| c.len(None, *v, true));
                }
            });
        }
        if let Some(r) = s.rotate {
            self.signed(Some("rotate"), r);
        }
        self.non_neg("borderSpacing", s.border_spacing);
        for (name, v) in [
            ("flexBasis", s.flex_basis),
            ("width", s.width),
            ("height", s.height),
            ("minWidth", s.min_width),
            ("minHeight", s.min_height),
            ("maxWidth", s.max_width),
            ("maxHeight", s.max_height),
        ] {
            if let Some(v) = v {
                self.len(Some(name), v, false);
            }
        }
        for (name, v) in [
            ("gap", s.gap),
            ("rowGap", s.row_gap),
            ("columnGap", s.column_gap),
        ] {
            if let Some(v) = v {
                self.len(Some(name), v, false);
            }
        }
        self.edges("padding", s.padding.as_ref(), false);
        self.edges("margin", s.margin.as_ref(), true);
        self.edges("inset", s.inset.as_ref(), true);
        if let Some(bw) = &s.border_width {
            self.at("borderWidth", |c| {
                for (k, v) in bw.iter() {
                    c.non_neg(k, Some(*v));
                }
            });
        }
        if let Some(bc) = &s.border_color {
            for (k, v) in bc.iter() {
                let f = format!("borderColor.{k}");
                self.color(Some(&f), v);
            }
        }
        if let Some(bg) = &s.background_color {
            self.color(Some("backgroundColor"), bg);
        }
        if let Some(o) = s.opacity {
            if !(0.0..=1.0).contains(&o) {
                self.err(Code::OutOfRange, Some("opacity"), "must be within [0, 1]");
            }
        }
        if let Some(a) = s.aspect_ratio {
            if !(a > 0.0) {
                self.err(Code::OutOfRange, Some("aspectRatio"), "must be positive");
            } else if !((1.0 / MAX_NUMBER..=MAX_NUMBER).contains(&a)) {
                self.err(
                    Code::OutOfRange,
                    Some("aspectRatio"),
                    format!("must be within [{}, {MAX_NUMBER}]", 1.0 / MAX_NUMBER),
                );
            }
        }
        if let Some(lh) = s.line_height {
            if !(lh > 0.0 && lh <= MAX_LINE_HEIGHT) {
                self.err(
                    Code::OutOfRange,
                    Some("lineHeight"),
                    format!("must be within (0, {MAX_LINE_HEIGHT}]"),
                );
            }
        }
        if s.max_lines == Some(0) {
            self.err(Code::OutOfRange, Some("maxLines"), "must be at least 1");
        }
        if s.text_indent
            .is_some_and(|v| !v.is_finite() || v.abs() > MAX_TEXT_INDENT)
        {
            self.err(
                Code::OutOfRange,
                Some("textIndent"),
                format!("must be within [-{MAX_TEXT_INDENT}, {MAX_TEXT_INDENT}]"),
            );
        }
        for (name, v) in [("orphans", s.orphans), ("widows", s.widows)] {
            if v.is_some_and(|v| !(1..=MAX_ORPHANS).contains(&v)) {
                self.err(
                    Code::OutOfRange,
                    Some(name),
                    format!("must be within [1, {MAX_ORPHANS}]"),
                );
            }
        }
        for (name, tracks) in [
            ("gridTemplateColumns", &s.grid_template_columns),
            ("gridTemplateRows", &s.grid_template_rows),
        ] {
            if let Some(tracks) = tracks {
                self.at(name, |c| c.template(tracks));
            }
        }
        for (name, tracks) in [
            ("gridAutoColumns", &s.grid_auto_columns),
            ("gridAutoRows", &s.grid_auto_rows),
        ] {
            if let Some(tracks) = tracks {
                self.at(name, |c| {
                    for (i, t) in tracks.iter().enumerate() {
                        c.at_index(i, |c| c.track(t));
                    }
                });
            }
        }
        if let Some(rows) = &s.grid_template_areas {
            if let Err((code, msg)) = parse_areas(rows) {
                self.err(code, Some("gridTemplateAreas"), msg);
            }
        }
        if let Some(name) = &s.grid_area {
            if !is_area_name(name) {
                self.err(
                    Code::InvalidValue,
                    Some("gridArea"),
                    format!("`{name}` is not a valid area name"),
                );
            } else if s.grid_row.is_some() || s.grid_column.is_some() {
                self.err(
                    Code::InvalidValue,
                    Some("gridArea"),
                    "`gridArea` cannot be combined with `gridRow` or `gridColumn`",
                );
            }
        }
    }

    /// `gridTemplateColumns` / `gridTemplateRows`. Auto-repeat follows CSS:
    /// at most one per list, and only with tracks of a fixed size.
    fn template(&mut self, tracks: &[GridTrack]) {
        let mut auto_repeats = 0;
        for (i, t) in tracks.iter().enumerate() {
            let (sizes, auto) = match t {
                GridTrack::Single(t) => (std::slice::from_ref(t), false),
                GridTrack::Repeat(n, t) => (&t[..], !matches!(n, RepeatCount::Count(_))),
            };
            self.at_index(i, |c| match t {
                GridTrack::Single(t) => c.track(t),
                GridTrack::Repeat(_, t) => c.at("tracks", |c| {
                    for (j, t) in t.iter().enumerate() {
                        c.at_index(j, |c| c.track(t));
                    }
                }),
            });
            if auto {
                auto_repeats += 1;
                if auto_repeats > 1 {
                    self.at_index(i, |c| {
                        c.err(
                            Code::InvalidValue,
                            Some("repeat"),
                            "only one auto-fill / auto-fit repeat is allowed per track list",
                        );
                    });
                } else if sizes.iter().all(|t| fixed_size(t).is_some())
                    && !sizes.iter().any(|t| fixed_size(t).is_some_and(|v| v > 0.0))
                {
                    self.at_index(i, |c| {
                        c.err(
                            Code::InvalidValue,
                            Some("tracks"),
                            "an auto-fill / auto-fit repeat needs a track with a positive fixed size",
                        );
                    });
                }
            }
        }
        if auto_repeats > 0 {
            let all_fixed = tracks.iter().all(|t| match t {
                GridTrack::Single(t) => fixed_size(t).is_some(),
                GridTrack::Repeat(_, t) => t.iter().all(|t| fixed_size(t).is_some()),
            });
            if !all_fixed {
                self.err(
                    Code::InvalidValue,
                    None,
                    "with an auto-fill / auto-fit repeat, every track needs a fixed size (points or a percentage) as its minimum or maximum",
                );
            }
        }
    }

    /// One transform function: lengths as everywhere, angles and factors
    /// within ±[`MAX_NUMBER`], skew angles short of ±90°.
    fn transform_op(&mut self, op: &TransformOp) {
        let each = |c: &mut Self, name: &str, xs: &[f32], f: fn(&mut Self, f32)| {
            c.at(name, |c| {
                for (i, &x) in xs.iter().enumerate() {
                    c.at_index(i, |c| f(c, x));
                }
            });
        };
        match op {
            TransformOp::Translate(v) => self.at("translate", |c| {
                for (i, &v) in v.iter().enumerate() {
                    c.at_index(i, |c| c.len(None, v, true));
                }
            }),
            TransformOp::Scale(v) => each(self, "scale", v, |c, x| c.signed(None, x)),
            TransformOp::Rotate(v) => self.signed(Some("rotate"), *v),
            TransformOp::Skew(v) => each(self, "skew", v, |c, a| {
                if !(a.abs() < 90.0) {
                    c.err(
                        Code::OutOfRange,
                        None,
                        "skew angles must be within (-90, 90) degrees",
                    );
                }
            }),
            TransformOp::Matrix(m) => each(self, "matrix", m, |c, x| c.signed(None, x)),
        }
    }
}

/// The fixed (points or percentage) part of a track, as Taffy uses it to
/// count auto repetitions; percentages count as their raw fraction.
fn fixed_size(t: &TrackSize) -> Option<f32> {
    let fixed = |b: TrackBreadth| match b {
        TrackBreadth::Points(v) | TrackBreadth::Percent(v) => Some(v),
        _ => None,
    };
    match *t {
        TrackSize::Breadth(b) => fixed(b),
        TrackSize::MinMax(a, b) => fixed(b).or(fixed(a)),
        TrackSize::FitContent(_) => None,
    }
}

/// The code for an image that failed its header probe.
fn probe_code(e: &crate::images::ProbeError) -> Code {
    match e {
        crate::images::ProbeError::Invalid(_) => Code::InvalidImage,
        crate::images::ProbeError::TooLarge(_) => Code::ImageTooLarge,
    }
}
