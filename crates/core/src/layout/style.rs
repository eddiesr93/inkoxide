//! `schema::Style` → `taffy::Style`.

use taffy::prelude::*;
use taffy::style::{
    AlignContent as TAlignContent, AlignItems as TAlignItems, AlignSelf as TAlignSelf,
    Display as TDisplay, FlexDirection as TFlexDirection, FlexWrap as TFlexWrap,
    GridAutoFlow as TGridAutoFlow, GridTemplateArea, GridTemplateAreas, GridTemplateComponent,
    GridTemplateRepetition, JustifyContent as TJustifyContent, MaxTrackSizingFunction,
    MinTrackSizingFunction, Overflow as TOverflow, Position as TPosition, RepetitionCount,
};

use crate::schema::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, GridTrack, JustifyContent, Length, LengthPercent, Overflow, Position,
    RepeatCount, Style as S, TrackBreadth, TrackSize,
};

fn dim(l: Option<Length>) -> Dimension {
    match l {
        Some(Length::Points(v)) => Dimension::length(v),
        Some(Length::Percent(v)) => Dimension::percent(v),
        Some(Length::Auto) | None => Dimension::auto(),
    }
}

fn lpa(l: Option<Length>, default: LengthPercentageAuto) -> LengthPercentageAuto {
    match l {
        Some(Length::Points(v)) => LengthPercentageAuto::length(v),
        Some(Length::Percent(v)) => LengthPercentageAuto::percent(v),
        Some(Length::Auto) => LengthPercentageAuto::auto(),
        None => default,
    }
}

fn lp(l: Option<LengthPercent>) -> LengthPercentage {
    match l {
        Some(LengthPercent::Points(v)) => LengthPercentage::length(v),
        Some(LengthPercent::Percent(v)) => LengthPercentage::percent(v),
        None => LengthPercentage::length(0.0),
    }
}

fn min_track(b: TrackBreadth) -> MinTrackSizingFunction {
    type M = MinTrackSizingFunction;
    match b {
        TrackBreadth::Points(v) => M::length(v),
        TrackBreadth::Percent(v) => M::percent(v),
        // Validation rejects fr minimums; `auto` is the CSS fallback.
        TrackBreadth::Fr(_) | TrackBreadth::Auto => M::auto(),
        TrackBreadth::MinContent => M::min_content(),
        TrackBreadth::MaxContent => M::max_content(),
    }
}

fn max_track(b: TrackBreadth) -> MaxTrackSizingFunction {
    type M = MaxTrackSizingFunction;
    match b {
        TrackBreadth::Points(v) => M::length(v),
        TrackBreadth::Percent(v) => M::percent(v),
        TrackBreadth::Fr(v) => M::fr(v),
        TrackBreadth::Auto => M::auto(),
        TrackBreadth::MinContent => M::min_content(),
        TrackBreadth::MaxContent => M::max_content(),
    }
}

fn track(t: TrackSize) -> TrackSizingFunction {
    match t {
        TrackSize::Breadth(b) => match b {
            TrackBreadth::Points(v) => length(v),
            TrackBreadth::Percent(v) => percent(v),
            TrackBreadth::Fr(v) => fr(v),
            TrackBreadth::Auto => auto(),
            TrackBreadth::MinContent => min_content(),
            TrackBreadth::MaxContent => max_content(),
        },
        TrackSize::MinMax(min, max) => minmax(min_track(min), max_track(max)),
        TrackSize::FitContent(l) => fit_content(lp(Some(l))),
    }
}

fn template(tracks: &[GridTrack]) -> Vec<GridTemplateComponent<String>> {
    tracks
        .iter()
        .map(|t| match t {
            GridTrack::Single(t) => GridTemplateComponent::Single(track(*t)),
            GridTrack::Repeat(n, tracks) => GridTemplateComponent::Repeat(GridTemplateRepetition {
                count: match n {
                    RepeatCount::Count(n) => RepetitionCount::Count(*n),
                    RepeatCount::AutoFill => RepetitionCount::AutoFill,
                    RepeatCount::AutoFit => RepetitionCount::AutoFit,
                },
                tracks: tracks.iter().map(|&t| track(t)).collect(),
                line_names: Vec::new(),
            }),
        })
        .collect()
}

