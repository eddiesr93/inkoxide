//! Semantic checks on a parsed document: font and image ids, anchors,
//! value ranges, and where each node type and style property applies.

mod attachments;
#[cfg(test)]
pub(crate) use attachments::{MAX_XMP_NAME, MAX_XMP_PROPERTIES, MAX_XMP_SCHEMAS, MAX_XMP_VALUE};
mod fields;
mod images;
mod links;
mod styles;
mod tables;
mod tagging;
mod text;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use super::parse::{push_key, unsupported_version};
use crate::date::parse_rfc3339;
use crate::error::{ValidationCode as Code, ValidationError};
use crate::images::ContentKey;
use crate::pdf::{Profile, PDF_A_MIN_PAGE_EDGE};
use crate::schema::{
    is_area_name, parse_areas, BorderCollapse, Break, BreakMode, Document, FieldKind, FontFeature,
    Length, LengthPercent, Node, NodeKind, PageSpec, Position, Style, TrackBreadth, SCHEMA_VERSION,
};
use crate::{AttachmentStore, FontStore, ImageStore};

/// Stop collecting after this many problems; the rest is usually noise.
const MAX_ERRORS: usize = 100;

/// Largest page edge PDF viewers accept (200 in, PDF 1.6+ user unit aside).
pub(crate) const MAX_PAGE_EDGE: f32 = 14_400.0;

/// Largest percentage either way, as a fraction (10000 %), wherever a
/// percentage is accepted. Far past any useful layout, and small enough
/// that a percentage of any length stays finite: an unbounded one (say
/// `"9999999999999999999999999999999999999%"`) overflowed `f32` and wrote
/// `inf` into the content stream.
pub(crate) const MAX_PERCENT: f32 = 100.0;

/// Largest magnitude of a length in points (about 350 m), and of the other
/// numbers that have no tighter bound of their own: flex factors, `fr`,
/// angles, scale and matrix entries. Sums over a whole document stay far
/// from overflowing `f32`.
pub(crate) const MAX_NUMBER: f32 = 1_000_000.0;

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

/// Checks `doc` for a render with `profile`. Without `images`, image ids
/// are not looked up (inline `data` is still checked); without
/// `attachments` (`validate`), neither are attachment ids, nor what only a
/// render can tell. The fonts of
/// `deferred` ([`crate::RenderOptions::deferred_fonts`]) may be missing from
/// `fonts`. Under PDF/A, a
/// valid document that the level does not allow gets one error:
/// `PDFA_CONFLICT` at the first thing it does not allow (a page under
/// [`PDF_A_MIN_PAGE_EDGE`] at any level; transparency at PDF/A-1b).
pub(crate) fn check(
    doc: &Document,
    fonts: &FontStore,
    deferred: &[String],
    images: Option<&ImageStore>,
    attachments: Option<&AttachmentStore>,
    profile: Profile,
) -> Vec<ValidationError> {
    let mut c = Checker {
        attachments,
        rendering: attachments.is_some(),
        profile,
        decoration: 0,
        last_heading: None,
        structure: tagging::Structure::default(),
        path: String::with_capacity(128),
        errors: Vec::new(),
        declared: BTreeSet::new(),
        fonts,
        deferred,
        images,
        doc,
        anchors: None,
        in_fixed: false,
        fields: HashMap::new(),
        conflict: None,
        image_transparency: HashMap::new(),
        field_groups: HashSet::new(),
        checked_radios: HashSet::new(),
        feature_sets: HashSet::new(),
        table_slots: 0,
    };
    c.document(doc);
    if c.errors.is_empty() {
        c.errors.extend(c.conflict);
    }
    c.errors
}

/// The error of a render at PDF/A-1b, which forbids transparency: `what`,
/// at `path`, needs it.
pub(crate) fn transparency_conflict(path: String, what: &str) -> ValidationError {
    ValidationError {
        code: Code::PdfaConflict,
        path,
        message: format!("PDF/A-1b does not allow transparency, which {what} needs"),
    }
}

