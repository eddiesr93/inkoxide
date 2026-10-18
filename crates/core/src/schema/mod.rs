//! Serde mirror of `packages/schema/document.v1.json`.
//!
//! The JSON Schema is the source of truth; these structs are checked against
//! it in CI (every fixture must pass both). Structural rules that serde can
//! express (unknown fields, types, enums) are enforced here, with a few
//! ranges that belong to a value's own type (`de`, `areas`); the other
//! semantic rules (ranges, references, applicability) live in `validate`.

mod areas;
mod de;
#[cfg(test)]
mod parity_tests;

pub use areas::{is_area_name, parse_areas};

use serde::Deserialize;

use crate::images::InlineData;

pub use de::{GridLine, GridTrack, Length, LengthPercent, RepeatCount, TrackBreadth, TrackSize};

/// The document `version` this engine reads.
pub const SCHEMA_VERSION: u32 = 1;

/// RGBA, each component in [0, 1].
pub type Color = [f32; 4];

/// The default colour of text, borders and form fields.
pub const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Document {
    #[serde(deserialize_with = "de::int")]
    pub version: u32,
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub fonts: Vec<FontDecl>,
    #[serde(default)]
    pub fallback_fonts: Vec<String>,
    /// Emoji sequences drawn as images: sequence → image id.
    #[serde(default)]
    pub emoji_images: std::collections::BTreeMap<String, String>,
    /// Files the PDF carries, each written once.
    #[serde(default)]
    pub attachments: Vec<AttachmentDecl>,
    pub pages: Vec<PageSpec>,
}

/// A file the PDF carries (`attachments`): listed in the viewer's
/// attachments panel, and opened from the icons of `attachment` nodes and
/// runs that name it.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "de::RawAttachmentDecl")]
pub struct AttachmentDecl {
    /// The file name, unique in the document.
    pub name: String,
    pub source: crate::attachments::FileSource,
    /// Default `application/octet-stream`.
    pub mime_type: Option<String>,
    pub description: Option<String>,
    pub relationship: Relationship,
    /// RFC 3339 timestamp; default: the creation date, if any.
    pub modified: Option<String>,
}

/// How a file relates to the document (`/AFRelationship`, which PDF/A-3
/// requires).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Relationship {
    /// The original the document was made from.
    Source,
    /// The data the document shows (an invoice's XML).
    Data,
    /// Another form of the same content.
    Alternative,
    /// Something that adds to the content.
    Supplement,
    #[default]
    Unspecified,
}

/// An `attachment` node's or run's icon: it opens file `file`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Attachment {
    /// The `name` of a declared attachment.
    pub file: String,
    #[serde(default)]
    pub icon: FileIcon,
    /// The icon's colour (its alpha is not used); default
    /// [`ATTACHMENT_BLUE`].
    pub color: Option<Color>,
    /// The text shown with the icon; default the file's description or
    /// name.
    pub contents: Option<String>,
}

/// The icon of an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileIcon {
    #[default]
    PushPin,
    Paperclip,
    Graph,
    Tag,
}

/// The default colour of an attachment icon.
pub const ATTACHMENT_BLUE: Color = [0.3, 0.55, 0.9, 1.0];

/// An XMP schema the PDF/A metadata declares and fills in
/// (`meta.xmpExtensions`), such as Factur-X's: PDF/A accepts properties
/// outside its own schemas only when they are declared.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct XmpSchema {
    /// What the schema is (`pdfaSchema:schema`).
    pub schema: String,
    pub namespace: String,
    pub prefix: String,
    pub properties: Vec<XmpProperty>,
}

/// A Text property of an [`XmpSchema`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct XmpProperty {
    pub name: String,
    pub value: String,
    /// What the property is (`pdfaProperty:description`).
    pub description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Meta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    /// Default `Inkoxide <version>`.
    pub producer: Option<String>,
    pub lang: Option<String>,
    pub creation_date: Option<String>,
    /// Default: `useOutlines` when the document has bookmarks.
    pub page_mode: Option<PageMode>,
    /// Default: not written (the viewer's choice).
    pub page_layout: Option<PageLayout>,
    /// XMP schemas the PDF/A metadata declares and fills in.
    pub xmp_extensions: Option<Vec<XmpSchema>>,
}

/// What the viewer shows when it opens the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageMode {
    UseNone,
    UseOutlines,
    UseThumbs,
    FullScreen,
    /// The attachments panel.
    UseAttachments,
}