/// Invalid templates were rejected by validation; ignore them here.
fn areas(rows: &[String]) -> Option<GridTemplateAreas<String>> {
    let a = crate::schema::parse_areas(rows).ok()?;
    Some(GridTemplateAreas {
        areas: a
            .areas
            .iter()
            .map(|a| GridTemplateArea {
                name: a.name.to_string(),
                row_start: a.rows.0 + 1,
                row_end: a.rows.1 + 1,
                column_start: a.cols.0 + 1,
                column_end: a.cols.1 + 1,
            })
            .collect(),
        row_count: a.rows,
        column_count: a.cols,
    })
}

fn placement(p: Option<GridPlacement>) -> Line<taffy::style::GridPlacement> {
    use taffy::style::GridPlacement as G;
    let Some(p) = p else {
        return Line {
            start: G::Auto,
            end: G::Auto,
        };
    };
    let l = |v: i16| G::from_line_index(v);
    match (p.start, p.end, p.span) {
        (Some(s), _, Some(n)) => Line {
            start: l(s.0),
            end: G::Span(n),
        },
        (None, Some(e), Some(n)) => Line {
            start: G::Span(n),
            end: l(e.0),
        },
        (None, None, Some(n)) => Line {
            start: G::Span(n),
            end: G::Auto,
        },
        (s, e, None) => Line {
            start: s.map_or(G::Auto, |s| l(s.0)),
            end: e.map_or(G::Auto, |e| l(e.0)),
        },
    }
}

fn align_items(a: AlignItems) -> TAlignItems {
    match a {
        AlignItems::FlexStart => TAlignItems::FLEX_START,
        AlignItems::FlexEnd => TAlignItems::FLEX_END,
        AlignItems::Center => TAlignItems::CENTER,
        AlignItems::Baseline => TAlignItems::BASELINE,
        AlignItems::Stretch => TAlignItems::STRETCH,
        AlignItems::Start => TAlignItems::START,
        AlignItems::End => TAlignItems::END,
    }
}

fn align_self(a: AlignSelf) -> Option<TAlignSelf> {
    Some(match a {
        AlignSelf::Auto => return None,
        AlignSelf::FlexStart => TAlignSelf::FLEX_START,
        AlignSelf::FlexEnd => TAlignSelf::FLEX_END,
        AlignSelf::Center => TAlignSelf::CENTER,
        AlignSelf::Baseline => TAlignSelf::BASELINE,
        AlignSelf::Stretch => TAlignSelf::STRETCH,
        AlignSelf::Start => TAlignSelf::START,
        AlignSelf::End => TAlignSelf::END,
    })
}

