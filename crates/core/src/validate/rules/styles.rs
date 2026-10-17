//! The checks of style values: colours, lengths, edges, grid tracks and
//! templates, transforms, and each style property.

use super::{
    Alpha, Checker, Len, MAX_LINE_HEIGHT, MAX_NUMBER, MAX_ORPHANS, MAX_PERCENT, MAX_TEXT_INDENT,
};
use crate::error::ValidationCode as Code;
use crate::schema::{
    is_area_name, parse_areas, BorderRadius, Color, GridTrack, OptEdges, RepeatCount, Style,
    TrackBreadth, TrackSize, TransformOp,
};

impl Checker<'_> {
    pub(super) fn color(&mut self, field: Option<&str>, c: &Color) {
        self.color_as(field, c, Alpha::Blended);
    }

    /// A colour whose alpha the painter uses as `alpha` says.
    pub(super) fn color_as(&mut self, field: Option<&str>, c: &Color, alpha: Alpha) {
        if c.iter().any(|v| !(0.0..=1.0).contains(v)) {
            self.err(
                Code::OutOfRange,
                field,
                "colour components must be within [0, 1]",
            );
        } else if c[3] < 1.0 && (alpha == Alpha::Blended || alpha == Alpha::Fill && c[3] > 0.0) {
            self.transparent(field, "a colour with alpha below 1");
        }
    }

    /// A non-negative number of at most [`MAX_NUMBER`].
    pub(super) fn non_neg(&mut self, field: &str, v: Option<f32>) {
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
    pub(super) fn signed(&mut self, field: Option<&str>, v: f32) {
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
    pub(super) fn len(&mut self, field: Option<&str>, v: impl Into<Len>, negative: bool) {
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
    pub(super) fn edges<T: Copy + Into<Len>>(
        &mut self,
        field: &str,
        e: Option<&OptEdges<T>>,
        negative: bool,
    ) {
        let Some(e) = e else { return };
        self.at(field, |c| {
            for (k, v) in e.iter() {
                c.len(Some(k), *v, negative);
            }
        });
    }

    /// A grid track: its breadth, or each bound of `{min, max}` or
    /// `{fitContent}`.
    pub(super) fn track(&mut self, t: &TrackSize) {
        match *t {
            TrackSize::Breadth(b) => self.len(None, b, false),
            TrackSize::MinMax(min, max) => {
                self.len(Some("min"), min, false);
                self.len(Some("max"), max, false);
            }
            TrackSize::FitContent(f) => self.len(Some("fitContent"), f, false),
        }
    }

    pub(super) fn style(&mut self, s: &Style, is_text: bool, is_cell: bool) {
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
                ("float", s.float.is_some()),
                ("clear", s.clear.is_some()),
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
            self.color_as(Some("backgroundColor"), bg, Alpha::Fill);
        }
        if let Some(o) = s.opacity {
            if !(0.0..=1.0).contains(&o) {
                self.err(Code::OutOfRange, Some("opacity"), "must be within [0, 1]");
            } else if o > 0.0 && o < 1.0 {
                // 0 draws nothing.
                self.transparent(Some("opacity"), "an `opacity` below 1");
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
    pub(super) fn template(&mut self, tracks: &[GridTrack]) {
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
    pub(super) fn transform_op(&mut self, op: &TransformOp) {
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