/// How the painter uses a colour's alpha.
#[derive(Clone, Copy, PartialEq)]
enum Alpha {
    /// Below 1 it is drawn with transparency.
    Blended,
    /// 0 draws nothing (backgrounds); between 0 and 1 it is blended.
    Fill,
    /// Not used: page backgrounds and form fields draw in the colour
    /// alone.
    Ignored,
}

struct Checker<'a> {
    path: String,
    errors: Vec<ValidationError>,
    declared: BTreeSet<&'a str>,
    fonts: &'a FontStore,
    /// Font ids that may be missing from `fonts`.
    deferred: &'a [String],
    images: Option<&'a ImageStore>,
    /// The files attachments name, in a render.
    attachments: Option<&'a AttachmentStore>,
    /// Checking for a render, which knows its options.
    rendering: bool,
    /// The output rendered: plain PDF or a PDF/A level, and PDF/UA-1 or
    /// not. At any PDF/A level a page that is too small is recorded, at
    /// PDF/A-1b the first use of transparency, and under PDF/UA-1 the first
    /// thing it lacks.
    profile: Profile,
    /// Inside so many nodes that tagged output leaves out as decoration
    /// (painted on every page, or with the role `artifact`).
    decoration: u32,
    /// The level of the last heading, in document order.
    last_heading: Option<u8>,
    /// Where the node checked is in the structure of tagged output.
    structure: tagging::Structure,
    doc: &'a Document,
    /// Ids that `#id` links can reach, collected on the first such link.
    anchors: Option<HashSet<&'a str>>,
    /// Inside a header or footer (at any depth).
    in_fixed: bool,
    /// Form field names seen so far, with their kind.
    fields: HashMap<String, FieldKind>,
    /// The first thing the PDF/A level does not allow.
    conflict: Option<ValidationError>,
    /// What needs transparency in each image checked (by content), under
    /// PDF/A-1b: an image used many times is scanned once.
    image_transparency: HashMap<ContentKey<Arc<[u8]>>, Option<&'static str>>,
    /// The groups field names make: every name before one of its dots
    /// (`a` and `a.b` for `a.b.c`).
    field_groups: HashSet<String>,
    /// The radio groups that have a checked button so far.
    checked_radios: HashSet<String>,
    /// The different `fontFeatures` of runs seen so far, sorted by tag.
    feature_sets: HashSet<Vec<FontFeature>>,
    /// Grid slots (rows times columns) of the tables seen so far.
    table_slots: usize,
}

// The bounds below, up to `MAX_FLOATS`, are the schema's, and so are
// `MAX_PAGE_EDGE`, `MAX_PERCENT` and `MAX_NUMBER` above:
// `schema::parity_tests` reads each from `document.v1.json` and checks
// that validation enforces it.

/// Longest link target accepted.
pub(crate) const MAX_LINK: usize = 2048;

/// Longest `Id` (node, font and image ids), in characters.
const MAX_ID: usize = 256;