/// Converts a node style. `flexDirection` defaults to `column`.
pub(crate) fn to_taffy(s: &S) -> Style {
    let mut t = Style {
        display: match s.display {
            Some(Display::Grid) => TDisplay::Grid,
            Some(Display::None) => TDisplay::None,
            Some(Display::Flex) | None => TDisplay::Flex,
        },
        flex_direction: match s.flex_direction {
            Some(FlexDirection::Row) => TFlexDirection::Row,
            Some(FlexDirection::RowReverse) => TFlexDirection::RowReverse,
            Some(FlexDirection::ColumnReverse) => TFlexDirection::ColumnReverse,
            Some(FlexDirection::Column) | None => TFlexDirection::Column,
        },
        flex_wrap: match s.flex_wrap {
            Some(FlexWrap::Wrap) => TFlexWrap::Wrap,
            Some(FlexWrap::WrapReverse) => TFlexWrap::WrapReverse,
            Some(FlexWrap::NoWrap) | None => TFlexWrap::NoWrap,
        },
        justify_content: s.justify_content.map(|j| match j {
            JustifyContent::FlexStart => TJustifyContent::FLEX_START,
            JustifyContent::FlexEnd => TJustifyContent::FLEX_END,
            JustifyContent::Center => TJustifyContent::CENTER,
            JustifyContent::SpaceBetween => TJustifyContent::SPACE_BETWEEN,
            JustifyContent::SpaceAround => TJustifyContent::SPACE_AROUND,
            JustifyContent::SpaceEvenly => TJustifyContent::SPACE_EVENLY,
            JustifyContent::Start => TJustifyContent::START,
            JustifyContent::End => TJustifyContent::END,
            JustifyContent::Stretch => TJustifyContent::STRETCH,
        }),
        align_items: s.align_items.map(align_items),
        align_self: s.align_self.and_then(align_self),
        align_content: s.align_content.map(|a| match a {
            AlignContent::FlexStart => TAlignContent::FLEX_START,
            AlignContent::FlexEnd => TAlignContent::FLEX_END,
            AlignContent::Center => TAlignContent::CENTER,
            AlignContent::SpaceBetween => TAlignContent::SPACE_BETWEEN,
            AlignContent::SpaceAround => TAlignContent::SPACE_AROUND,
            AlignContent::SpaceEvenly => TAlignContent::SPACE_EVENLY,
            AlignContent::Stretch => TAlignContent::STRETCH,
            AlignContent::Start => TAlignContent::START,
            AlignContent::End => TAlignContent::END,
        }),
        flex_grow: s.flex_grow.unwrap_or(0.0),
        flex_shrink: s.flex_shrink.unwrap_or(1.0),
        flex_basis: dim(s.flex_basis),
        gap: Size {
            width: lp(s.column_gap.or(s.gap)),
            height: lp(s.row_gap.or(s.gap)),
        },
        size: Size {
            width: dim(s.width),
            height: dim(s.height),
        },
        min_size: Size {
            width: lpa(s.min_width, LengthPercentageAuto::auto()),
            height: lpa(s.min_height, LengthPercentageAuto::auto()),
        },
        max_size: Size {
            width: lpa(s.max_width, LengthPercentageAuto::auto()),
            height: lpa(s.max_height, LengthPercentageAuto::auto()),
        },
        aspect_ratio: s.aspect_ratio,
        position: match s.position {
            Some(Position::Absolute) => TPosition::Absolute,
            Some(Position::Relative) | None => TPosition::Relative,
        },
        grid_auto_flow: match s.grid_auto_flow {
            Some(GridAutoFlow::Column) => TGridAutoFlow::Column,
            Some(GridAutoFlow::RowDense) => TGridAutoFlow::RowDense,
            Some(GridAutoFlow::ColumnDense) => TGridAutoFlow::ColumnDense,
            Some(GridAutoFlow::Row) | None => TGridAutoFlow::Row,
        },
        grid_row: placement(s.grid_row),
        grid_column: placement(s.grid_column),
        justify_items: s.justify_items.map(align_items),
        justify_self: s.justify_self.and_then(align_self),
        scrollbar_width: 0.0,
        ..Default::default()
    };
    if let Some(p) = &s.padding {
        t.padding = Rect {
            top: lp(p.top),
            right: lp(p.right),
            bottom: lp(p.bottom),
            left: lp(p.left),
        };
    }
    if let Some(m) = &s.margin {
        let zero = LengthPercentageAuto::length(0.0);
        t.margin = Rect {
            top: lpa(m.top, zero),
            right: lpa(m.right, zero),
            bottom: lpa(m.bottom, zero),
            left: lpa(m.left, zero),
        };
    }
    if let Some(i) = &s.inset {
        let a = LengthPercentageAuto::auto();
        t.inset = Rect {
            top: lpa(i.top, a),
            right: lpa(i.right, a),
            bottom: lpa(i.bottom, a),
            left: lpa(i.left, a),
        };
    }
    if let Some(b) = &s.border_width {
        let [top, right, bottom, left] = b.get(0.0).map(LengthPercentage::length);
        t.border = Rect {
            top,
            right,
            bottom,
            left,
        };
    }
    if let Some(cols) = &s.grid_template_columns {
        t.grid_template_columns = template(cols);
    }
    if let Some(rows) = &s.grid_template_rows {
        t.grid_template_rows = template(rows);
    }
    if let Some(cols) = &s.grid_auto_columns {
        t.grid_auto_columns = cols.iter().map(|&c| track(c)).collect();
    }
    if let Some(rows) = &s.grid_auto_rows {
        t.grid_auto_rows = rows.iter().map(|&r| track(r)).collect();
    }
    if let Some(rows) = &s.grid_template_areas {
        t.grid_template_areas = areas(rows);
    }
    if let Some(name) = &s.grid_area {
        // `<name>` resolves to the implicit `<name>-start` / `<name>-end`
        // lines that the parent's area template defines.
        let line = || Line {
            start: taffy::style::GridPlacement::NamedLine(name.to_string(), 1),
            end: taffy::style::GridPlacement::NamedLine(name.to_string(), 1),
        };
        t.grid_row = line();
        t.grid_column = line();
    }
    if s.overflow == Some(Overflow::Hidden) {
        t.overflow = taffy::geometry::Point {
            x: TOverflow::Hidden,
            y: TOverflow::Hidden,
        };
    }
    t
}