/// How the viewer lays out the pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageLayout {
    SinglePage,
    OneColumn,
    TwoColumnLeft,
    TwoColumnRight,
    TwoPageLeft,
    TwoPageRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FontDecl {
    pub id: String,
    /// Text runs name fonts by `id`; the family, weight and style are what
    /// `FontSet::select` matches (checked to be non-empty).
    pub family: String,
    #[serde(default = "default_weight", deserialize_with = "de::int")]
    pub weight: u16,
    #[serde(default)]
    pub style: FontStyle,
    /// The registered font it draws with; `id` when absent. One variable
    /// font can then serve several declarations.
    pub source: Option<String>,
    /// A variable font's axis values (user space), in the order written:
    /// the static instance it draws with.
    #[serde(default, deserialize_with = "de::font_variations")]
    pub variations: Option<Box<[FontVariation]>>,
}

impl FontDecl {
    /// The id of the registered font it draws with.
    pub fn source(&self) -> &str {
        self.source.as_deref().unwrap_or(&self.id)
    }
}

/// One axis value of a [`FontDecl`]'s `variations`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontVariation {
    /// Four printable ASCII characters, once validated.
    pub tag: String,
    pub value: f32,
}

fn default_weight() -> u16 {
    400
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edges<T: Default> {
    #[serde(default)]
    pub top: T,
    #[serde(default)]
    pub right: T,
    #[serde(default)]
    pub bottom: T,
    #[serde(default)]
    pub left: T,
}

impl<T: Default + Copy> Edges<T> {
    /// The same value on every side.
    pub fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// `[top, right, bottom, left]`, for loops over the sides.
    pub fn to_array(self) -> [T; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}

impl<T: Default> From<[T; 4]> for Edges<T> {
    /// From `[top, right, bottom, left]`.
    fn from([top, right, bottom, left]: [T; 4]) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PageSpec {
    pub size: Size,
    #[serde(default)]
    pub margin: Edges<f32>,
    pub background: Option<Color>,
    /// Draws the boxes of the page and of every element on it.
    #[serde(default)]
    pub debug: bool,
    pub header: Option<Box<Node>>,
    pub footer: Option<Box<Node>>,
    #[serde(default, deserialize_with = "de::shrunk")]
    pub children: Vec<Node>,
}

impl PageSpec {
    /// Width and height of the content box: the page less its margins.
    /// The flow and the overlays are laid out against it.
    pub fn content_size(&self) -> (f32, f32) {
        let m = self.margin;
        (
            self.size.width - m.left - m.right,
            self.size.height - m.top - m.bottom,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakMode {
    #[default]
    Auto,
    Page,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsideMode {
    #[default]
    Auto,
    Avoid,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Break {
    #[serde(default)]
    pub before: BreakMode,
    #[serde(default)]
    pub after: BreakMode,
    #[serde(default)]
    pub inside: InsideMode,
    pub min_presence_ahead: Option<f32>,
    /// Painted on every page of the flow. An absolutely positioned page
    /// child is an overlay, placed against the page content box (see
    /// `pipeline`); any other fixed node keeps its place in the flow.
    #[serde(default)]
    pub fixed: bool,
}

/// A node of the document tree. Built from the flat [`de::RawNode`] so that
/// field errors keep precise paths (serde's internally tagged enums buffer
/// their content and lose both the path and some speed).
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "de::RawNode")]
pub struct Node {
    pub id: Option<String>,
    pub style: NodeStyle,
    pub brk: Break,
    /// Link target: a URI, or `#id` of a node (boxed: rare, and nodes are
    /// numerous).
    pub link: Option<Box<str>>,
    pub bookmark: Option<Box<Bookmark>>,
    /// Pages the node is painted on (boxed: rare). It takes its space in
    /// the layout on every page.
    pub pages: Option<Box<PageFilter>>,
    /// Draws the node's boxes over the page (a development aid).
    pub debug: bool,
    /// An image behind a box's children (boxed: rare). Only boxes take one.
    pub background_image: Option<Box<BackgroundImage>>,
    /// What the node is in tagged output (a heading, a list item…).
    pub role: Option<Role>,
    pub kind: NodeKind,
}

/// What a node is in the structure of tagged output: a structure element
/// of PDF 1.7, or decoration (`Artifact`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    P,
    Section,
    BlockQuote,
    Caption,
    Figure,
    List,
    ListItem,
    Label,
    ListBody,
    /// Decoration: neither it nor its content is read.
    Artifact,
}

impl Role {
    /// The heading level, for a heading.
    pub fn heading(self) -> Option<u8> {
        match self {
            Self::H1 => Some(1),
            Self::H2 => Some(2),
            Self::H3 => Some(3),
            Self::H4 => Some(4),
            Self::H5 => Some(5),
            Self::H6 => Some(6),
            _ => None,
        }
    }
}

impl Node {
    /// Whether tagged output leaves it out as decoration: painted on every
    /// page, or with the role `artifact`.
    pub fn is_decoration(&self) -> bool {
        self.brk.fixed || self.role == Some(Role::Artifact)
    }

    /// Whether `display: none` hides the node.
    pub fn hidden(&self) -> bool {
        self.style.display == Some(Display::None)
    }

    /// The nodes one level down, in document order: a box's children, or
    /// the contents of a table's cells, row by row. Other kinds have none.
    pub fn children(&self) -> impl Iterator<Item = &Node> {
        let (children, rows) = self.parts();
        children
            .iter()
            .chain(rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.children))
    }

    /// [`Node::children`] that are displayed: without the nodes, and the
    /// contents of the cells, that `display: none` hides.
    pub fn shown_children(&self) -> impl Iterator<Item = &Node> {
        let (children, rows) = self.parts();
        let cells = rows
            .iter()
            .flat_map(|r| &r.cells)
            .filter(|c| c.style.display != Some(Display::None));
        children
            .iter()
            .chain(cells.flat_map(|c| &c.children))
            .filter(|n| !n.hidden())
    }

    /// A box's children, or a table's rows.
    fn parts(&self) -> (&[Node], &[TableRow]) {
        match &self.kind {
            NodeKind::Box { children, .. } => (children, &[]),
            NodeKind::Table(t) => (&[], &t.rows),
            _ => (&[], &[]),
        }
    }
}

/// Which pages show a node, by physical page number (as `pageNumber`).
#[derive(Debug, Clone, PartialEq)]
pub enum PageFilter {
    Pattern(PagePattern),
    /// 1-based page numbers; negative numbers count from the end (-1 is
    /// the last page).
    List(Box<[i32]>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PagePattern {
    First,
    Last,
    NotFirst,
    NotLast,
    Odd,
    Even,
}

impl PageFilter {
    /// Whether page `n` of `total` (1-based) shows the node.
    pub fn shows(&self, n: u32, total: u32) -> bool {
        match self {
            Self::Pattern(p) => match p {
                PagePattern::First => n == 1,
                PagePattern::Last => n == total,
                PagePattern::NotFirst => n != 1,
                PagePattern::NotLast => n != total,
                PagePattern::Odd => !n.is_multiple_of(2),
                PagePattern::Even => n.is_multiple_of(2),
            },
            Self::List(list) => list.iter().any(|&k| {
                let k = if k < 0 {
                    i64::from(total) + 1 + i64::from(k)
                } else {
                    i64::from(k)
                };
                k == i64::from(n)
            }),
        }
    }
}

/// An outline entry pointing at the top of its node.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bookmark {
    pub title: String,
    /// Nesting level, 1 = top; an entry nests under the nearest earlier
    /// entry of a lower level.
    #[serde(default = "one_u8", deserialize_with = "de::int")]
    pub level: u8,
}

fn one_u8() -> u8 {
    1
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Box {
        children: Vec<Node>,
        /// A `figure` box's alternative text.
        alt: Option<String>,
    },
    Text {
        runs: Vec<TextRun>,
    },
    Image {
        source: ImageSource,
        fit: ImageFit,
        object_position: ObjectPosition,
        /// What text extraction gets for the image.
        alt: Option<String>,
    },
    Table(Table),
    PageBreak,
    Spacer,
    /// An interactive form field (boxed: large and rare).
    Field(Box<Field>),
    /// A text annotation: an icon that opens a note (boxed: rare).
    Note(Box<Note>),
    /// An icon that opens an attachment (boxed: rare).
    Attachment(Box<Attachment>),
}

/// A text annotation, shown as a sticky-note icon.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub contents: String,
    pub author: Option<String>,
    /// The icon's colour (its alpha is not used); default [`NOTE_YELLOW`].
    pub color: Option<Color>,
    /// Shown open when the document opens.
    #[serde(default)]
    pub open: bool,
}

/// The default colour of a note.
pub const NOTE_YELLOW: Color = [1.0, 0.82, 0.2, 1.0];

/// An `AcroForm` field. Fields sharing a `name` are one field with several
/// widgets (radio buttons: one per option).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Field {
    /// The full name: dots separate the groups it is in (`a.b.c`).
    pub name: String,
    pub kind: FieldKind,
    /// Text: the initial text. Select and list: the selected option (or
    /// options: a list with `multi_select`). Radio: the value this button
    /// exports when selected.
    pub value: Option<FieldValue>,
    /// Checkbox and radio: initially on. At most one button of a radio
    /// group; buttons of the group with the same value are on together.
    #[serde(default)]
    pub checked: bool,
    /// Select and list: the choices. Set, it holds at least one.
    #[serde(default, deserialize_with = "de::non_empty")]
    pub options: Vec<String>,
    /// List: several options can be selected.
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub password: bool,
    #[serde(default, deserialize_with = "de::opt_int")]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub required: bool,
    /// Text, select and list: a declared font id.
    pub font: Option<String>,
    #[serde(default = "default_field_size")]
    pub size: f32,
    pub color: Option<Color>,
    /// Text: how the value shows and is typed (boxed: rare).
    pub format: Option<Box<FieldFormat>>,
    /// What the field is for, which screen readers say (`/TU`).
    pub label: Option<String>,
}

impl Field {
    /// The value when it is one string.
    pub fn value_str(&self) -> Option<&str> {
        match &self.value {
            Some(FieldValue::One(v)) => Some(v),
            _ => None,
        }
    }

    /// Every value: none, the one string, or the strings of an array.
    pub fn values(&self) -> &[String] {
        match &self.value {
            None => &[],
            Some(FieldValue::One(v)) => std::slice::from_ref(v),
            Some(FieldValue::Many(v)) => v,
        }
    }

    /// Whether the field shows text: a text field, or a choice of options.
    pub fn is_text_like(&self) -> bool {
        matches!(
            self.kind,
            FieldKind::Text | FieldKind::Select | FieldKind::List
        )
    }
}

/// A field `value`: a string, or the options selected in a list that
/// takes several (read as a string or an array in `de.rs`).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    One(String),
    Many(Vec<String>),
}

fn default_field_size() -> f32 {
    10.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    Checkbox,
    Radio,
    Select,
    /// A list box: several options in view, and possibly selected.
    List,
}

/// How a text field shows and takes its value: a number, a date, a postal
/// code… Viewers apply it as the user types, through the standard form
/// scripts.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FieldFormat {
    #[serde(rename = "type")]
    pub kind: FormatKind,
    /// Number and percent: digits after the decimal separator (default 2).
    #[serde(default, deserialize_with = "de::opt_int")]
    pub decimals: Option<u8>,
    /// Number and percent: the separators (default `1,234.56`).
    pub separators: Option<Separators>,
    /// Number: how negative numbers show (default a minus sign).
    pub negative: Option<NegativeStyle>,
    /// Number: a currency symbol.
    pub currency: Option<String>,
    /// Number: the currency goes before the number (default `true`).
    pub currency_before: Option<bool>,
    /// Date and time: how the value shows (`dd/mm/yyyy`, `HH:MM`).
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatKind {
    Number,
    Percent,
    Date,
    Time,
    Zip,
    ZipPlus4,
    Phone,
    Ssn,
}

/// The thousands and decimal separators of a number, by example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Separators {
    #[default]
    #[serde(rename = "1,234.56")]
    CommaDot,
    #[serde(rename = "1234.56")]
    Dot,
    #[serde(rename = "1.234,56")]
    DotComma,
    #[serde(rename = "1234,56")]
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NegativeStyle {
    /// `-1,234.56`.
    #[default]
    Minus,
    /// `1,234.56` in red.
    Red,
    /// `(1,234.56)`.
    Parens,
    /// `(1,234.56)` in red.
    ParensRed,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Key into the render-time image map.
    Ref(String),
    /// Inline base64 payload, decoded once, when validation first reads it.
    Inline(InlineData),
}

/// A box's `backgroundImage`: painted into its padding box, over its
/// `backgroundColor` and under its borders and children.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "de::RawBackgroundImage")]
pub struct BackgroundImage {
    pub source: ImageSource,
    pub fit: ImageFit,
    pub object_position: ObjectPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
}

/// Where a fitted image sits in its box (CSS `object-position`), per axis:
/// points from the left or top edge, or a fraction of the free space (the
/// keywords are 0, ½ and 1).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectPosition {
    #[serde(default = "centre", deserialize_with = "de::position_x")]
    pub x: LengthPercent,
    #[serde(default = "centre", deserialize_with = "de::position_y")]
    pub y: LengthPercent,
}

fn centre() -> LengthPercent {
    LengthPercent::Percent(0.5)
}

impl ObjectPosition {
    /// Centred on both axes: the default.
    pub const CENTER: Self = Self {
        x: LengthPercent::Percent(0.5),
        y: LengthPercent::Percent(0.5),
    };
}

impl Default for ObjectPosition {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decoration {
    None,
    Underline,
    LineThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placeholder {
    PageNumber,
    TotalPages,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextRun {
    pub text: Option<String>,
    pub font: String,
    pub size: f32,
    pub color: Option<Color>,
    pub letter_spacing: Option<f32>,
    pub decoration: Option<Decoration>,
    /// Decoration colour; default the run colour.
    pub decoration_color: Option<Color>,
    /// Decoration thickness in points; default from the font.
    pub decoration_thickness: Option<f32>,
    pub decoration_style: Option<DecorationStyle>,
    pub placeholder: Option<Placeholder>,
    /// Link target of this run (as `Node::link`).
    pub link: Option<String>,
    /// Overrides the paragraph's `hyphenate` for this run's words.
    pub hyphenate: Option<bool>,
    /// Where the run sits on its line; default its baseline on the line's.
    pub vertical_align: Option<RunAlign>,
    /// OpenType features to turn on or off, in the order written.
    #[serde(default, deserialize_with = "de::font_features")]
    pub font_features: Option<Box<[FontFeature]>>,
    /// An image in the line, in place of text (boxed: rare).
    pub image: Option<Box<RunImage>>,
    /// A note in the line, in place of text (boxed: rare): it takes no
    /// room, and its icon hangs from the line's top where the run is.
    pub note: Option<Box<Note>>,
    /// An attachment's icon in the line, as a `note` is.
    pub attachment: Option<Box<Attachment>>,
}

impl TextRun {
    /// The icon (a note or an attachment's) it is in place of text, which
    /// takes no room in its line.
    pub(crate) fn icon(&self) -> Option<crate::icons::Icon<'_>> {
        use crate::icons::Icon;
        self.note
            .as_deref()
            .map(Icon::Note)
            .or_else(|| self.attachment.as_deref().map(Icon::Attachment))
    }

    /// Whether it is an icon in place of text ([`icon`](Self::icon)).
    pub fn is_icon(&self) -> bool {
        self.note.is_some() || self.attachment.is_some()
    }
}

/// A run's `image`: drawn in the line like a glyph, `width` × `height`
/// points (one of them, or both, from the image's aspect ratio and the
/// run's size).
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "de::RawRunImage")]
pub struct RunImage {
    pub source: ImageSource,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub fit: ImageFit,
    pub object_position: ObjectPosition,
    /// What text extraction gets for the image.
    pub alt: Option<String>,
}

/// Where a run sits on its line (CSS `vertical-align` on an inline box):
/// its baseline on the line's, or moved up or down. The values after
/// `Sub` place images (validation keeps them to image runs).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RunAlign {
    #[default]
    Baseline,
    /// Raised by the font's superscript offset.
    Super,
    /// Lowered by the font's subscript offset.
    Sub,
    /// The middle at the baseline plus half the font's x-height.
    Middle,
    /// The top at the font's ascender.
    TextTop,
    /// The bottom at the font's descender.
    TextBottom,
    /// The top at the line's top.
    Top,
    /// The bottom at the line's bottom.
    Bottom,
    /// The bottom this many points above the baseline (negative: below).
    Points(f32),
}