/// Font weights are `1..=MAX_WEIGHT` (`FontDecl.weight`).
pub(crate) const MAX_WEIGHT: u16 = 1000;
/// Longest form field `name`, in characters.
pub(crate) const MAX_FIELD_NAME: usize = 512;
/// Longest part of a form field `name` between dots, in characters.
pub(crate) const MAX_FIELD_PART_LEN: usize = 128;
/// Most parts of a form field `name`: groups nest at most one less deep.
pub(crate) const MAX_FIELD_PARTS: usize = 32;
/// Most `options` of a form field.
pub(crate) const MAX_FIELD_OPTIONS: usize = 1000;
/// Longest option of a form field, in characters.
pub(crate) const MAX_FIELD_OPTION_LEN: usize = 1000;
/// Longest form field `value`, in characters (PDF/A-2 holds strings to
/// 32767 bytes; a character is at most 4 of them in UTF-16).
pub(crate) const MAX_FIELD_VALUE: usize = 8000;
/// Longest `contents` of a note, in characters (PDF/A-2 holds strings to
/// 32767 bytes).
pub(crate) const MAX_NOTE: usize = 8000;
/// Longest `author` of a note, in characters.
pub(crate) const MAX_NOTE_AUTHOR: usize = 256;
/// Largest font size of a text run or a form field, in points.
pub(crate) const MAX_FONT_SIZE: f32 = 1000.0;
/// Longest bookmark title, in characters.
pub(crate) const MAX_BOOKMARK_TITLE: usize = 1024;
/// Deepest bookmark `level`.
pub(crate) const MAX_BOOKMARK_LEVEL: u8 = 32;
/// Longest alternative text of an inline image, in characters.
pub(crate) const MAX_ALT: usize = 1024;
/// Most entries of `fallbackFonts`.
pub(crate) const MAX_FALLBACK_FONTS: usize = 256;
/// Most entries of `emojiImages`.
pub(crate) const MAX_EMOJI_IMAGES: usize = 4096;
/// Longest key of `emojiImages`, in bytes (UTF-8). The schema's
/// `maxLength`, in characters, is the looser bound JSON Schema can state.
pub(crate) const MAX_EMOJI_SEQUENCE: usize = 64;
/// Thickest `decorationThickness`, in points.
pub(crate) const MAX_DECORATION_THICKNESS: f32 = 100.0;
/// Largest `letterSpacing` either way, in points.
pub(crate) const MAX_LETTER_SPACING: f32 = 1000.0;
/// Largest `lineHeight` multiplier.
pub(crate) const MAX_LINE_HEIGHT: f32 = 100.0;
/// Largest `textIndent` either way, in points.
pub(crate) const MAX_TEXT_INDENT: f32 = 10_000.0;
/// Largest `hyphenationPenalty`: from it on, a hyphenation point is no
/// break (TeX's infinite penalty).
pub(crate) const MAX_HYPHENATION_PENALTY: f32 = 10_000.0;
/// Most corners of a `shapeOutside` polygon.
pub(crate) const MAX_POLYGON_POINTS: usize = 1000;
/// Largest `orphans` and `widows`.
pub(crate) const MAX_ORPHANS: u32 = 100;
/// Most `fontFeatures` of a run.
pub(crate) const MAX_FONT_FEATURES: usize = 32;
/// Most axis values a font declaration's `variations` takes.
pub(crate) const MAX_VARIATIONS: usize = 16;
/// Largest axis value, either way: the range of the 16.16 numbers `fvar`
/// stores.
pub(crate) const MAX_VARIATION_VALUE: f32 = 32_767.0;
/// Most font declarations with `variations`: each embeds a program.
pub(crate) const MAX_FONT_INSTANCES: usize = 64;
/// Largest value of a font feature.
pub(crate) const MAX_FEATURE_VALUE: u32 = 65_535;

// The bounds below are the engine's own: the schema does not state them,
// and `validate::tests` checks each.

/// Most floated children of one box: placing each looks at the others.
pub(crate) const MAX_FLOATS: usize = 1000;

/// Most different sets of `fontFeatures` in a document: each is shaped
/// with plans and words of its own.
pub(crate) const MAX_FEATURE_SETS: usize = 64;

/// Most grid slots (rows times columns) of the tables of a document, in
/// all: table layout keeps a few values per slot, whatever the spans.
pub(crate) const MAX_TABLE_SLOTS: usize = 1_000_000;

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
const WHOLE: &str = "a forced page break does nothing inside a row, grid, wrapping or reversed box, or inside an absolutely positioned, rotated, transformed, floated or fixed node";

