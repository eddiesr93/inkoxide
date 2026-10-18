//! `shapeOutside`: where a float takes room from the lines beside it.
//!
//! A float's shape is cut into horizontal strips of its margin box. Each
//! strip takes the room of the shape's widest part in it, widened by
//! `shapeMargin` and clipped to the margin box (CSS Shapes §3), and a
//! line takes the room of every strip it meets: a strip is never narrower
//! than the shape at any height in it, so no line overlaps the shape.
//! The float itself is placed, and boxes beside it narrowed, by its
//! margin box, as in CSS.

use std::sync::Arc;

use crate::schema::{LengthPercent, ShapeOutside};

/// Fewest points a strip is high.
const STRIP: f32 = 2.0;
/// Most strips of one float.
const MAX_STRIPS: usize = 128;

/// A strip of a float's margin box, from `top` to `bottom` (down from its
/// top), and the extent of its shape there, from its left edge; `None`
/// where the shape is not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Strip {
    pub top: f32,
    pub bottom: f32,
    pub extent: Option<(f32, f32)>,
}

/// A shape resolved in a margin box.
enum Resolved {
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Rect {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        r: f32,
    },
    Polygon(Vec<(f32, f32)>),
}

/// The strips of `shape` with `margin` around it, in a margin box `w` ×
/// `h`; empty when the box has no height.
pub(crate) fn strips(shape: &ShapeOutside, margin: f32, w: f32, h: f32) -> Arc<[Strip]> {
    if !(h > 0.0 && w >= 0.0) {
        return Arc::from([]);
    }
    let resolved = resolve(shape, w, h);
    // At least `STRIP` high each (one when the box is lower), and at most
    // `MAX_STRIPS`.
    let n = ((h / STRIP).floor() as usize).clamp(1, MAX_STRIPS);
    let step = h / n as f32;
    (0..n)
        .map(|i| {
            let top = i as f32 * step;
            let bottom = if i + 1 == n { h } else { top + step };
            let extent = resolved
                .extent(top - margin, bottom + margin)
                .map(|(l, r)| ((l - margin).max(0.0), (r + margin).min(w)))
                .filter(|(l, r)| l < r);
            Strip {
                top,
                bottom,
                extent,
            }
        })
        .collect()
}

fn resolve(shape: &ShapeOutside, w: f32, h: f32) -> Resolved {
    let at = |at: Option<[LengthPercent; 2]>| {
        at.map_or((w / 2.0, h / 2.0), |[x, y]| (x.resolve(w), y.resolve(h)))
    };
    match shape {
        ShapeOutside::Circle(c) => {
            let (cx, cy) = at(c.at);
            // CSS: a percentage of the diagonal over √2; by default to the
            // closest side.
            let r = c.radius.map_or_else(
                || {
                    cx.abs()
                        .min((w - cx).abs())
                        .min(cy.abs())
                        .min((h - cy).abs())
                },
                |r| r.resolve(w.hypot(h) / std::f32::consts::SQRT_2),
            );
            Resolved::Ellipse {
                cx,
                cy,
                rx: r.max(0.0),
                ry: r.max(0.0),
            }
        }
        ShapeOutside::Ellipse(e) => {
            let (cx, cy) = at(e.at);
            let rx =
                e.rx.map_or_else(|| cx.abs().min((w - cx).abs()), |r| r.resolve(w));
            let ry =
                e.ry.map_or_else(|| cy.abs().min((h - cy).abs()), |r| r.resolve(h));
            Resolved::Ellipse {
                cx,
                cy,
                rx: rx.max(0.0),
                ry: ry.max(0.0),
            }
        }
        ShapeOutside::Inset(i) => {
            let side = |v: Option<LengthPercent>, of: f32| v.map_or(0.0, |v| v.resolve(of));
            let (x0, x1) = (side(i.left, w), w - side(i.right, w));
            let (y0, y1) = (side(i.top, h), h - side(i.bottom, h));
            let r = i
                .radius
                .unwrap_or(0.0)
                .min((x1 - x0) / 2.0)
                .min((y1 - y0) / 2.0)
                .max(0.0);
            Resolved::Rect { x0, y0, x1, y1, r }
        }
        ShapeOutside::Polygon(points) => Resolved::Polygon(
            points
                .iter()
                .map(|[x, y]| (x.resolve(w), y.resolve(h)))
                .collect(),
        ),
    }
}