/// An OpenType feature setting (CSS `font-feature-settings`): 0 turns the
/// feature off, 1 on, and a larger value picks an alternate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontFeature {
    /// Four printable ASCII characters, once validated.
    pub tag: String,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Points(f32),
    Fr(f32),
    Auto,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnSpec {
    pub width: ColumnWidth,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TableCell {
    /// Part of the schema (checked like any id); cells are not link targets.
    pub id: Option<String>,
    /// Draws the cell's boxes over the page.
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "one", deserialize_with = "de::int")]
    pub col_span: u32,
    #[serde(default = "one", deserialize_with = "de::int")]
    pub row_span: u32,
    #[serde(default)]
    pub style: NodeStyle,
    #[serde(default, deserialize_with = "de::shrunk")]
    pub children: Vec<Node>,
    /// A header of its row, in tagged output (`TH`, scope row); the cells
    /// of the header rows are column headers.
    #[serde(default)]
    pub header: bool,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TableRow {
    /// Part of the schema (checked like any id); rows are not link targets.
    pub id: Option<String>,
    /// Draws the row's box over the page.
    #[serde(default)]
    pub debug: bool,
    pub min_height: Option<f32>,
    pub background_color: Option<Color>,
    #[serde(deserialize_with = "de::shrunk")]
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub columns: Vec<ColumnSpec>,
    pub header_rows: u32,
    pub rows: Vec<TableRow>,
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

macro_rules! kebab_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $s)] $variant),+
        }
    };
}

