//! schema::Style → taffy::Style.

use taffy::prelude::*;
use taffy::style::{
    AlignContent as TAlignContent, AlignItems as TAlignItems, AlignSelf as TAlignSelf,
    Display as TDisplay, FlexDirection as TFlexDirection, FlexWrap as TFlexWrap,
    GridAutoFlow as TGridAutoFlow, GridTemplateComponent, JustifyContent as TJustifyContent,
    Overflow as TOverflow, Position as TPosition,
};

use crate::schema::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, JustifyContent, Length, LengthPercent, Overflow, Position, Style as S,
    TrackSize,
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

fn track(t: TrackSize) -> TrackSizingFunction {
    match t {
        TrackSize::Points(v) => length(v),
        TrackSize::Percent(v) => percent(v),
        TrackSize::Fr(v) => fr(v),
        TrackSize::Auto => auto(),
        TrackSize::MinContent => min_content(),
        TrackSize::MaxContent => max_content(),
    }
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

/// Converts a node style. `flexDirection` defaults to `column` (ADR-006).
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
        align_items: s.align_items.map(|a| match a {
            AlignItems::FlexStart => TAlignItems::FLEX_START,
            AlignItems::FlexEnd => TAlignItems::FLEX_END,
            AlignItems::Center => TAlignItems::CENTER,
            AlignItems::Baseline => TAlignItems::BASELINE,
            AlignItems::Stretch => TAlignItems::STRETCH,
            AlignItems::Start => TAlignItems::START,
            AlignItems::End => TAlignItems::END,
        }),
        align_self: s.align_self.and_then(|a| {
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
        }),
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
        t.grid_template_columns = cols
            .iter()
            .map(|&c| GridTemplateComponent::Single(track(c)))
            .collect();
    }
    if let Some(rows) = &s.grid_template_rows {
        t.grid_template_rows = rows
            .iter()
            .map(|&r| GridTemplateComponent::Single(track(r)))
            .collect();
    }
    if s.overflow == Some(Overflow::Hidden) {
        t.overflow = taffy::geometry::Point {
            x: TOverflow::Hidden,
            y: TOverflow::Hidden,
        };
    }
    t
}
