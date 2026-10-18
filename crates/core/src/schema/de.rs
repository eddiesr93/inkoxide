//! Hand-written deserialisers for the few schema shapes serde cannot express
//! directly (number-or-string unions, the node discriminator).
//!
//! Errors raised here carry a `[CODE]` or `[CODE@field]` prefix that
//! `validate` turns into a structured [`crate::ValidationError`].

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use super::{
    Attachment, AttachmentDecl, BackgroundImage, Bookmark, BorderRadius, BorderStyle, BorderStyles,
    Break, ColumnSpec, ColumnWidth, Corners, Field, FieldValue, FontFeature, FontVariation,
    ImageFit, ImageSource, InlineData, Node, NodeKind, Note, ObjectPosition, OptEdges, PageFilter,
    PagePattern, Relationship, RunAlign, RunImage, Style, Table, TableRow, TextRun, TransformOp,
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

impl LengthPercent {
    /// In points, a percentage taken of `of`.
    pub fn resolve(self, of: f32) -> f32 {
        match self {
            LengthPercent::Points(v) => v,
            LengthPercent::Percent(p) => p * of,
        }
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

/// The number of a percentage (`"-12.5%"`) or a flexible size (`"2fr"`),
/// written as the schema's `Percent` and `Fr` patterns allow: digits, then
/// optionally `.` and more digits; a percentage may start with `-`.
///
/// The patterns bound the digits as written, so the number keeps where it
/// stands against a bound: digits past a whole number (`"10000.0001"`,
/// which is `10000` as an `f32`) read as the next `f32` away from zero,
/// past a bound of `10000`, and digits that are not all zero never read
/// as 0, so a tiny negative stays negative.
fn parse_suffixed(s: &str, suffix: &str) -> Option<f32> {
    let n = s.strip_suffix(suffix)?;
    let unsigned = match suffix {
        "%" => n.strip_prefix('-').unwrap_or(n),
        _ => n,
    };
    let (int, frac) = unsigned.split_once('.').unwrap_or((unsigned, "0"));
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !(digits(int) && digits(frac)) {
        return None;
    }
    let v = n.parse::<f32>().ok().filter(|v| v.is_finite())?;
    let away = |v: f32| {
        if n.starts_with('-') {
            v.next_down()
        } else {
            v.next_up()
        }
    };
    // Below 2^24 every whole number is an `f32`, and the bounds are there.
    if v.fract() == 0.0 && v.abs() < 16_777_216.0 {
        let whole = int.trim_start_matches('0');
        let written = if whole.len() > 8 {
            u32::MAX
        } else {
            whole.parse().unwrap_or(0)
        };
        let fraction = frac.bytes().any(|b| b != b'0');
        let at = v.abs() as u32;
        if written > at || (written == at && fraction) {
            return Some(away(v));
        }
    }
    Some(v)
}

/// A percentage (`"50%"`) as a fraction, as [`parse_suffixed`] reads it:
/// one that is not 0 does not become 0.
fn percent(s: &str) -> Option<f32> {
    let p = parse_suffixed(s, "%")?;
    let f = p / 100.0;
    Some(if f == 0.0 && p != 0.0 {
        f32::from_bits(1).copysign(p)
    } else {
        f
    })
}

/// A `Deserialize` for `$ty` from a number or a string. The string arm
/// gives `Option<$ty>` (`None`: "expected {expecting}"), or after
/// `@result` a `Result<$ty, String>` with its own message.
macro_rules! num_or_str_visitor {
    ($ty:ident, $expecting:expr, |$v:ident| $num:expr, |$s:ident| @result $str:expr) => {
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
                        let r: Result<$ty, String> = $str;
                        r.map_err(E::custom)
                    }
                }
                d.deserialize_any(V)
            }
        }
    };
    ($ty:ident, $expecting:expr, |$v:ident| $num:expr, |$s:ident| $str:expr) => {
        num_or_str_visitor!($ty, $expecting, |$v| $num, |$s| @result $str.ok_or_else(|| {
            format!("[INVALID_VALUE] expected {}, found \"{}\"", $expecting, $s)
        }));
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
        percent(s).map(Length::Percent)
    }
);

num_or_str_visitor!(
    LengthPercent,
    "a number of points or a percentage like \"50%\"",
    |v| finite(v).map(LengthPercent::Points),
    |s| percent(s).map(LengthPercent::Percent)
);

