//! Box painting: background, borders, clipping, opacity, rotation, then
//! content and children, in document order.

use super::shapes::{inset_radii, line, polygon, rounded_rect, uniform_radii, Radii, NO_RADII};
use super::{Dash, FillRule, Op, Path};
use crate::layout::table::TableLayout;
use crate::layout::{Geometry, NodeIdx, Part, Tree};
use crate::paginate::Placement;
use crate::schema::{BorderStyle, Color, Display, Node, Overflow, Style};

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

/// What a leaf painter needs to know about the node it paints.
pub(crate) struct ContentCtx<'p> {
    /// Translation from flow coordinates to page coordinates, horizontally.
    pub dx: f32,
    /// Translation from paged coordinates to page coordinates, vertically.
    pub dy: f32,
    /// The node's shift into paged space.
    pub shift: f32,
    /// Per-line shifts of a split paragraph (empty: use `shift`).
    pub line_shifts: &'p [f32],
    /// Paged y range shown on this page (`None`: everything).
    pub window: Option<(f32, f32)>,
    /// Cumulative opacity in effect; content with its own alpha multiplies it.
    pub opacity: f32,
}

/// Paints leaf content (text, images, tables) inside a node's content box.
pub(crate) trait ContentPainter {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>);
}

/// Which part of a flow to paint.
#[derive(Clone, Copy)]
pub(crate) enum FlowPart<'p> {
    /// Everything, unpaginated (headers, footers).
    All,
    /// Page `page` of a paginated flow, without its fixed nodes.
    Page { placement: &'p Placement, page: u32 },
    /// The flow's fixed nodes, repeated on page `page`.
    Fixed { placement: &'p Placement, page: u32 },
}

/// Paints part of `tree`. `dx`/`dy` translate flow (or paged) coordinates
/// to page coordinates.
pub(crate) fn paint_flow(
    tree: &Tree<'_>,
    part: FlowPart<'_>,
    dx: f32,
    dy: f32,
    content: &mut dyn ContentPainter,
    ops: &mut Vec<Op>,
) {
    let (placement, page, fixed_only) = match part {
        FlowPart::All => (None, 0, false),
        FlowPart::Page { placement, page } => (Some(placement), page, false),
        FlowPart::Fixed { placement, page } => (Some(placement), page, true),
    };
    let mut p = BoxPainter {
        tree,
        placement,
        page,
        dx,
        dy,
        override_shift: None,
        opacity: 1.0,
        content,
        ops,
    };
    match (fixed_only, placement) {
        (true, Some(pl)) => {
            // Fixed nodes keep their position relative to their first page.
            let h = pl.page_h;
            for &i in &pl.fixed {
                let first = pl.page_range[i as usize].0;
                p.dy = dy + (page as f32 - first as f32) * h;
                p.placement = Some(pl);
                p.node(i, true);
            }
        }
        _ => {
            for c in tree.children(0) {
                p.node(c, false);
            }
        }
    }
}

struct BoxPainter<'t, 'a, 'c, 'p> {
    tree: &'t Tree<'a>,
    placement: Option<&'p Placement>,
    page: u32,
    dx: f32,
    dy: f32,
    /// Paints a subtree at a fixed shift, ignoring pagination (repeated
    /// table header rows).
    override_shift: Option<f32>,
    /// Cumulative opacity of the ancestors.
    opacity: f32,
    content: &'c mut dyn ContentPainter,
    ops: &'c mut Vec<Op>,
}

impl BoxPainter<'_, '_, '_, '_> {
    fn node(&mut self, i: NodeIdx, as_fixed: bool) {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else {
            // Rows and cells: the table paints their backgrounds and borders.
            if let (Some(pl), None, false) = (self.placement, self.override_shift, as_fixed) {
                if !pl.on_page(i, self.page) {
                    return;
                }
            }
            for c in self.tree.children(i) {
                self.node(c, as_fixed);
            }
            return;
        };
        let s = &node.style;
        if s.display == Some(Display::None) {
            return;
        }
        let window = match self.placement {
            Some(_) if self.override_shift.is_some() => None,
            Some(pl) if !as_fixed => {
                // Fixed nodes are painted by the fixed pass on every page.
                if node.brk.fixed || !pl.on_page(i, self.page) {
                    return;
                }
                let h = pl.page_h;
                Some((self.page as f32 * h, (self.page + 1) as f32 * h))
            }
            _ => None,
        };
        let g = ln.geo;
        let (shift, extra) = match (self.override_shift, self.placement) {
            (Some(o), _) => (o, 0.0),
            (None, Some(pl)) => {
                let st = pl.shift_top[i as usize];
                (st, pl.shift_bottom[i as usize] - st)
            }
            (None, None) => (0.0, 0.0),
        };
        let (x, y, w, h) = (g.x + self.dx, g.y + shift + self.dy, g.w, g.h + extra);
        if let Some(target) = &node.link {
            // Only the part of the box shown on this page.
            let (top, bottom) = match window {
                Some((w0, w1)) => (y.max(w0 + self.dy), (y + h).min(w1 + self.dy)),
                None => (y, y + h),
            };
            if bottom > top && w > 0.0 {
                self.ops.push(Op::Link {
                    rect: super::Rect::new(x, top, w, bottom - top),
                    target: target.clone(),
                });
            }
        }

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
        self.borders(s, g.border, w, x, y, h, &radii);

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

        // After the clip: row backgrounds must not escape rounded corners.
        let table = self.tree.tables.get(&i).cloned();
        if let Some(t) = &table {
            self.table_body(i, &g, t);
        }

        let ctx = ContentCtx {
            dx: self.dx,
            dy: self.dy,
            shift,
            line_shifts: match (self.override_shift, self.placement) {
                (None, Some(pl)) => pl.line_shifts[i as usize].as_slice(),
                _ => &[][..],
            },
            window,
            opacity: self.opacity,
        };
        self.content.paint(node, &g, &ctx, self.ops);
        for c in self.tree.children(i) {
            self.node(c, as_fixed);
        }
        if let Some(t) = &table {
            self.repeat_header(i, &g, t);
        }

        if saved {
            self.ops.push(Op::Restore);
        }
        self.opacity = outer_opacity;
    }

    /// Shift of table row `r` (paged space).
    fn row_shift(&self, rows: &[NodeIdx], r: usize) -> f32 {
        match (self.override_shift, self.placement) {
            (Some(o), _) => o,
            (None, Some(pl)) => pl.shift_top[rows[r] as usize],
            (None, None) => 0.0,
        }
    }

    fn row_visible(&self, rows: &[NodeIdx], r: usize) -> bool {
        match (self.override_shift, self.placement) {
            (None, Some(pl)) => pl.on_page(rows[r], self.page),
            _ => true,
        }
    }

    /// Row and cell backgrounds and collapsed borders of the rows on this
    /// page (or of the header rows, when repeating them).
    fn table_body(&mut self, i: NodeIdx, g: &Geometry, t: &TableLayout) {
        let rows: Vec<NodeIdx> = self.tree.children(i).collect();
        let n = rows.len();
        let range = if self.override_shift.is_some() {
            0..t.header_rows
        } else {
            0..n
        };
        // Rows on this page. Only `lo..hi` is scanned below, so painting a
        // long table costs O(rows on the page), not O(rows) per page.
        let in_page = |r: usize| range.contains(&r) && self.row_visible(&rows, r);
        let lo = range.clone().find(|&r| in_page(r)).unwrap_or(n);
        let hi = range
            .clone()
            .rev()
            .find(|&r| in_page(r))
            .map_or(lo, |r| r + 1);
        let visible = |r: usize| r >= lo && r < hi && in_page(r);
        let shift = |r: usize| self.row_shift(&rows, r);
        let (cx, cy, _, _) = g.content_box();
        let (ox, oy) = (cx + self.dx, cy + self.dy);

        let mut fills = Vec::new();
        let first_cell = t.cells.partition_point(|c| c.row < lo);
        if t.separate {
            let cells: Vec<(usize, f32)> = (first_cell..t.cells.len())
                .take_while(|&k| t.cells[k].row < hi)
                .filter(|&k| visible(t.cells[k].row))
                .map(|k| (k, shift(t.cells[k].row)))
                .collect();
            self.separate_cells(t, &rows, &cells, ox, oy);
            return;
        }
        for (r, &row_idx) in rows.iter().enumerate().take(hi).skip(lo) {
            if !visible(r) {
                continue;
            }
            if let Part::Row(row) = self.tree.nodes[row_idx as usize].part {
                if let Some(bg) = row.background_color.filter(|c| c[3] > 0.0) {
                    fills.push((ox, oy + t.row_y[r] + shift(r), t.width, t.row_h[r], bg));
                }
            }
        }
        for cell in t.cells[first_cell..].iter().take_while(|c| c.row < hi) {
            if !visible(cell.row) {
                continue;
            }
            let style = self.tree.nodes[cell.idx as usize].style();
            if let Some(bg) = style.background_color.filter(|c| c[3] > 0.0) {
                fills.push((
                    ox + cell.x,
                    oy + cell.y + shift(cell.row),
                    cell.w,
                    cell.h,
                    bg,
                ));
            }
        }

        let repeating_header = t.header_rows > 0 && self.override_shift.is_none();
        let first_h = t.h_edges.partition_point(|e| e.line < lo);
        for e in t.h_edges[first_h..].iter().take_while(|e| e.line <= hi) {
            let (x, w) = (ox + t.col_x[e.col], t.col_w[e.col]);
            let l = e.line;
            if l == 0 {
                if visible(0) {
                    fills.push((x, oy + shift(0), w, e.width, e.color));
                }
            } else if l == n {
                if visible(n - 1) {
                    fills.push((
                        x,
                        oy + t.row_y[n - 1] + t.row_h[n - 1] - e.width + shift(n - 1),
                        w,
                        e.width,
                        e.color,
                    ));
                }
            } else if (shift(l - 1) - shift(l)).abs() < 1e-3 {
                if visible(l - 1) || visible(l) {
                    fills.push((
                        x,
                        oy + t.row_y[l] - e.width / 2.0 + shift(l),
                        w,
                        e.width,
                        e.color,
                    ));
                }
            } else {
                // The line falls on a page break: close the fragment above,
                // and open the one below unless a repeated header does.
                if visible(l - 1) {
                    fills.push((
                        x,
                        oy + t.row_y[l] - e.width + shift(l - 1),
                        w,
                        e.width,
                        e.color,
                    ));
                }
                if visible(l) && !(repeating_header && l > t.header_rows) {
                    fills.push((x, oy + t.row_y[l] + shift(l), w, e.width, e.color));
                }
            }
        }
        let first_v = t.v_edges.partition_point(|e| e.row < lo);
        for e in t.v_edges[first_v..].iter().take_while(|e| e.row < hi) {
            if !visible(e.row) {
                continue;
            }
            let lx = if e.line < t.col_x.len() {
                t.col_x[e.line]
            } else {
                t.width
            };
            let x = if e.line == 0 {
                lx
            } else if e.line == t.col_x.len() {
                lx - e.width
            } else {
                lx - e.width / 2.0
            };
            let h: f32 = t.row_h[e.row..e.row + e.rows].iter().sum();
            fills.push((
                ox + x,
                oy + t.row_y[e.row] + shift(e.row),
                e.width,
                h,
                e.color,
            ));
        }
        for (x, y, w, h, color) in fills {
            self.fill(rounded_rect(x, y, w, h, &NO_RADII), color);
        }
    }

    /// Separate borders: each visible cell paints the row background, its
    /// own background and its own borders, with its `borderRadius`.
    /// `cells`: (index into `t.cells`, row shift) of the visible cells.
    fn separate_cells(
        &mut self,
        t: &TableLayout,
        rows: &[NodeIdx],
        cells: &[(usize, f32)],
        ox: f32,
        oy: f32,
    ) {
        for &(k, shift) in cells {
            let cell = &t.cells[k];
            let style = self.tree.nodes[cell.idx as usize].style();
            let (x, y, w, h) = (ox + cell.x, oy + cell.y + shift, cell.w, cell.h);
            let radii = match style.border_radius {
                Some(r) if r > 0.0 => uniform_radii(r, w, h),
                _ => NO_RADII,
            };
            let row_bg = match self.tree.nodes[rows[cell.row] as usize].part {
                Part::Row(row) => row.background_color,
                _ => None,
            };
            for bg in [row_bg, style.background_color].into_iter().flatten() {
                if bg[3] > 0.0 {
                    self.fill(rounded_rect(x, y, w, h, &radii), bg);
                }
            }
            self.borders(style, cell.insets, w, x, y, h, &radii);
        }
    }

    /// On continuation pages, repaints the header rows at the top of the
    /// page, above the first body row (which pagination pushed down).
    fn repeat_header(&mut self, i: NodeIdx, g: &Geometry, t: &TableLayout) {
        let (Some(pl), None) = (self.placement, self.override_shift) else {
            return;
        };
        if t.header_rows == 0 {
            return;
        }
        let rows: Vec<NodeIdx> = self.tree.children(i).collect();
        let page_top = self.page as f32 * pl.page_h;
        // First body row on this page, if it starts below a reserved header.
        let Some(first) = (t.header_rows..rows.len()).find(|&r| pl.on_page(rows[r], self.page))
        else {
            return;
        };
        let (_, cy, _, _) = g.content_box();
        let first_top = cy + t.row_y[first] + pl.shift_top[rows[first] as usize];
        if (first_top - (page_top + t.header_height())).abs() > crate::paginate::EPS {
            return;
        }
        let shift = page_top - (cy + t.row_y[0]);
        self.override_shift = Some(shift);
        self.table_body(i, g, t);
        for &r in &rows[..t.header_rows] {
            self.node(r, false);
        }
        self.override_shift = None;
    }

    fn fill(&mut self, path: Path, color: Color) {
        self.with_alpha(color[3], |ops| {
            ops.push(Op::FillPath {
                path,
                color,
                rule: FillRule::NonZero,
            });
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

    /// `h` is the painted (paged) height, which can exceed the laid-out
    /// height when the box is split across pages.
    #[allow(clippy::too_many_arguments)]
    fn borders(
        &mut self,
        s: &Style,
        widths: [f32; 4],
        w: f32,
        x: f32,
        y: f32,
        h: f32,
        radii: &Radii,
    ) {
        if widths.iter().all(|w| *w <= 0.0) {
            return;
        }
        let colors: [Color; 4] = s.border_color.map_or([BLACK; 4], |c| c.get(BLACK));
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
                });
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
                    });
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
                    });
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
            if uniform && !square {
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
            });
        });
    }
}
