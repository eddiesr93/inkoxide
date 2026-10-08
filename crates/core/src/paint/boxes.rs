//! Box painting: background, borders, clipping, opacity, rotation, then
//! children, in document order.

use super::shapes::{inset_radii, line, polygon, rounded_rect, uniform_radii, Radii, NO_RADII};
use super::{Dash, FillRule, Op, Path};
use crate::layout::{Geometry, NodeIdx, Tree};
use crate::schema::{BorderStyle, Color, Display, Node, Overflow};

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

/// Paints leaf content (text, images, tables) inside a node's content box.
pub(crate) trait ContentPainter {
    /// `opacity` is the cumulative opacity in effect; content with its own
    /// alpha (text colour) must multiply it.
    fn paint(
        &mut self,
        node: &Node,
        geo: &Geometry,
        dx: f32,
        dy: f32,
        opacity: f32,
        ops: &mut Vec<Op>,
    );
}

/// Paints every node of `tree` (except the synthetic root) translated by
/// `(dx, dy)`.
pub(crate) fn paint_flow(
    tree: &Tree<'_>,
    dx: f32,
    dy: f32,
    content: &mut dyn ContentPainter,
    ops: &mut Vec<Op>,
) {
    let mut p = BoxPainter {
        tree,
        dx,
        dy,
        opacity: 1.0,
        content,
        ops,
    };
    for c in tree.children(0) {
        p.node(c);
    }
}

struct BoxPainter<'t, 'a, 'c> {
    tree: &'t Tree<'a>,
    dx: f32,
    dy: f32,
    /// Cumulative opacity of the ancestors.
    opacity: f32,
    content: &'c mut dyn ContentPainter,
    ops: &'c mut Vec<Op>,
}