/// One axis of an [`ObjectPosition`]: a percentage, or that axis's keywords
/// (`start`, `"center"`, `end`) as 0 %, 50 % and 100 %.
fn position_axis(s: &str, start: &str, end: &str) -> Option<LengthPercent> {
    let fraction = match s {
        "center" => 0.5,
        _ if s == start => 0.0,
        _ if s == end => 1.0,
        _ => percent(s)?,
    };
    Some(LengthPercent::Percent(fraction))
}

/// The horizontal axis of an [`ObjectPosition`].
struct PositionX(LengthPercent);
/// The vertical axis of an [`ObjectPosition`].
struct PositionY(LengthPercent);

num_or_str_visitor!(
    PositionX,
    "a number of points, a percentage, \"left\", \"center\" or \"right\"",
    |v| finite(v).map(|p| PositionX(LengthPercent::Points(p))),
    |s| position_axis(s, "left", "right").map(PositionX)
);

num_or_str_visitor!(
    PositionY,
    "a number of points, a percentage, \"top\", \"center\" or \"bottom\"",
    |v| finite(v).map(|p| PositionY(LengthPercent::Points(p))),
    |s| position_axis(s, "top", "bottom").map(PositionY)
);

/// `deserialize_with` for [`ObjectPosition::x`].
pub(super) fn position_x<'de, D: Deserializer<'de>>(d: D) -> Result<LengthPercent, D::Error> {
    PositionX::deserialize(d).map(|p| p.0)
}

/// `deserialize_with` for [`ObjectPosition::y`].
pub(super) fn position_y<'de, D: Deserializer<'de>>(d: D) -> Result<LengthPercent, D::Error> {
    PositionY::deserialize(d).map(|p| p.0)
}

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
                Vec::<Int<i32>>::deserialize(de::value::SeqAccessDeserializer::new(seq))
                    .map(|v| PageFilter::List(v.into_iter().map(|i| i.0).collect()))
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for FieldValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = FieldValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string, or an array of strings")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<FieldValue, E> {
                Ok(FieldValue::One(s.to_string()))
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<FieldValue, E> {
                Ok(FieldValue::One(s))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<FieldValue, A::Error> {
                Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq))
                    .map(|v| FieldValue::Many(shrink(v)))
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for TransformOp {
    /// An object with exactly one key. serde's externally tagged enum
    /// reports a second key, or none, as malformed input with no path.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = TransformOp;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a transform function: an object with one key")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<TransformOp, A::Error> {
                const ONE_KEY: &str = "[INVALID_VALUE] a transform function is an object with exactly one key: `translate`, `scale`, `rotate`, `skew` or `matrix`";
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::custom(ONE_KEY));
                };
                let op = match key.as_str() {
                    "translate" => TransformOp::Translate(map.next_value()?),
                    "scale" => TransformOp::Scale(map.next_value()?),
                    "rotate" => TransformOp::Rotate(map.next_value()?),
                    "skew" => TransformOp::Skew(map.next_value()?),
                    "matrix" => TransformOp::Matrix(map.next_value()?),
                    _ => {
                        return Err(de::Error::custom(format_args!(
                            "[INVALID_VALUE] unknown transform function `{key}`, expected `translate`, `scale`, `rotate`, `skew` or `matrix`"
                        )))
                    }
                };
                if map.next_key::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom(ONE_KEY));
                }
                Ok(op)
            }
        }
        d.deserialize_map(V)
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
            .map(TrackBreadth::Fr)
            .or_else(|| percent(s).map(TrackBreadth::Percent)),
    }
}

num_or_str_visitor!(
    TrackBreadth,
    BREADTH,
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
        let Int(v) = Int::<i128>::deserialize(d)?;
        if v == 0 || v.unsigned_abs() > i16::MAX as u128 {
            return Err(de::Error::custom(
                "[OUT_OF_RANGE] grid line must be a non-zero integer within ±32767",
            ));
        }
        Ok(GridLine(v as i16))
    }
}

/// An integer of type `T`, read as any integer: one past `T`'s range is
/// `OUT_OF_RANGE`, like one past a bound the schema states (serde would
/// call it `INVALID_VALUE`). The schema states `T`'s range where it has no
/// tighter bound.
struct Int<T>(T);

