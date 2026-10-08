//! Path construction: rectangles, rounded rectangles, border rings.

use super::{Path, PathSeg};

/// Bézier circle approximation constant (§6.7).
const KAPPA: f32 = 0.552_284_8;

/// Corner radii `(rx, ry)` in order top-left, top-right, bottom-right,
/// bottom-left.
pub(crate) type Radii = [(f32, f32); 4];

pub(crate) const NO_RADII: Radii = [(0.0, 0.0); 4];

pub(crate) fn uniform_radii(r: f32, w: f32, h: f32) -> Radii {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    [(r, r); 4]
}

pub(crate) fn is_square(radii: &Radii) -> bool {
    radii.iter().all(|&(rx, ry)| rx <= 0.0 || ry <= 0.0)
}

/// Inner radii of a ring whose edges are inset by `[top, right, bottom, left]`.
pub(crate) fn inset_radii(radii: &Radii, inset: [f32; 4]) -> Radii {
    let [t, r, b, l] = inset;
    let sub = |(rx, ry): (f32, f32), dx: f32, dy: f32| ((rx - dx).max(0.0), (ry - dy).max(0.0));
    [
        sub(radii[0], l, t),
        sub(radii[1], r, t),
        sub(radii[2], r, b),
        sub(radii[3], l, b),
    ]
}

/// Appends a clockwise (in y-down space) rounded rectangle.
pub(crate) fn push_rounded_rect(p: &mut Path, x: f32, y: f32, w: f32, h: f32, radii: &Radii) {
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

pub(crate) fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: &Radii) -> Path {
    let mut p = Path::default();
    push_rounded_rect(&mut p, x, y, w, h, radii);
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
        let p = rounded_rect(0.0, 0.0, 10.0, 5.0, &NO_RADII);
        assert_eq!(p.segs.len(), 5);
    }

    #[test]
    fn rounded_rect_has_four_arcs() {
        let p = rounded_rect(0.0, 0.0, 10.0, 10.0, &uniform_radii(3.0, 10.0, 10.0));
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
    fn inset_radii_never_negative() {
        let r = inset_radii(&uniform_radii(4.0, 100.0, 100.0), [10.0, 1.0, 1.0, 1.0]);
        assert_eq!(r[0], (3.0, 0.0));
        assert_eq!(r[2], (3.0, 3.0));
    }
}
