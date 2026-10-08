//! Display list: the backend-neutral drawing operations of one page.
//!
//! Coordinates are in points with the origin at the **top-left** of the page
//! and y growing downwards (like the layout tree). The PDF sink flips once
//! per page.

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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Save,
    Restore,
    /// Concatenates `[a b c d e f]` to the current transform.
    Transform([f32; 6]),
    ClipRect(Rect),
    ClipPath(Path),
    FillRect {
        rect: Rect,
        color: Color,
    },
    FillPath {
        path: Path,
        color: Color,
    },
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