impl<'de, T: TryFrom<i128>> Deserialize<'de> for Int<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = i128;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an integer")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<i128, E> {
                Ok(v.into())
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<i128, E> {
                Ok(v.into())
            }
            /// An integer written as a float (`400.0`), as JSON Schema
            /// reads it; a whole number past `i128` saturates, and is out
            /// of range as `T`.
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<i128, E> {
                if v.fract() == 0.0 {
                    Ok(v as i128)
                } else {
                    Err(E::invalid_type(de::Unexpected::Float(v), &self))
                }
            }
        }
        let v = d.deserialize_i64(V)?;
        T::try_from(v)
            .map(Int)
            .map_err(|_| de::Error::custom(format_args!("[OUT_OF_RANGE] {v} is out of range")))
    }
}

/// `deserialize_with` for an integer field: see [`Int`].
pub(super) fn int<'de, D: Deserializer<'de>, T: TryFrom<i128>>(d: D) -> Result<T, D::Error> {
    Int::deserialize(d).map(|i| i.0)
}

/// [`int`] for an optional field (with `#[serde(default)]`).
pub(super) fn opt_int<'de, D: Deserializer<'de>, T: TryFrom<i128>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<Int<T>>::deserialize(d).map(|o| o.map(|i| i.0))
}

/// `deserialize_with` for [`TextRun::font_features`]: an object of feature
/// tags to integer values, kept in the order written (validation checks
/// the tags, values and count).
pub(super) fn font_features<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Box<[FontFeature]>>, D::Error> {
    tag_map(
        d,
        "an object of OpenType feature tags to integers",
        |tag, Int(value)| FontFeature { tag, value },
    )
}

/// `deserialize_with` for [`FontDecl::variations`]: an object of axis tags
/// to numbers, kept in the order written (validation checks the tags,
/// values and count).
///
/// [`FontDecl::variations`]: super::FontDecl::variations
pub(super) fn font_variations<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Box<[FontVariation]>>, D::Error> {
    tag_map(
        d,
        "an object of variation axis tags to numbers",
        |tag, value| FontVariation { tag, value },
    )
}

/// An optional object of tags to values, as `make` turns each entry into
/// a `T`, in the order written.
fn tag_map<'de, D, V, T>(
    d: D,
    expecting: &'static str,
    make: fn(String, V) -> T,
) -> Result<Option<Box<[T]>>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Entries<V, T> {
        expecting: &'static str,
        make: fn(String, V) -> T,
    }
    impl<'de, V: Deserialize<'de>, T> Visitor<'de> for Entries<V, T> {
        type Value = Box<[T]>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str(self.expecting)
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some((tag, value)) = map.next_entry::<String, V>()? {
                out.push((self.make)(tag, value));
            }
            Ok(out.into_boxed_slice())
        }
    }
    impl<'de, V: Deserialize<'de>, T> de::DeserializeSeed<'de> for Entries<V, T> {
        type Value = Box<[T]>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_map(self)
        }
    }
    struct Optional<V, T>(Entries<V, T>);
    impl<'de, V: Deserialize<'de>, T> Visitor<'de> for Optional<V, T> {
        type Value = Option<Box<[T]>>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str(self.0.expecting)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            de::DeserializeSeed::deserialize(self.0, d).map(Some)
        }
    }
    d.deserialize_option(Optional(Entries { expecting, make }))
}

/// `deserialize_with` for a list that, when present, has at least one item
/// (the schema's `minItems: 1`), whatever its owner.
pub(super) fn non_empty<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    let v = Vec::<T>::deserialize(d)?;
    if v.is_empty() {
        return Err(de::Error::custom(
            "[INVALID_VALUE] the list needs at least one item",
        ));
    }
    Ok(v)
}

/// `deserialize_with` for [`super::GridPlacement::span`]: at least one track.
pub(super) fn grid_span<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    let Some(Int(v)) = Option::<Int<i128>>::deserialize(d)? else {
        return Ok(None);
    };
    match u16::try_from(v) {
        Ok(n) if n >= 1 => Ok(Some(n)),
        _ => Err(de::Error::custom(
            "[OUT_OF_RANGE] grid span must be an integer within [1, 65535]",
        )),
    }
}

num_or_str_visitor!(
    ColumnWidth,
    "points, \"<n>fr\" or \"auto\"",
    |v| if v > 0.0 && v.is_finite() {
        Ok(ColumnWidth::Points(v as f32))
    } else {
        Err(E::custom(format_args!(
            "[OUT_OF_RANGE] column width must be a positive number, found {v}"
        )))
    },
    |s| @result match parse_suffixed(s, "fr") {
        _ if s == "auto" => Ok(ColumnWidth::Auto),
        Some(v) if v > 0.0 => Ok(ColumnWidth::Fr(v)),
        Some(_) => Err(format!(
            "[OUT_OF_RANGE] column width must be positive, found \"{s}\""
        )),
        None => Err(format!(
            "[INVALID_VALUE] column width must be points, \"<n>fr\" or \"auto\", found \"{s}\""
        )),
    }
);

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum NodeType {
    Box,
    Text,
    Image,
    Table,
    PageBreak,
    Spacer,
    Field,
    Note,
    Attachment,
}