impl Resolved {
    /// The leftmost and rightmost points of the shape from `y0` down to
    /// `y1`, or `None` when it is not there.
    fn extent(&self, y0: f32, y1: f32) -> Option<(f32, f32)> {
        match *self {
            Self::Ellipse { cx, cy, rx, ry } => {
                // Widest at the height nearest the centre.
                let y = cy.clamp(y0, y1);
                let dy = y - cy;
                if ry <= 0.0 || dy.abs() > ry {
                    return None;
                }
                let half = rx * (1.0 - (dy / ry).powi(2)).max(0.0).sqrt();
                Some((cx - half, cx + half))
            }
            Self::Rect {
                x0,
                y0: top,
                x1,
                y1: bottom,
                r,
            } => {
                if x1 < x0 || y1 < top || y0 > bottom {
                    return None;
                }
                // Widest between the rounded corners; else at the height
                // nearest them, the corners' arcs set it in.
                let y = (top + r)
                    .clamp(y0, y1)
                    .max((bottom - r).min(y1).min(top + r));
                let y = y.clamp(top, bottom);
                let dy = if y < top + r {
                    top + r - y
                } else if y > bottom - r {
                    y - (bottom - r)
                } else {
                    0.0
                };
                let inset = r - (r * r - dy * dy).max(0.0).sqrt();
                Some((x0 + inset, x1 - inset))
            }
            Self::Polygon(ref points) => {
                // The region's extremes in a band are on its outline: the
                // edges, cut to the band.
                let mut extent: Option<(f32, f32)> = None;
                let mut add = |x: f32| {
                    extent = Some(extent.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
                };
                for (k, &(ax, ay)) in points.iter().enumerate() {
                    let (bx, by) = points[(k + 1) % points.len()];
                    let (lo, hi) = (ay.min(by), ay.max(by));
                    if hi < y0 || lo > y1 {
                        continue;
                    }
                    if (by - ay).abs() <= f32::EPSILON {
                        add(ax);
                        add(bx);
                        continue;
                    }
                    let x_at = |y: f32| ax + (bx - ax) * (y - ay) / (by - ay);
                    add(x_at(lo.max(y0)));
                    add(x_at(hi.min(y1)));
                }
                extent
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ShapeCircle, ShapeEllipse, ShapeInset};

    fn pct(v: f32) -> LengthPercent {
        LengthPercent::Percent(v)
    }

    fn pt(v: f32) -> LengthPercent {
        LengthPercent::Points(v)
    }

    #[test]
    fn a_circle_is_widest_at_its_centre_and_absent_outside() {
        let circle = ShapeOutside::Circle(ShapeCircle::default());
        let s = strips(&circle, 0.0, 100.0, 100.0);
        assert_eq!(s.len(), 50);
        // The middle strips reach the sides, the top one barely.
        let mid = s[25].extent.unwrap();
        assert!(mid.0 < 1.0 && mid.1 > 99.0, "{mid:?}");
        let top = s[0].extent.unwrap();
        assert!(top.0 > 30.0 && top.1 < 70.0, "{top:?}");
        // A smaller circle at the top leaves the strips below it empty.
        let small = ShapeOutside::Circle(ShapeCircle {
            radius: Some(pt(10.0)),
            at: Some([pct(0.5), pt(10.0)]),
        });
        let s = strips(&small, 0.0, 100.0, 100.0);
        assert!(s[0].extent.is_some() && s[20].extent.is_none());
        // A margin widens it, within the margin box.
        let s = strips(&small, 5.0, 100.0, 100.0);
        assert_eq!(s[4].extent.map(|(l, r)| r - l), Some(30.0));
        assert!(s[13].extent.is_none() && s[12].extent.is_some());
    }

    #[test]
    fn ellipses_insets_and_polygons_take_their_extent() {
        let ellipse = ShapeOutside::Ellipse(ShapeEllipse {
            rx: Some(pct(0.25)),
            ry: None,
            at: None,
        });
        let s = strips(&ellipse, 0.0, 200.0, 100.0);
        assert_eq!(s[25].extent, Some((50.0, 150.0)));
        let inset = ShapeOutside::Inset(ShapeInset {
            top: Some(pt(10.0)),
            left: Some(pct(0.1)),
            radius: Some(10.0),
            ..ShapeInset::default()
        });
        let s = strips(&inset, 0.0, 100.0, 100.0);
        assert!(s[0].extent.is_none());
        // In a rounded corner, then between them.
        let corner = s[5].extent.unwrap();
        assert!(corner.0 > 10.0 && corner.1 < 100.0, "{corner:?}");
        assert_eq!(s[25].extent, Some((10.0, 100.0)));
        // A triangle, pointing down.
        let triangle = ShapeOutside::Polygon(vec![
            [pct(0.0), pct(0.0)],
            [pct(1.0), pct(0.0)],
            [pct(0.5), pct(1.0)],
        ]);
        let s = strips(&triangle, 0.0, 100.0, 100.0);
        assert_eq!(s[0].extent, Some((0.0, 100.0)));
        let low = s[49].extent.unwrap();
        assert!(low.0 >= 48.0 && low.1 <= 52.0, "{low:?}");
    }

    #[test]
    fn strips_are_bounded() {
        let circle = ShapeOutside::Circle(ShapeCircle::default());
        assert_eq!(strips(&circle, 0.0, 10.0, 1e6).len(), MAX_STRIPS);
        assert_eq!(strips(&circle, 0.0, 10.0, 0.5).len(), 1);
        assert!(strips(&circle, 0.0, 10.0, 0.0).is_empty());
    }

    #[test]
    fn strips_are_at_least_two_points_high() {
        let circle = ShapeOutside::Circle(ShapeCircle::default());
        for h in [3.0, 5.0, 201.0, 255.0] {
            let s = strips(&circle, 0.0, 10.0, h);
            assert!(
                s.iter().all(|s| s.bottom - s.top >= STRIP - 1e-3),
                "{h}: {s:?}"
            );
        }
        assert_eq!(strips(&circle, 0.0, 10.0, 201.0).len(), 100);
    }
}
