//! Display list: the backend-neutral drawing operations of one page.
//!
//! Coordinates are in points with the origin at the **top-left** of the page
//! and y growing downwards (like the layout tree). The PDF sink flips once
//! per page.

mod boxes;
pub(crate) mod debug;
mod shapes;

pub(crate) use boxes::{content_radii, paint_flow, ContentCtx, ContentPainter, FlowPart};
pub(crate) use debug::{draw as draw_debug, page_box as debug_page_box};
pub(crate) use debug::{DebugBoxes, Labels as DebugLabels};
pub(crate) use shapes::{is_square, rounded_rect};

use crate::schema::Color;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathSeg {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Path {
    pub segs: Vec<PathSeg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dash {
    /// On/off lengths.
    pub pattern: [f32; 2],
    pub round_cap: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Save,
    Restore,
    /// Concatenates `[a b c d e f]` to the current transform.
    Transform([f32; 6]),
    ClipRect(Rect),
    ClipPath {
        path: Path,
        rule: FillRule,
    },
    FillRect {
        rect: Rect,
        color: Color,
    },
    FillPath {
        path: Path,
        color: Color,
        rule: FillRule,
    },
    StrokePath {
        path: Path,
        color: Color,
        width: f32,
        dash: Option<Dash>,
    },
    Text(TextOp),
    /// Draws an image `XObject` with its unit square mapped by `matrix`.
    Image {
        key: crate::images::ImageKey,
        matrix: [f32; 6],
    },
    /// Sets the absolute fill and stroke alpha (cumulative opacity is
    /// computed by the painter; PDF `gs` does not multiply).
    SetOpacity(f32),
    /// A clickable area (draws nothing). `rect` is in page coordinates;
    /// transforms (rotation) do not apply to it.
    Link {
        rect: Rect,
        target: Box<str>,
    },
    /// A form field widget (draws nothing here; the writer adds an
    /// annotation with its appearance). Like links, transforms do not
    /// apply to it.
    Field(Box<crate::forms::FieldOp>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextGlyph {
    pub id: u16,
    /// Pen position relative to the op's origin, in points.
    pub x: f32,
    /// Vertical offset, points, positive up.
    pub y_offset: f32,
}

/// A run of glyphs in one font, size and colour on one baseline.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextOp {
    pub font: crate::fonts::FontKey,
    pub size: f32,
    /// RGB; alpha is applied through `SetOpacity`.
    pub color: Color,
    /// Baseline origin (layout space).
    pub x: f32,
    pub y: f32,
    pub glyphs: Vec<TextGlyph>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageContent {
    pub width: f32,
    pub height: f32,
    pub ops: Vec<Op>,
}

impl PageContent {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ops: Vec::new(),
        }
    }
}
