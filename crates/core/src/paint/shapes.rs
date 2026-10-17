//! Path construction: rectangles, rounded rectangles, border rings.

use super::{Path, PathSeg};
use crate::geom::Rect;
use crate::schema::Edges;

/// Bézier circle approximation constant.
pub(crate) const KAPPA: f32 = 0.552_284_8;

/// Corner radii `(rx, ry)` in order top-left, top-right, bottom-right,
/// bottom-left.
pub(crate) type Radii = [(f32, f32); 4];

pub(crate) const NO_RADII: Radii = [(0.0, 0.0); 4];

pub(crate) fn uniform_radii(r: f32, w: f32, h: f32) -> Radii {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    [(r, r); 4]
}

/// Per-corner radii (top-left, top-right, bottom-right, bottom-left),
/// scaled down together when two on one side would overlap (CSS
/// Backgrounds §5.5).
pub(crate) fn corner_radii(c: [f32; 4], w: f32, h: f32) -> Radii {
    let c = c.map(|r| r.max(0.0));
    let mut f: f32 = 1.0;
    for (sum, side) in [
        (c[0] + c[1], w),
        (c[3] + c[2], w),
        (c[0] + c[3], h),
        (c[1] + c[2], h),
    ] {
        if sum > side {
            f = f.min(side.max(0.0) / sum);
        }
    }
    c.map(|r| (r * f, r * f))
}

pub(crate) fn is_square(radii: &Radii) -> bool {
    radii.iter().all(|&(rx, ry)| rx <= 0.0 || ry <= 0.0)
}

/// Inner radii of a ring whose edges are inset by `inset`.
pub(crate) fn inset_radii(radii: &Radii, inset: Edges<f32>) -> Radii {
    let Edges {
        top: t,
        right: r,
        bottom: b,
        left: l,
    } = inset;
    let sub = |(rx, ry): (f32, f32), dx: f32, dy: f32| ((rx - dx).max(0.0), (ry - dy).max(0.0));
    [
        sub(radii[0], l, t),
        sub(radii[1], r, t),
        sub(radii[2], r, b),
        sub(radii[3], l, b),
    ]
}

/// Appends a clockwise (in y-down space) rounded rectangle.
pub(crate) fn push_rounded_rect(p: &mut Path, rect: Rect, radii: &Radii) {
    let Rect { x, y, w, h } = rect;
    let segs = &mut p.segs;
    if is_square(radii) {
        segs.extend([
            PathSeg::MoveTo(x, y),
            PathSeg::LineTo(x + w, y),
            PathSeg::LineTo(x + w, y + h),
            PathSeg::LineTo(x, y + h),
            PathSeg::Close,
        ]);
        return;
    }
    let [(tlx, tly), (trx, try_), (brx, bry), (blx, bly)] = *radii;
    let (r, b) = (x + w, y + h);
    segs.push(PathSeg::MoveTo(x + tlx, y));
    segs.push(PathSeg::LineTo(r - trx, y));
    if trx > 0.0 && try_ > 0.0 {
        segs.push(PathSeg::CubicTo(
            r - trx + KAPPA * trx,
            y,
            r,
            y + try_ - KAPPA * try_,
            r,
            y + try_,
        ));
    }
    segs.push(PathSeg::LineTo(r, b - bry));
    if brx > 0.0 && bry > 0.0 {
        segs.push(PathSeg::CubicTo(
            r,
            b - bry + KAPPA * bry,
            r - brx + KAPPA * brx,
            b,
            r - brx,
            b,
        ));
    }
    segs.push(PathSeg::LineTo(x + blx, b));
    if blx > 0.0 && bly > 0.0 {
        segs.push(PathSeg::CubicTo(
            x + blx - KAPPA * blx,
            b,
            x,
            b - bly + KAPPA * bly,
            x,
            b - bly,
        ));
    }
    segs.push(PathSeg::LineTo(x, y + tly));
    if tlx > 0.0 && tly > 0.0 {
        segs.push(PathSeg::CubicTo(
            x,
            y + tly - KAPPA * tly,
            x + tlx - KAPPA * tlx,
            y,
            x + tlx,
            y,
        ));
    }
    segs.push(PathSeg::Close);
}

pub(crate) fn rounded_rect(rect: Rect, radii: &Radii) -> Path {
    let mut p = Path::default();
    push_rounded_rect(&mut p, rect, radii);
    p
}

pub(crate) fn polygon(points: &[(f32, f32)]) -> Path {
    let mut segs = Vec::with_capacity(points.len() + 1);
    for (i, &(x, y)) in points.iter().enumerate() {
        segs.push(if i == 0 {
            PathSeg::MoveTo(x, y)
        } else {
            PathSeg::LineTo(x, y)
        });
    }
    segs.push(PathSeg::Close);
    Path { segs }
}

pub(crate) fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Path {
    Path {
        segs: vec![PathSeg::MoveTo(x1, y1), PathSeg::LineTo(x2, y2)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_rect_has_four_lines() {
        let p = rounded_rect(Rect::new(0.0, 0.0, 10.0, 5.0), &NO_RADII);
        assert_eq!(p.segs.len(), 5);
    }

    #[test]
    fn rounded_rect_has_four_arcs() {
        let p = rounded_rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            &uniform_radii(3.0, 10.0, 10.0),
        );
        let arcs = p
            .segs
            .iter()
            .filter(|s| matches!(s, PathSeg::CubicTo(..)))
            .count();
        assert_eq!(arcs, 4);
    }

    #[test]
    fn radii_are_clamped_to_half_the_box() {
        assert_eq!(uniform_radii(50.0, 10.0, 20.0)[0], (5.0, 5.0));
    }

    #[test]
    fn corner_radii_scale_together() {
        let r = corner_radii([20.0, 20.0, 0.0, 5.0], 20.0, 100.0);
        assert_eq!(r[0], (10.0, 10.0));
        assert_eq!(r[1], (10.0, 10.0));
        assert_eq!(r[3], (2.5, 2.5));
        assert_eq!(
            corner_radii([4.0, 0.0, 0.0, 0.0], 100.0, 100.0)[0],
            (4.0, 4.0)
        );
    }

    #[test]
    fn inset_radii_never_negative() {
        let inset = Edges::from([10.0, 1.0, 1.0, 1.0]);
        let r = inset_radii(&uniform_radii(4.0, 100.0, 100.0), inset);
        assert_eq!(r[0], (3.0, 0.0));
        assert_eq!(r[2], (3.0, 3.0));
    }
}