kebab_enum!(Display { Flex = "flex", Grid = "grid", None = "none" });
kebab_enum!(FlexDirection {
    Row = "row",
    Column = "column",
    RowReverse = "row-reverse",
    ColumnReverse = "column-reverse",
});
kebab_enum!(FlexWrap { NoWrap = "nowrap", Wrap = "wrap", WrapReverse = "wrap-reverse" });
kebab_enum!(JustifyContent {
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    SpaceBetween = "space-between",
    SpaceAround = "space-around",
    SpaceEvenly = "space-evenly",
    Start = "start",
    End = "end",
    Stretch = "stretch",
});
kebab_enum!(AlignItems {
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    Baseline = "baseline",
    Stretch = "stretch",
    Start = "start",
    End = "end",
});
kebab_enum!(AlignSelf {
    Auto = "auto",
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    Baseline = "baseline",
    Stretch = "stretch",
    Start = "start",
    End = "end",
});
kebab_enum!(AlignContent {
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    SpaceBetween = "space-between",
    SpaceAround = "space-around",
    SpaceEvenly = "space-evenly",
    Stretch = "stretch",
    Start = "start",
    End = "end",
});
kebab_enum!(Position { Relative = "relative", Absolute = "absolute" });
kebab_enum!(GridAutoFlow {
    Row = "row",
    Column = "column",
    RowDense = "row dense",
    ColumnDense = "column dense",
});
kebab_enum!(Overflow { Visible = "visible", Hidden = "hidden" });
kebab_enum!(Float {
    Left = "left",
    Right = "right",
    None = "none",
});
kebab_enum!(Clear {
    Left = "left",
    Right = "right",
    Both = "both",
    None = "none",
});
kebab_enum!(BorderStyle { Solid = "solid", Dashed = "dashed", Dotted = "dotted" });
kebab_enum!(TextAlign {
    Left = "left",
    Center = "center",
    Right = "right",
    Justify = "justify",
    Start = "start",
    End = "end",
});
kebab_enum!(Direction { Ltr = "ltr", Rtl = "rtl", Auto = "auto" });
kebab_enum!(DecorationStyle {
    Solid = "solid",
    Double = "double",
    Dotted = "dotted",
    Dashed = "dashed",
    Wavy = "wavy",
});
kebab_enum!(WhiteSpace {
    Normal = "normal",
    Pre = "pre",
    NoWrap = "nowrap",
    PreWrap = "pre-wrap",
    PreLine = "pre-line",
});
kebab_enum!(TextWrap { Wrap = "wrap", Pretty = "pretty", Balance = "balance" });
kebab_enum!(BorderCollapse { Collapse = "collapse", Separate = "separate" });
kebab_enum!(VerticalAlign { Top = "top", Middle = "middle", Bottom = "bottom" });