/// Where a node sits; some node types are only valid in the flow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// A child of the page: in the flow, or an overlay.
    Page,
    Flow,
    /// In flowing content that does not split at forced breaks: inside a
    /// row, grid, wrapping or reversed box, or inside an absolutely positioned,
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

    /// Under `opaque`, records `field` (of the current path) as the first
    /// use of transparency, unless there is one already. `what` needs it.
    fn transparent(&mut self, field: Option<&str>, what: &str) {
        if self.profile.transparency() || self.conflict.is_some() {
            return;
        }
        let mut path = self.path.clone();
        if let Some(f) = field {
            push_key(&mut path, f);
        }
        self.conflict = Some(transparency_conflict(path, what));
    }

    /// `debug` draws translucent outlines.
    fn debug(&mut self, debug: bool) {
        if debug {
            self.transparent(Some("debug"), "`debug` (its outlines are translucent)");
        }
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
        if self.profile.ua() {
            for (field, missing) in [
                ("title", doc.meta.title.as_deref().is_none_or(str::is_empty)),
                ("lang", doc.meta.lang.as_deref().is_none_or(str::is_empty)),
            ] {
                if missing && self.conflict.is_none() {
                    self.conflict = Some(ValidationError {
                        code: Code::UaConflict,
                        path: format!("meta.{field}"),
                        message: format!("PDF/UA requires the document's {field}"),
                    });
                }
            }
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
                    let (field, source) = match &f.source {
                        Some(source) => (Some("source"), source),
                        None => (Some("id"), &f.id),
                    };
                    if f.source.as_ref().is_some_and(|s| !c.id(Some("source"), s)) {
                        // Reported by `id`.
                    } else if !c.fonts.contains(source) && !c.deferred.contains(source) {
                        c.err(
                            Code::FontNotRegistered,
                            field,
                            format!("font `{source}` is declared but no font data was registered under that id"),
                        );
                    }
                    if let Some(v) = &f.variations {
                        c.variations(v);
                    }
                });
            });
            self.declared.insert(&f.id);
        }
        let instances = doc.fonts.iter().filter(|f| f.variations.is_some()).count();
        if instances > MAX_FONT_INSTANCES {
            self.err(
                Code::OutOfRange,
                Some("fonts"),
                format!("at most {MAX_FONT_INSTANCES} fonts declare variations; this document has {instances}"),
            );
        }
        if doc.fallback_fonts.len() > MAX_FALLBACK_FONTS {
            self.err(
                Code::OutOfRange,
                Some("fallbackFonts"),
                format!("at most {MAX_FALLBACK_FONTS} fallback fonts"),
            );
        } else {
            for (i, id) in doc.fallback_fonts.iter().enumerate() {
                self.at("fallbackFonts", |c| {
                    c.at_index(i, |c| c.font_ref(None, id));
                });
            }
        }
        if doc.emoji_images.len() > MAX_EMOJI_IMAGES {
            self.err(
                Code::OutOfRange,
                Some("emojiImages"),
                format!("at most {MAX_EMOJI_IMAGES} emoji images"),
            );
        } else {
            for (sequence, id) in &doc.emoji_images {
                self.at("emojiImages", |c| c.emoji_image(sequence, id));
            }
        }
        self.attachments(&doc.attachments);
        if let Some(schemas) = &doc.meta.xmp_extensions {
            self.at("meta", |c| {
                c.at("xmpExtensions", |c| c.xmp_extensions(schemas));
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
        if self.profile.is_pdf_a()
            && self.conflict.is_none()
            && s.width.min(s.height) < PDF_A_MIN_PAGE_EDGE
        {
            self.conflict = Some(ValidationError {
                code: Code::PdfaConflict,
                path: format!("{}.size", self.path),
                message: format!(
                    "PDF/A does not allow a page side under {PDF_A_MIN_PAGE_EDGE} points, and the page is {} × {}",
                    s.width, s.height
                ),
            });
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
            self.color_as(Some("background"), bg, Alpha::Ignored);
        }
        self.debug(page.debug);
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
        self.floats(parent, children);
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

    /// Checks the floated children of a box with `parent` style (`None`:
    /// the page): they float in a column only, a box holds at most
    /// [`MAX_FLOATS`], and a box with floats lays its children out in
    /// order, so it takes no `alignContent`, they no `flexBasis`, and a
    /// float, shrink-to-fit, no `flexGrow`.
    fn floats(&mut self, parent: Option<&Style>, children: &[Node]) {
        let floated = |n: &Node| {
            n.style.floats().is_some() && n.style.display != Some(crate::schema::Display::None)
        };
        let count = children.iter().filter(|n| floated(n)).count();
        if count == 0 {
            return;
        }
        if count > MAX_FLOATS {
            self.err(
                Code::OutOfRange,
                Some("children"),
                format!("a box holds at most {MAX_FLOATS} floated children"),
            );
        }
        let column = parent.is_none_or(Style::column);
        if column && parent.is_some_and(|p| p.align_content.is_some()) {
            self.at("style", |c| {
                c.err(
                    Code::StyleNotApplicable,
                    Some("alignContent"),
                    "`alignContent` does not apply to a box with floated children: they stack in one column",
                );
            });
        }
        self.at("children", |c| {
            for (i, n) in children.iter().enumerate() {
                c.at_index(i, |c| {
                    let s = &n.style;
                    if column {
                        let problem = if s.flex_basis.is_some() {
                            Some(("flexBasis", "`flexBasis` does not apply in a box with floated children"))
                        } else if s.flex_grow.is_some() && floated(n) {
                            Some(("flexGrow", "a float does not grow: it is as wide and as tall as its content"))
                        } else {
                            None
                        };
                        if let Some((name, message)) = problem {
                            c.at("style", |c| c.err(Code::StyleNotApplicable, Some(name), message));
                        }
                    }
                    if !floated(n) {
                        return;
                    }
                    let problem = if !column {
                        Some("floats only apply to children of a column box, not of a row, grid, wrapping or reversed box")
                    } else if s.position == Some(Position::Absolute) {
                        Some("a float cannot be absolutely positioned")
                    } else if n.brk.before == BreakMode::Page || n.brk.after == BreakMode::Page {
                        Some("a float cannot force a page break")
                    } else {
                        None
                    };
                    if let Some(message) = problem {
                        c.at("style", |c| c.err(Code::InvalidValue, Some("float"), message));
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
        let decoration = node.is_decoration();
        self.decoration += u32::from(decoration);
        if self.profile.ua() {
            self.ua_node(node, !self.in_fixed && self.decoration == 0);
        }
        let outer = self.structure;
        self.structure = outer.enter(node);
        self.node_body(node, ctx);
        self.structure = outer;
        self.decoration -= u32::from(decoration);
    }

    fn node_body(&mut self, node: &Node, ctx: Ctx) {
        if let Some(id) = &node.id {
            self.id(Some("id"), id);
        }
        self.debug(node.debug);
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
            && (crate::paginate::moves_whole(node) || crate::paginate::banded(node))
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
        if let Some(bg) = &node.background_image {
            self.at("backgroundImage", |c| {
                c.object_position(&bg.object_position);
                c.image_source(&bg.source);
            });
        }

        if let Some(role) = node.role {
            self.role(role, &node.kind);
        }

        match &node.kind {
            NodeKind::Box { children, alt } => {
                self.children(Some(&node.style), children, inner);
                self.alt(alt.as_deref());
                if alt.is_some() && node.role != Some(crate::schema::Role::Figure) {
                    self.err(
                        Code::InvalidValue,
                        Some("alt"),
                        "only a box with the role `figure` takes `alt`",
                    );
                }
            }
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
                alt,
                ..
            } => {
                self.object_position(object_position);
                self.image_source(source);
                self.alt(alt.as_deref());
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
            NodeKind::Note(n) => self.at("note", |c| c.note(n)),
            NodeKind::Attachment(a) => self.at("attachment", |c| c.attachment(a)),
        }
    }
}