/// Properties that most node types take.
const COMMON: &[&str] = &["style", "break", "link", "bookmark", "pages", "debug"];

impl NodeType {
    #[cfg(test)]
    pub(super) const ALL: [Self; 9] = [
        Self::Box,
        Self::Text,
        Self::Image,
        Self::Table,
        Self::PageBreak,
        Self::Spacer,
        Self::Field,
        Self::Note,
        Self::Attachment,
    ];

    /// The `type` value.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Box => "box",
            Self::Text => "text",
            Self::Image => "image",
            Self::Table => "table",
            Self::PageBreak => "pageBreak",
            Self::Spacer => "spacer",
            Self::Field => "field",
            Self::Note => "note",
            Self::Attachment => "attachment",
        }
    }

    /// The properties a node of this type takes, besides `type` and `id`.
    pub(super) fn fields(self) -> impl Iterator<Item = &'static str> {
        let (common, own): (&[&str], &[&str]) = match self {
            Self::Box => (COMMON, &["children", "backgroundImage"]),
            Self::Text => (COMMON, &["runs"]),
            Self::Image => (COMMON, &["src", "data", "fit", "objectPosition", "alt"]),
            Self::Table => (COMMON, &["columns", "headerRows", "rows"]),
            Self::PageBreak => (&[], &[]),
            Self::Spacer => (COMMON, &[]),
            Self::Field => (&["style", "break", "pages", "debug"], &["field"]),
            Self::Note => (&["style", "break", "pages", "debug"], &["note"]),
            Self::Attachment => (&["style", "break", "pages", "debug"], &["attachment"]),
        };
        common.iter().chain(own).copied()
    }
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
    debug: Option<bool>,
    children: Option<Vec<Node>>,
    runs: Option<Vec<TextRun>>,
    src: Option<String>,
    data: Option<String>,
    fit: Option<ImageFit>,
    object_position: Option<ObjectPosition>,
    alt: Option<String>,
    columns: Option<Vec<ColumnSpec>>,
    #[serde(default, deserialize_with = "opt_int")]
    header_rows: Option<u32>,
    rows: Option<Vec<TableRow>>,
    field: Option<Box<Field>>,
    note: Option<Box<Note>>,
    attachment: Option<Box<Attachment>>,
    background_image: Option<Box<BackgroundImage>>,
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
        let type_name = r.ty.name();
        let present = [
            ("style", r.style.is_some()),
            ("break", r.brk.is_some()),
            ("link", r.link.is_some()),
            ("bookmark", r.bookmark.is_some()),
            ("pages", r.pages.is_some()),
            ("debug", r.debug.is_some()),
            ("children", r.children.is_some()),
            ("runs", r.runs.is_some()),
            ("src", r.src.is_some()),
            ("data", r.data.is_some()),
            ("fit", r.fit.is_some()),
            ("objectPosition", r.object_position.is_some()),
            ("alt", r.alt.is_some()),
            ("columns", r.columns.is_some()),
            ("headerRows", r.header_rows.is_some()),
            ("rows", r.rows.is_some()),
            ("field", r.field.is_some()),
            ("note", r.note.is_some()),
            ("attachment", r.attachment.is_some()),
            ("backgroundImage", r.background_image.is_some()),
        ];
        if let Some((field, _)) = present
            .iter()
            .find(|(f, set)| *set && !r.ty.fields().any(|a| a == *f))
        {
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
                let source = image_source(r.src, r.data, "an `image` node")?;
                NodeKind::Image {
                    source,
                    fit: r.fit.unwrap_or_default(),
                    object_position: r.object_position.unwrap_or_default(),
                    alt: r.alt,
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
            NodeType::Note => NodeKind::Note(r.note.ok_or_else(|| missing("note"))?),
            NodeType::Attachment => {
                NodeKind::Attachment(r.attachment.ok_or_else(|| missing("attachment"))?)
            }
        };
        Ok(Node {
            id: r.id,
            style: r.style.map(super::NodeStyle::new).unwrap_or_default(),
            brk: r.brk.unwrap_or_default(),
            link: r.link,
            bookmark: r.bookmark,
            pages: r.pages,
            debug: r.debug.unwrap_or(false),
            background_image: r.background_image,
            kind,
        })
    }
}

