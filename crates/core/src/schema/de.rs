//! Hand-written deserialisers for the few schema shapes serde cannot express
//! directly (number-or-string unions, the node discriminator).
//!
//! Errors raised here carry a `[CODE]` or `[CODE@field]` prefix that
//! `validate` turns into a structured [`crate::ValidationError`].

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use super::{
    Bookmark, BorderRadius, BorderStyle, BorderStyles, Break, ColumnSpec, ColumnWidth, Corners,
    Field, ImageFit, ImageSource, Node, NodeKind, OptEdges, PageFilter, PagePattern, Style, Table,
    TableRow, TextRun,
};

/// A length: points, a fraction of the containing block, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Points(f32),
    /// Fraction in [0, 1] (`"50%"` → `0.5`).
    Percent(f32),
    Auto,
}

/// A length without `auto` (padding, gaps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthPercent {
    Points(f32),
    Percent(f32),
}

impl Default for LengthPercent {
    fn default() -> Self {
        LengthPercent::Points(0.0)
    }
}

/// One grid track sizing value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackBreadth {
    Points(f32),
    Percent(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

/// A grid track: a single breadth, `minmax(min, max)` or `fit-content(limit)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Breadth(TrackBreadth),
    MinMax(TrackBreadth, TrackBreadth),
    FitContent(LengthPercent),
}

/// The first argument of `repeat()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatCount {
    Count(u16),
    AutoFill,
    AutoFit,
}

/// An entry of `gridTemplateColumns` / `gridTemplateRows`.
#[derive(Debug, Clone, PartialEq)]
pub enum GridTrack {
    Single(TrackSize),
    Repeat(RepeatCount, Box<[TrackSize]>),
}

/// Most `repeat()` count accepted; Taffy caps a grid at 10 000 tracks anyway.
pub const MAX_REPEAT: u16 = 1000;

/// A 1-based, non-zero grid line index (negative counts from the end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLine(pub i16);

fn parse_suffixed(s: &str, suffix: &str) -> Option<f32> {
    let n = s.strip_suffix(suffix)?;
    if n.is_empty() || n.starts_with('+') || n.contains(['e', 'E']) {
        return None;
    }
    n.parse::<f32>().ok().filter(|v| v.is_finite())
}

macro_rules! num_or_str_visitor {
    ($ty:ident, $expecting:literal, |$v:ident| $num:expr, |$s:ident| $str:expr) => {
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $ty;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str($expecting)
                    }
                    fn visit_f64<E: de::Error>(self, $v: f64) -> Result<$ty, E> {
                        $num
                    }
                    fn visit_i64<E: de::Error>(self, v: i64) -> Result<$ty, E> {
                        self.visit_f64(v as f64)
                    }
                    fn visit_u64<E: de::Error>(self, v: u64) -> Result<$ty, E> {
                        self.visit_f64(v as f64)
                    }
                    fn visit_str<E: de::Error>(self, $s: &str) -> Result<$ty, E> {
                        $str.ok_or_else(|| {
                            E::custom(format_args!(
                                "[INVALID_VALUE] expected {}, found \"{}\"",
                                $expecting, $s
                            ))
                        })
                    }
                }
                d.deserialize_any(V)
            }
        }
    };
}

fn finite<E: de::Error>(v: f64) -> Result<f32, E> {
    let f = v as f32;
    if f.is_finite() {
        Ok(f)
    } else {
        Err(E::custom(
            "[OUT_OF_RANGE] number is not a finite 32-bit float",
        ))
    }
}

num_or_str_visitor!(
    Length,
    "a number of points, a percentage like \"50%\", or \"auto\"",
    |v| finite(v).map(Length::Points),
    |s| if s == "auto" {
        Some(Length::Auto)
    } else {
        parse_suffixed(s, "%").map(|p| Length::Percent(p / 100.0))
    }
);

num_or_str_visitor!(
    LengthPercent,
    "a number of points or a percentage like \"50%\"",
    |v| finite(v).map(LengthPercent::Points),
    |s| parse_suffixed(s, "%").map(|p| LengthPercent::Percent(p / 100.0))
);

