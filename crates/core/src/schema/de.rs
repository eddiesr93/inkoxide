//! Hand-written deserialisers for the few schema shapes serde cannot express
//! directly (number-or-string unions, the node discriminator).
//!
//! Errors raised here carry a `[CODE]` or `[CODE@field]` prefix that
//! `validate` turns into a structured [`crate::ValidationError`].

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use super::{
    Bookmark, Break, ColumnSpec, ColumnWidth, ImageFit, ImageSource, Node, NodeKind, Style, Table,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Points(f32),
    Percent(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

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

num_or_str_visitor!(
    TrackSize,
    "a number of points, a percentage, \"<n>fr\", \"auto\", \"min-content\" or \"max-content\"",
    |v| finite(v).map(TrackSize::Points),
    |s| match s {
        "auto" => Some(TrackSize::Auto),
        "min-content" => Some(TrackSize::MinContent),
        "max-content" => Some(TrackSize::MaxContent),
        _ => parse_suffixed(s, "fr")
            .filter(|v| *v >= 0.0)
            .map(TrackSize::Fr)
            .or_else(|| parse_suffixed(s, "%").map(|p| TrackSize::Percent(p / 100.0))),
    }
);

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
    children: Option<Vec<Node>>,
    runs: Option<Vec<TextRun>>,
    src: Option<String>,
    data: Option<String>,
    fit: Option<ImageFit>,
    columns: Option<Vec<ColumnSpec>>,
    header_rows: Option<u32>,
    rows: Option<Vec<TableRow>>,
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
        };
        let present = [
            ("style", r.style.is_some()),
            ("break", r.brk.is_some()),
            ("link", r.link.is_some()),
            ("bookmark", r.bookmark.is_some()),
            ("children", r.children.is_some()),
            ("runs", r.runs.is_some()),
            ("src", r.src.is_some()),
            ("data", r.data.is_some()),
            ("fit", r.fit.is_some()),
            ("columns", r.columns.is_some()),
            ("headerRows", r.header_rows.is_some()),
            ("rows", r.rows.is_some()),
        ];
        let allowed: &[&str] = match r.ty {
            NodeType::Box => &["style", "break", "link", "bookmark", "children"],
            NodeType::Text => &["style", "break", "link", "bookmark", "runs"],
            NodeType::Image => &["style", "break", "link", "bookmark", "src", "data", "fit"],
            NodeType::Table => &[
                "style",
                "break",
                "link",
                "bookmark",
                "columns",
                "headerRows",
                "rows",
            ],
            NodeType::PageBreak => &[],
            NodeType::Spacer => &["style", "break", "link", "bookmark"],
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
        };
        Ok(Node {
            id: r.id,
            style: r.style.unwrap_or_default(),
            brk: r.brk.unwrap_or_default(),
            link: r.link,
            bookmark: r.bookmark,
            kind,
        })
    }
}