/// The source of an image from its `src` and `data`, exactly one of which
/// `what` (for the message) must set.
fn image_source(
    src: Option<String>,
    data: Option<String>,
    what: &str,
) -> Result<ImageSource, String> {
    src_or_data(src, data, what, ImageSource::Ref, |d| {
        ImageSource::Inline(InlineData::new(d))
    })
}

/// `by_id(src)` or `inline(data)`, exactly one of which `what` (for the
/// message) must set.
fn src_or_data<T>(
    src: Option<String>,
    data: Option<String>,
    what: &str,
    by_id: impl FnOnce(String) -> T,
    inline: impl FnOnce(String) -> T,
) -> Result<T, String> {
    match (src, data) {
        (Some(s), None) => Ok(by_id(s)),
        (None, Some(d)) => Ok(inline(d)),
        (Some(_), Some(_)) => Err(format!(
            "[INVALID_VALUE@data] {what} takes either `src` or `data`, not both"
        )),
        (None, None) => Err(format!("[MISSING_FIELD] {what} requires `src` or `data`")),
    }
}

/// A [`BackgroundImage`] as written: `src` or `data`, checked on conversion.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawBackgroundImage {
    src: Option<String>,
    data: Option<String>,
    fit: Option<ImageFit>,
    object_position: Option<ObjectPosition>,
}

impl TryFrom<RawBackgroundImage> for BackgroundImage {
    type Error = String;

    fn try_from(r: RawBackgroundImage) -> Result<Self, String> {
        Ok(Self {
            source: image_source(r.src, r.data, "`backgroundImage`")?,
            fit: r.fit.unwrap_or_default(),
            object_position: r.object_position.unwrap_or_default(),
        })
    }
}

/// An [`AttachmentDecl`] as written: `src` or `data`, checked on
/// conversion.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawAttachmentDecl {
    name: String,
    src: Option<String>,
    data: Option<String>,
    mime_type: Option<String>,
    description: Option<String>,
    #[serde(default)]
    relationship: Relationship,
    modified: Option<String>,
}

impl TryFrom<RawAttachmentDecl> for AttachmentDecl {
    type Error = String;

    fn try_from(r: RawAttachmentDecl) -> Result<Self, String> {
        use crate::attachments::{FileSource, InlineFile};
        let source = src_or_data(r.src, r.data, "an attachment", FileSource::Ref, |d| {
            FileSource::Inline(InlineFile::new(d))
        })?;
        Ok(Self {
            name: r.name,
            source,
            mime_type: r.mime_type,
            description: r.description,
            relationship: r.relationship,
            modified: r.modified,
        })
    }
}

/// A [`RunImage`] as written: `src` or `data`, checked on conversion.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawRunImage {
    src: Option<String>,
    data: Option<String>,
    width: Option<f32>,
    height: Option<f32>,
    fit: Option<ImageFit>,
    object_position: Option<ObjectPosition>,
    alt: Option<String>,
}

impl TryFrom<RawRunImage> for RunImage {
    type Error = String;

    fn try_from(r: RawRunImage) -> Result<Self, String> {
        Ok(Self {
            source: image_source(r.src, r.data, "a run's `image`")?,
            width: r.width,
            height: r.height,
            fit: r.fit.unwrap_or_default(),
            object_position: r.object_position.unwrap_or_default(),
            alt: r.alt,
        })
    }
}

num_or_str_visitor!(
    RunAlign,
    "baseline, super, sub, middle, textTop, textBottom, top, bottom or a number",
    // Bounded by validation.
    |v| Ok(RunAlign::Points(v as f32)),
    |s| @result match s {
        "baseline" => Ok(RunAlign::Baseline),
        "super" => Ok(RunAlign::Super),
        "sub" => Ok(RunAlign::Sub),
        "middle" => Ok(RunAlign::Middle),
        "textTop" => Ok(RunAlign::TextTop),
        "textBottom" => Ok(RunAlign::TextBottom),
        "top" => Ok(RunAlign::Top),
        "bottom" => Ok(RunAlign::Bottom),
        _ => Err(format!(
            "[INVALID_VALUE] `verticalAlign` must be baseline, super, sub, middle, \
             textTop, textBottom, top, bottom or a number, found \"{s}\""
        )),
    }
);