impl<'de> Deserialize<'de> for BorderRadius {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = BorderRadius;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a radius in points or a per-corner object")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<BorderRadius, E> {
                finite(v).map(BorderRadius::Uniform)
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<BorderRadius, E> {
                self.visit_f64(v as f64)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<BorderRadius, E> {
                self.visit_f64(v as f64)
            }
            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<BorderRadius, A::Error> {
                Corners::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(BorderRadius::Corners)
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for BorderStyles {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = BorderStyles;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"solid\", \"dashed\", \"dotted\" or a per-side object")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<BorderStyles, E> {
                BorderStyle::deserialize(de::value::StrDeserializer::<E>::new(s))
                    .map(BorderStyles::Uniform)
            }
            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<BorderStyles, A::Error> {
                OptEdges::<BorderStyle>::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(BorderStyles::Sides)
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for PageFilter {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = PageFilter;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"first\", \"last\", \"notFirst\", \"notLast\", \"odd\", \"even\" or an array of page numbers")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<PageFilter, E> {
                PagePattern::deserialize(de::value::StrDeserializer::<E>::new(s))
                    .map(PageFilter::Pattern)
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<PageFilter, A::Error> {
                Vec::<i32>::deserialize(de::value::SeqAccessDeserializer::new(seq))
                    .map(|v| PageFilter::List(v.into_boxed_slice()))
            }
        }
        d.deserialize_any(V)
    }
}

const BREADTH: &str =
    "a number of points, a percentage, \"<n>fr\", \"auto\", \"min-content\" or \"max-content\"";

fn breadth_str(s: &str) -> Option<TrackBreadth> {
    match s {
        "auto" => Some(TrackBreadth::Auto),
        "min-content" => Some(TrackBreadth::MinContent),
        "max-content" => Some(TrackBreadth::MaxContent),
        _ => parse_suffixed(s, "fr")
            .filter(|v| *v >= 0.0)
            .map(TrackBreadth::Fr)
            .or_else(|| parse_suffixed(s, "%").map(|p| TrackBreadth::Percent(p / 100.0))),
    }
}

num_or_str_visitor!(
    TrackBreadth,
    "a number of points, a percentage, \"<n>fr\", \"auto\", \"min-content\" or \"max-content\"",
    |v| finite(v).map(TrackBreadth::Points),
    |s| breadth_str(s)
);

num_or_str_visitor!(
    RepeatCount,
    "an integer repeat count, \"auto-fill\" or \"auto-fit\"",
    |v| if v.fract() == 0.0 && (1.0..=f64::from(MAX_REPEAT)).contains(&v) {
        Ok(RepeatCount::Count(v as u16))
    } else {
        Err(de::Error::custom(format_args!(
            "[OUT_OF_RANGE] repeat count must be an integer within 1..={MAX_REPEAT}, found {v}"
        )))
    },
    |s| match s {
        "auto-fill" => Some(RepeatCount::AutoFill),
        "auto-fit" => Some(RepeatCount::AutoFit),
        _ => None,
    }
);

/// Object form of a track: `{min, max}`, `{fitContent}` or, in templates
/// only, `{repeat, tracks}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawTrack {
    min: Option<TrackBreadth>,
    max: Option<TrackBreadth>,
    fit_content: Option<LengthPercent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawTemplateTrack {
    min: Option<TrackBreadth>,
    max: Option<TrackBreadth>,
    fit_content: Option<LengthPercent>,
    repeat: Option<RepeatCount>,
    tracks: Option<Vec<TrackSize>>,
}

const TRACK_OBJECT: &str =
    "[INVALID_VALUE] a track object is either `{min, max}` or `{fitContent}`";

fn track_object(
    min: Option<TrackBreadth>,
    max: Option<TrackBreadth>,
    fit: Option<LengthPercent>,
) -> Result<TrackSize, &'static str> {
    match (min, max, fit) {
        (Some(TrackBreadth::Fr(_)), Some(_), None) => {
            Err("[INVALID_VALUE] the `min` of a minmax track cannot be a flexible (fr) size")
        }
        (Some(min), Some(max), None) => Ok(TrackSize::MinMax(min, max)),
        (None, None, Some(f)) => Ok(TrackSize::FitContent(f)),
        _ => Err(TRACK_OBJECT),
    }
}

struct TrackVisitor<const TEMPLATE: bool>;

impl<'de, const TEMPLATE: bool> Visitor<'de> for TrackVisitor<TEMPLATE> {
    type Value = GridTrack;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{BREADTH}, or a track object")
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<GridTrack, E> {
        let b = finite(v).map(TrackBreadth::Points)?;
        Ok(GridTrack::Single(TrackSize::Breadth(b)))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<GridTrack, E> {
        self.visit_f64(v as f64)
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<GridTrack, E> {
        self.visit_f64(v as f64)
    }
    fn visit_str<E: de::Error>(self, s: &str) -> Result<GridTrack, E> {
        breadth_str(s)
            .map(|b| GridTrack::Single(TrackSize::Breadth(b)))
            .ok_or_else(|| {
                E::custom(format_args!(
                    "[INVALID_VALUE] expected {BREADTH}, found \"{s}\""
                ))
            })
    }
    fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<GridTrack, A::Error> {
        let d = de::value::MapAccessDeserializer::new(map);
        if !TEMPLATE {
            let r = RawTrack::deserialize(d)?;
            return track_object(r.min, r.max, r.fit_content)
                .map(GridTrack::Single)
                .map_err(de::Error::custom);
        }
        let r = RawTemplateTrack::deserialize(d)?;
        match (r.repeat, r.tracks) {
            (None, None) => track_object(r.min, r.max, r.fit_content)
                .map(GridTrack::Single)
                .map_err(de::Error::custom),
            (Some(n), Some(t)) if r.min.is_none() && r.max.is_none() && r.fit_content.is_none() => {
                if t.is_empty() {
                    return Err(de::Error::custom(
                        "[INVALID_VALUE] `repeat` needs at least one track",
                    ));
                }
                Ok(GridTrack::Repeat(n, t.into_boxed_slice()))
            }
            _ => Err(de::Error::custom(
                "[INVALID_VALUE] a repeat object has exactly `repeat` and `tracks`",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for TrackSize {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match d.deserialize_any(TrackVisitor::<false>)? {
            GridTrack::Single(t) => Ok(t),
            GridTrack::Repeat(..) => Err(de::Error::custom(TRACK_OBJECT)),
        }
    }
}

impl<'de> Deserialize<'de> for GridTrack {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(TrackVisitor::<true>)
    }
}

impl<'de> Deserialize<'de> for GridLine {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = i64::deserialize(d)?;
        if v == 0 || v.unsigned_abs() > i16::MAX as u64 {
            return Err(de::Error::custom(
                "[OUT_OF_RANGE] grid line must be a non-zero integer within ±32767",
            ));
        }
        Ok(GridLine(v as i16))
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum RawColumnWidth {
    Num(f64),
    Str(String),
}

impl TryFrom<RawColumnWidth> for ColumnWidth {
    type Error = String;
    fn try_from(raw: RawColumnWidth) -> Result<Self, String> {
        match raw {
            RawColumnWidth::Num(v) if v > 0.0 && v.is_finite() => Ok(ColumnWidth::Points(v as f32)),
            RawColumnWidth::Num(v) => Err(format!(
                "[OUT_OF_RANGE] column width must be a positive number, found {v}"
            )),
            RawColumnWidth::Str(s) if s == "auto" => Ok(ColumnWidth::Auto),
            RawColumnWidth::Str(s) => parse_suffixed(&s, "fr")
                .filter(|v| *v > 0.0)
                .map(ColumnWidth::Fr)
                .ok_or_else(|| {
                    format!(
                        "[INVALID_VALUE] column width must be points, \"<n>fr\" or \"auto\", found \"{s}\""
                    )
                }),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum NodeType {
    Box,
    Text,
    Image,
    Table,
    PageBreak,
    Spacer,
    Field,
}

/// Every field any node type may carry. Converted into [`Node`] with
/// per-type checks so that a field on the wrong node type is reported as
/// `UNKNOWN_FIELD` at its own path.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawNode {
    #[serde(rename = "type")]
    ty: NodeType,
    id: Option<String>,
    style: Option<Style>,
    #[serde(rename = "break")]
    brk: Option<Break>,
    link: Option<Box<str>>,
    bookmark: Option<Box<Bookmark>>,
    pages: Option<Box<PageFilter>>,
    children: Option<Vec<Node>>,
    runs: Option<Vec<TextRun>>,
    src: Option<String>,
    data: Option<String>,
    fit: Option<ImageFit>,
    columns: Option<Vec<ColumnSpec>>,
    header_rows: Option<u32>,
    rows: Option<Vec<TableRow>>,
    field: Option<Box<Field>>,
}

/// Drops the spare capacity serde leaves when growing a `Vec` of unknown
/// length. Node vectors are short and numerous (one per table cell), so the
/// slack would otherwise be most of a large document's memory.
fn shrink<T>(mut v: Vec<T>) -> Vec<T> {
    v.shrink_to_fit();
    v
}

/// `deserialize_with` form of [`shrink`].
pub(crate) fn shrunk<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Vec::<T>::deserialize(d).map(shrink)
}

impl TryFrom<RawNode> for Node {
    type Error = String;

    fn try_from(r: RawNode) -> Result<Self, String> {
        let type_name = match r.ty {
            NodeType::Box => "box",
            NodeType::Text => "text",
            NodeType::Image => "image",
            NodeType::Table => "table",
            NodeType::PageBreak => "pageBreak",
            NodeType::Spacer => "spacer",
            NodeType::Field => "field",
        };
        let present = [
            ("style", r.style.is_some()),
            ("break", r.brk.is_some()),
            ("link", r.link.is_some()),
            ("bookmark", r.bookmark.is_some()),
            ("pages", r.pages.is_some()),
            ("children", r.children.is_some()),
            ("runs", r.runs.is_some()),
            ("src", r.src.is_some()),
            ("data", r.data.is_some()),
            ("fit", r.fit.is_some()),
            ("columns", r.columns.is_some()),
            ("headerRows", r.header_rows.is_some()),
            ("rows", r.rows.is_some()),
            ("field", r.field.is_some()),
        ];
        let allowed: &[&str] = match r.ty {
            NodeType::Box => &["style", "break", "link", "bookmark", "pages", "children"],
            NodeType::Text => &["style", "break", "link", "bookmark", "pages", "runs"],
            NodeType::Image => &[
                "style", "break", "link", "bookmark", "pages", "src", "data", "fit",
            ],
            NodeType::Table => &[
                "style",
                "break",
                "link",
                "bookmark",
                "pages",
                "columns",
                "headerRows",
                "rows",
            ],
            NodeType::PageBreak => &[],
            NodeType::Spacer => &["style", "break", "link", "bookmark", "pages"],
            NodeType::Field => &["style", "break", "pages", "field"],
        };
        if let Some((field, _)) = present.iter().find(|(f, set)| *set && !allowed.contains(f)) {
            return Err(format!(
                "[UNKNOWN_FIELD@{field}] field `{field}` is not allowed on a `{type_name}` node"
            ));
        }
        let missing =
            |field: &str| format!("[MISSING_FIELD] `{type_name}` node requires `{field}`");

        let kind = match r.ty {
            NodeType::Box => NodeKind::Box {
                children: shrink(r.children.unwrap_or_default()),
            },
            NodeType::Text => NodeKind::Text {
                runs: shrink(r.runs.ok_or_else(|| missing("runs"))?),
            },
            NodeType::Image => {
                let source = match (r.src, r.data) {
                    (Some(s), None) => ImageSource::Ref(s),
                    (None, Some(d)) => ImageSource::Inline(d),
                    (Some(_), Some(_)) => {
                        return Err("[INVALID_VALUE@data] an `image` node takes either `src` or `data`, not both".into())
                    }
                    (None, None) => {
                        return Err("[MISSING_FIELD] `image` node requires `src` or `data`".into())
                    }
                };
                NodeKind::Image {
                    source,
                    fit: r.fit.unwrap_or_default(),
                }
            }
            NodeType::Table => NodeKind::Table(Table {
                columns: r.columns.ok_or_else(|| missing("columns"))?,
                header_rows: r.header_rows.unwrap_or(0),
                rows: shrink(r.rows.ok_or_else(|| missing("rows"))?),
            }),
            NodeType::PageBreak => NodeKind::PageBreak,
            NodeType::Spacer => NodeKind::Spacer,
            NodeType::Field => NodeKind::Field(r.field.ok_or_else(|| missing("field"))?),
        };
        Ok(Node {
            id: r.id,
            style: r.style.map(super::NodeStyle::new).unwrap_or_default(),
            brk: r.brk.unwrap_or_default(),
            link: r.link,
            bookmark: r.bookmark,
            pages: r.pages,
            kind,
        })
    }
}
