//! Plane geometry shared by layout, pagination, fonts and painting:
//! rectangles and 2×3 affine matrices, in points with y growing downwards.

use crate::schema::Edges;

/// Overflow below this (points) is rounding, not worth a clip.
pub(crate) const CLIP_EPS: f32 = 1e-3;

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

    /// Shrunk by `e` on each side, to no less than nothing.
    pub fn inset(self, e: Edges<f32>) -> Self {
        Self::new(
            self.x + e.left,
            self.y + e.top,
            (self.w - e.left - e.right).max(0.0),
            (self.h - e.top - e.bottom).max(0.0),
        )
    }

    /// Whether this sticks out of `outer` on some side by more than
    /// [`CLIP_EPS`]: content drawn here needs a clip to `outer`.
    pub fn overflows(self, outer: Rect) -> bool {
        self.x < outer.x - CLIP_EPS
            || self.y < outer.y - CLIP_EPS
            || self.x + self.w > outer.x + outer.w + CLIP_EPS
            || self.y + self.h > outer.y + outer.h + CLIP_EPS
    }

    /// Grown by `e` on each side (shrunk where negative, to no less than
    /// nothing).
    pub fn outset(self, e: Edges<f32>) -> Self {
        Self::new(
            self.x - e.left,
            self.y - e.top,
            (self.w + e.left + e.right).max(0.0),
            (self.h + e.top + e.bottom).max(0.0),
        )
    }
}

/// `a · b`: applies `b` first, then `a` (PDF matrix order `[a b c d e f]`).
pub(crate) fn mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// The inverse of `m`, when it has one.
pub(crate) fn invert(m: [f32; 6]) -> Option<[f32; 6]> {
    let [a, b, c, d, e, f] = m;
    let det = a * d - b * c;
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let inv = [d / det, -b / det, -c / det, a / det, 0.0, 0.0];
    let r = [
        inv[0],
        inv[1],
        inv[2],
        inv[3],
        -(inv[0] * e + inv[2] * f),
        -(inv[1] * e + inv[3] * f),
    ];
    r.iter().all(|v| v.is_finite()).then_some(r)
}

/// Point `(x, y)` transformed by `m`.
pub(crate) fn apply(m: [f32; 6], (x, y): (f32, f32)) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}