/// `borderRadius`: one radius for every corner, or one per corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderRadius {
    Uniform(f32),
    Corners(Corners),
}

/// Per-corner radii; a missing corner is square.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Corners {
    pub top_left: Option<f32>,
    pub top_right: Option<f32>,
    pub bottom_right: Option<f32>,
    pub bottom_left: Option<f32>,
}

impl Corners {
    /// Top-left, top-right, bottom-right, bottom-left.
    pub fn get(&self) -> [f32; 4] {
        [
            self.top_left.unwrap_or(0.0),
            self.top_right.unwrap_or(0.0),
            self.bottom_right.unwrap_or(0.0),
            self.bottom_left.unwrap_or(0.0),
        ]
    }
}

/// `borderStyle`: one style for every side, or one per side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyles {
    Uniform(BorderStyle),
    Sides(OptEdges<BorderStyle>),
}

impl BorderStyles {
    /// Top, right, bottom, left; unset sides are solid.
    pub fn sides(self) -> [BorderStyle; 4] {
        match self {
            BorderStyles::Uniform(s) => [s; 4],
            BorderStyles::Sides(e) => e.get(BorderStyle::Solid),
        }
    }
}

/// One transform function. Lengths are points or percentages of the
/// node's own border box; angles are degrees. Read from an object with
/// exactly one key (`de.rs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Translate([LengthPercent; 2]),
    Scale([f32; 2]),
    Rotate(f32),
    Skew([f32; 2]),
    Matrix([f32; 6]),
}