impl BoxPainter<'_, '_, '_> {
    fn node(&mut self, i: NodeIdx) {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else { return };
        let s = &node.style;
        if s.display == Some(Display::None) {
            return;
        }
        let g = ln.geo;
        let (x, y, w, h) = (g.x + self.dx, g.y + self.dy, g.w, g.h);

        let opacity = s.opacity.unwrap_or(1.0);
        if opacity <= 0.0 {
            return;
        }
        let rotate = s.rotate.filter(|r| *r % 360.0 != 0.0);
        let clip = s.overflow == Some(Overflow::Hidden);
        let saved = opacity < 1.0 || rotate.is_some() || clip;
        let outer_opacity = self.opacity;
        if saved {
            self.ops.push(Op::Save);
        }
        if opacity < 1.0 {
            self.opacity *= opacity;
            self.ops.push(Op::SetOpacity(self.opacity));
        }
        if let Some(deg) = rotate {
            let (sin, cos) = deg.to_radians().sin_cos();
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            // translate(c) · rotate · translate(-c); y-down, so positive
            // angles turn clockwise like CSS.
            self.ops.push(Op::Transform([
                cos,
                sin,
                -sin,
                cos,
                cx - cos * cx + sin * cy,
                cy - sin * cx - cos * cy,
            ]));
        }

        let radii = match s.border_radius {
            Some(r) if r > 0.0 => uniform_radii(r, w, h),
            _ => NO_RADII,
        };
        if let Some(bg) = s.background_color.filter(|c| c[3] > 0.0) {
            self.fill(rounded_rect(x, y, w, h, &radii), bg);
        }
        self.borders(node, &g, x, y, &radii);

        if clip {
            let b = g.border;
            let inner = inset_radii(&radii, b);
            self.ops.push(Op::ClipPath {
                path: rounded_rect(
                    x + b[3],
                    y + b[0],
                    (w - b[1] - b[3]).max(0.0),
                    (h - b[0] - b[2]).max(0.0),
                    &inner,
                ),
                rule: FillRule::NonZero,
            });
        }

        self.content
            .paint(node, &g, self.dx, self.dy, self.opacity, self.ops);
        for c in self.tree.children(i) {
            self.node(c);
        }

        if saved {
            self.ops.push(Op::Restore);
        }
        self.opacity = outer_opacity;
    }

    fn fill(&mut self, path: Path, color: Color) {
        self.with_alpha(color[3], |ops| {
            ops.push(Op::FillPath {
                path,
                color,
                rule: FillRule::NonZero,
            })
        });
    }

    /// Runs `f` with the colour's alpha folded into the current opacity.
    fn with_alpha(&mut self, alpha: f32, f: impl FnOnce(&mut Vec<Op>)) {
        if alpha >= 1.0 {
            f(self.ops);
            return;
        }
        self.ops.push(Op::Save);
        self.ops.push(Op::SetOpacity(self.opacity * alpha));
        f(self.ops);
        self.ops.push(Op::Restore);
    }

    fn borders(&mut self, node: &Node, g: &Geometry, x: f32, y: f32, radii: &Radii) {
        let s = &node.style;
        let widths = g.border;
        if widths.iter().all(|w| *w <= 0.0) {
            return;
        }
        let colors: [Color; 4] = s.border_color.map_or([BLACK; 4], |c| c.get(BLACK));
        let (w, h) = (g.w, g.h);
        let [bt, br, bb, bl] = widths;
        let inner_radii = inset_radii(radii, widths);
        let ix = x + bl;
        let iy = y + bt;
        let iw = (w - bl - br).max(0.0);
        let ih = (h - bt - bb).max(0.0);

        // The ring between the border box and the padding box.
        let mut ring = rounded_rect(x, y, w, h, radii);
        ring.segs
            .extend(rounded_rect(ix, iy, iw, ih, &inner_radii).segs);

        let active: Vec<usize> = (0..4).filter(|&k| widths[k] > 0.0).collect();
        let same_color = active.windows(2).all(|p| colors[p[0]] == colors[p[1]]);
        let style = s.border_style.unwrap_or(BorderStyle::Solid);

        let (r, b) = (x + w, y + h);
        let (ir, ib) = (ix + iw, iy + ih);
        // Mitred trapezoid per side.
        let sides = [
            [(x, y), (r, y), (ir, iy), (ix, iy)],
            [(r, y), (r, b), (ir, ib), (ir, iy)],
            [(r, b), (x, b), (ix, ib), (ir, ib)],
            [(x, b), (x, y), (ix, iy), (ix, ib)],
        ];
        let square = crate::paint::shapes::is_square(radii);

        if style == BorderStyle::Solid && same_color && (active.len() == 4 || !square) {
            let color = colors[active[0]];
            self.with_alpha(color[3], |ops| {
                ops.push(Op::FillPath {
                    path: ring,
                    color,
                    rule: FillRule::EvenOdd,
                })
            });
            return;
        }
        if style == BorderStyle::Solid && square {
            // No ring needed: avoids hairline seams where the outer and
            // inner edges of a zero-width side coincide.
            for &k in &active {
                let color = colors[k];
                let path = polygon(&sides[k]);
                self.with_alpha(color[3], |ops| {
                    ops.push(Op::FillPath {
                        path,
                        color,
                        rule: FillRule::NonZero,
                    })
                });
            }
            return;
        }

        self.ops.push(Op::Save);
        self.ops.push(Op::ClipPath {
            path: ring,
            rule: FillRule::EvenOdd,
        });
        if style == BorderStyle::Solid {
            // Rounded, several colours: trapezoids cut by the ring clip.
            for &k in &active {
                let color = colors[k];
                let path = polygon(&sides[k]);
                self.with_alpha(color[3], |ops| {
                    ops.push(Op::FillPath {
                        path,
                        color,
                        rule: FillRule::NonZero,
                    })
                });
            }
        } else {
            // Dashed / dotted: stroke along the middle of each side.
            let mid = [
                line(x, y + bt / 2.0, r, y + bt / 2.0),
                line(r - br / 2.0, y, r - br / 2.0, b),
                line(r, b - bb / 2.0, x, b - bb / 2.0),
                line(x + bl / 2.0, b, x + bl / 2.0, y),
            ];
            let uniform =
                active.len() == 4 && widths.windows(2).all(|p| p[0] == p[1]) && same_color;
            if uniform && !crate::paint::shapes::is_square(radii) {
                let half = bt / 2.0;
                let mid_radii = inset_radii(radii, [half; 4]);
                let path = rounded_rect(x + half, y + half, w - bt, h - bt, &mid_radii);
                self.stroke(path, colors[0], bt, style);
            } else {
                for &k in &active {
                    self.stroke(mid[k].clone(), colors[k], widths[k], style);
                }
            }
        }
        self.ops.push(Op::Restore);
    }

    fn stroke(&mut self, path: Path, color: Color, width: f32, style: BorderStyle) {
        let dash = match style {
            BorderStyle::Solid => None,
            BorderStyle::Dashed => Some(Dash {
                pattern: [3.0 * width, 3.0 * width],
                round_cap: false,
            }),
            BorderStyle::Dotted => Some(Dash {
                pattern: [0.0, 2.0 * width],
                round_cap: true,
            }),
        };
        self.with_alpha(color[3], |ops| {
            ops.push(Op::StrokePath {
                path,
                color,
                width,
                dash,
            })
        });
    }
}