/// The shape that lines beside a float wrap around (`shapeOutside`), in
/// the float's margin box: lengths are points or percentages of it.
/// Read from an object with exactly one key (`de.rs`).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeOutside {
    Circle(ShapeCircle),
    Ellipse(ShapeEllipse),
    Inset(ShapeInset),
    /// Its corners, at least 3.
    Polygon(Vec<[LengthPercent; 2]>),
}

/// A circle: its radius (a percentage of the margin box's diagonal over
/// √2; by default to the closest side) and its centre (by default the
/// middle).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeCircle {
    pub radius: Option<LengthPercent>,
    pub at: Option<[LengthPercent; 2]>,
}

/// An ellipse: its radii (percentages of the margin box's width and
/// height; by default to the closest sides) and its centre.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeEllipse {
    pub rx: Option<LengthPercent>,
    pub ry: Option<LengthPercent>,
    pub at: Option<[LengthPercent; 2]>,
}

/// A rectangle inset from the margin box's sides (0 when not set), with
/// corners rounded by `radius` points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeInset {
    pub top: Option<LengthPercent>,
    pub right: Option<LengthPercent>,
    pub bottom: Option<LengthPercent>,
    pub left: Option<LengthPercent>,
    pub radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridPlacement {
    pub start: Option<GridLine>,
    pub end: Option<GridLine>,
    #[serde(default, deserialize_with = "de::grid_span")]
    pub span: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptEdges<T> {
    pub top: Option<T>,
    pub right: Option<T>,
    pub bottom: Option<T>,
    pub left: Option<T>,
}

impl OptEdges<LengthPercent> {
    /// Each side in points, at least 0 (0 when not set), a percentage
    /// taken of `w`, the width of the containing block (on every side, as
    /// in CSS); `None` for a percentage when `w` is unknown.
    pub fn resolve(&self, w: Option<f32>) -> Edges<Option<f32>> {
        let side = |v: Option<LengthPercent>| match v {
            None => Some(0.0),
            Some(LengthPercent::Points(x)) => Some(x.max(0.0)),
            Some(l) => w.map(|w| l.resolve(w).max(0.0)),
        };
        Edges {
            top: side(self.top),
            right: side(self.right),
            bottom: side(self.bottom),
            left: side(self.left),
        }
    }
}

impl<T: Copy> OptEdges<T> {
    pub fn get(&self, default: T) -> [T; 4] {
        [
            self.top.unwrap_or(default),
            self.right.unwrap_or(default),
            self.bottom.unwrap_or(default),
            self.left.unwrap_or(default),
        ]
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &T)> {
        [
            ("top", self.top.as_ref()),
            ("right", self.right.as_ref()),
            ("bottom", self.bottom.as_ref()),
            ("left", self.left.as_ref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
    }
}

/// The style of a node or cell, shared between nodes whose styles are
/// equal. Reads as a [`Style`].
#[derive(Debug, Clone)]
pub struct NodeStyle(std::sync::Arc<Style>);

/// Distinct styles remembered while parsing. Templates repeat a handful
/// of styles (one per table column, say), so a short list catches them.
const RECENT_STYLES: usize = 16;

thread_local! {
    static RECENT: std::cell::RefCell<Vec<std::sync::Arc<Style>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

impl NodeStyle {
    /// Shares `style` with an equal one seen recently on this thread.
    pub fn new(style: Style) -> Self {
        RECENT.with_borrow_mut(|recent| {
            if let Some(i) = recent.iter().position(|s| **s == style) {
                return Self(recent[i].clone());
            }
            let s = std::sync::Arc::new(style);
            if recent.len() == RECENT_STYLES {
                recent.remove(0);
            }
            recent.push(s.clone());
            Self(s)
        })
    }
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self::new(Style::default())
    }
}

impl std::ops::Deref for NodeStyle {
    type Target = Style;
    fn deref(&self) -> &Style {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NodeStyle {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Style::deserialize(d).map(Self::new)
    }
}

/// Every style property; `None` means "not set" (engine default applies).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Style {
    // Layout
    pub display: Option<Display>,
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_self: Option<AlignSelf>,
    pub align_content: Option<AlignContent>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Length>,
    pub gap: Option<LengthPercent>,
    pub row_gap: Option<LengthPercent>,
    pub column_gap: Option<LengthPercent>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_width: Option<Length>,
    pub max_height: Option<Length>,
    pub aspect_ratio: Option<f32>,
    pub padding: Option<OptEdges<LengthPercent>>,
    pub margin: Option<OptEdges<Length>>,
    pub position: Option<Position>,
    pub inset: Option<OptEdges<Length>>,
    pub grid_template_columns: Option<Vec<GridTrack>>,
    pub grid_template_rows: Option<Vec<GridTrack>>,
    pub grid_template_areas: Option<Box<[String]>>,
    pub grid_auto_columns: Option<Box<[TrackSize]>>,
    pub grid_auto_rows: Option<Box<[TrackSize]>>,
    pub grid_auto_flow: Option<GridAutoFlow>,
    pub grid_column: Option<GridPlacement>,
    pub grid_row: Option<GridPlacement>,
    pub grid_area: Option<Box<str>>,
    pub justify_items: Option<AlignItems>,
    pub justify_self: Option<AlignSelf>,
    pub overflow: Option<Overflow>,
    /// Floats the box to a side of its column parent: text beside it wraps,
    /// boxes beside it are narrowed.
    pub float: Option<Float>,
    /// Moves the box below the floats before it on these sides.
    pub clear: Option<Clear>,
    /// A float's shape for the lines beside it to wrap around, instead of
    /// its margin box.
    pub shape_outside: Option<ShapeOutside>,
    /// How far the lines beside a float keep from its `shapeOutside`,
    /// points.
    pub shape_margin: Option<f32>,

    // Visual
    pub background_color: Option<Color>,
    pub opacity: Option<f32>,
    pub border_width: Option<OptEdges<f32>>,
    pub border_color: Option<OptEdges<Color>>,
    pub border_radius: Option<BorderRadius>,
    pub border_style: Option<BorderStyles>,
    pub rotate: Option<f32>,
    /// CSS-style transform functions, applied left to right as in CSS
    /// (the leftmost is outermost), after `rotate`.
    pub transform: Option<Box<[TransformOp]>>,
    /// Origin of `rotate` and `transform` within the border box; default
    /// the centre.
    pub transform_origin: Option<[LengthPercent; 2]>,
    /// Paint order among siblings (stable; default 0).
    #[serde(default, deserialize_with = "de::opt_int")]
    pub z_index: Option<i32>,

    // Text (text nodes only)
    pub text_align: Option<TextAlign>,
    pub line_height: Option<f32>,
    #[serde(default, deserialize_with = "de::opt_int")]
    pub max_lines: Option<u32>,
    pub ellipsis: Option<bool>,
    pub hyphenate: Option<bool>,
    pub lang: Option<String>,
    pub white_space: Option<WhiteSpace>,
    /// How lines break: first fit (`wrap`), over the whole paragraph
    /// (`pretty`), or first fit in lines of even widths (`balance`).
    pub text_wrap: Option<TextWrap>,
    /// Demerits of a break at a hyphenation point, for `pretty` (TeX's
    /// `\hyphenpenalty`); 10 000 or more forbids it.
    pub hyphenation_penalty: Option<f32>,
    /// Indent of the first line, points (negative: hanging).
    pub text_indent: Option<f32>,
    pub direction: Option<Direction>,
    /// Minimum lines left before a page break inside the paragraph.
    #[serde(default, deserialize_with = "de::opt_int")]
    pub orphans: Option<u32>,
    /// Minimum lines carried after a page break inside the paragraph.
    #[serde(default, deserialize_with = "de::opt_int")]
    pub widows: Option<u32>,

    // Table cells only
    pub vertical_align: Option<VerticalAlign>,

    // Tables only
    pub border_collapse: Option<BorderCollapse>,
    pub border_spacing: Option<f32>,
}

impl Style {
    /// Whether `rotate` or `transform` moves the box: a rotation that is
    /// not a whole turn, or any transform function.
    pub fn is_transformed(&self) -> bool {
        self.rotate.is_some_and(|r| r % 360.0 != 0.0)
            || self.transform.as_ref().is_some_and(|t| !t.is_empty())
    }

    /// The side the box floats to, when it floats.
    pub fn floats(&self) -> Option<Float> {
        self.float.filter(|f| *f != Float::None)
    }

    /// Whether a box with this style lays its children out in one column,
    /// top to bottom: not a grid, nor a row, wrapping or reversed box.
    pub fn column(&self) -> bool {
        self.display != Some(Display::Grid)
            && matches!(self.flex_direction, None | Some(FlexDirection::Column))
            && matches!(self.flex_wrap, None | Some(FlexWrap::NoWrap))
    }

    /// Names of text-only properties that are set, for applicability checks.
    pub fn text_props_set(&self) -> impl Iterator<Item = &'static str> + '_ {
        [
            ("textAlign", self.text_align.is_some()),
            ("lineHeight", self.line_height.is_some()),
            ("maxLines", self.max_lines.is_some()),
            ("ellipsis", self.ellipsis.is_some()),
            ("hyphenate", self.hyphenate.is_some()),
            ("lang", self.lang.is_some()),
            ("whiteSpace", self.white_space.is_some()),
            ("textWrap", self.text_wrap.is_some()),
            ("hyphenationPenalty", self.hyphenation_penalty.is_some()),
            ("textIndent", self.text_indent.is_some()),
            ("direction", self.direction.is_some()),
            ("orphans", self.orphans.is_some()),
            ("widows", self.widows.is_some()),
        ]
        .into_iter()
        .filter_map(|(k, set)| set.then_some(k))
    }
}
