//! Box painting: background, borders, clipping, opacity, rotation, then
//! content and children, in document order.

use super::debug::{DebugBox, DebugBoxes, DebugLine};
use super::shapes::{
    corner_radii, inset_radii, line, polygon, rounded_rect, uniform_radii, Radii, NO_RADII,
};
use super::{with_alpha, Dash, FillRule, Op, Path, Rect};
use crate::layout::table::TableLayout;
use crate::layout::{Geometry, LineBoxes, NodeIdx, Part, Tree};
use crate::paginate::{PageWindow, Placement, PAGED_EPS};
use crate::schema::{
    BorderRadius, BorderStyle, BorderStyles, Color, Display, Edges, LengthPercent, Node, Overflow,
    Style, TransformOp, BLACK,
};
use crate::text::PageNumbers;

/// Row shifts closer than this (points) are equal: no page break falls
/// between the rows.
const SHIFT_EPS: f32 = 1e-3;

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
    pub window: Option<PageWindow>,
    /// Cumulative opacity in effect; content with its own alpha multiplies it.
    pub opacity: f32,
    /// The line boxes of text laid out beside floats (`None`: its content
    /// width).
    pub line_boxes: Option<&'p LineBoxes>,
}

/// Paints leaf content (text, images, tables) inside a node's content box.
pub(crate) trait ContentPainter {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>);

    /// [`paint`](Self::paint) for a node with a `debug` outline, recording
    /// the lines of a paragraph, and the runs on them, in `lines`.
    fn paint_outlined(
        &mut self,
        node: &Node,
        geo: &Geometry,
        ctx: &ContentCtx<'_>,
        ops: &mut Vec<Op>,
        lines: &mut Vec<DebugLine>,
    ) {
        let _ = lines;
        self.paint(node, geo, ctx, ops);
    }

    /// Paints the `backgroundImage` of `node` into `padding_box` (page
    /// coordinates), clipped to it rounded by `radii`.
    fn background(&mut self, _node: &Node, _padding_box: Rect, _radii: &Radii, _ops: &mut Vec<Op>) {
    }

    /// The physical page number and the page count of the page painted:
    /// placeholders show them, and nodes shown on some pages only are
    /// painted by them.
    fn numbers(&self) -> PageNumbers;
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
/// to page coordinates. The boxes of elements with `debug` (of all, with
/// `debug.all`) go to `debug`.
pub(crate) fn paint_flow(
    tree: &Tree<'_>,
    part: FlowPart<'_>,
    dx: f32,
    dy: f32,
    content: &mut dyn ContentPainter,
    ops: &mut Vec<Op>,
    debug: &mut DebugBoxes,
) {
    let mode = match part {
        FlowPart::All => Mode::Unpaged,
        FlowPart::Page { placement, page } => Mode::Paged { placement, page },
        FlowPart::Fixed { placement, page } => Mode::Fixed { placement, page },
    };
    let mut p = BoxPainter {
        tree,
        mode,
        dx,
        dy,
        opacity: 1.0,
        ctm: None,
        content,
        ops,
        debug,
    };
    match mode {
        Mode::Fixed { placement, page } => {
            // Fixed nodes keep their position relative to their first page.
            let h = placement.page_h;
            for &i in &placement.fixed {
                let first = placement.first_page(i);
                p.dy = dy + (page as f32 - first as f32) * h;
                p.node(i);
            }
        }
        _ => {
            for c in paint_order(tree, 0) {
                p.node(c);
            }
        }
    }
}

/// How the nodes of a flow map onto the page being painted.
#[derive(Clone, Copy)]
enum Mode<'p> {
    /// Unpaginated (headers, footers, overlays): every node, unshifted.
    Unpaged,
    /// Page `page` of a paginated flow: the nodes on it, shifted into paged
    /// space, without the fixed nodes.
    Paged { placement: &'p Placement, page: u32 },
    /// The fixed nodes of a paginated flow, repeated on page `page`:
    /// shifted as on their first page, and not cut to the page.
    Fixed { placement: &'p Placement, page: u32 },
    /// The header rows of a table, repeated at `shift` (paged space),
    /// ignoring pagination.
    Header { shift: f32 },
}

impl<'p> Mode<'p> {
    /// The placement and page of a paginated flow.
    fn paged(self) -> Option<(&'p Placement, u32)> {
        match self {
            Mode::Paged { placement, page } | Mode::Fixed { placement, page } => {
                Some((placement, page))
            }
            Mode::Unpaged | Mode::Header { .. } => None,
        }
    }

    /// Whether node `i` is on this page (fixed nodes aside).
    fn shows(self, i: NodeIdx) -> bool {
        match self {
            Mode::Paged { placement, page } => placement.on_page(i, page),
            _ => true,
        }
    }

    /// Paged y range shown on this page (`None`: everything).
    fn window(self) -> Option<PageWindow> {
        match self {
            Mode::Paged { placement, page } => Some(placement.window(page)),
            _ => None,
        }
    }

    /// Shift of node `i` into paged space, and its growth from being split
    /// across pages.
    fn shifts(self, i: NodeIdx) -> (f32, f32) {
        match self {
            Mode::Unpaged => (0.0, 0.0),
            Mode::Paged { placement, .. } | Mode::Fixed { placement, .. } => {
                (placement.shift(i), placement.growth(i))
            }
            Mode::Header { shift } => (shift, 0.0),
        }
    }

    /// Per-line shifts of a split paragraph `i` (empty: use its shift).
    fn line_shifts(self, i: NodeIdx) -> &'p [f32] {
        match self.paged() {
            Some((pl, _)) => pl.line_shifts(i),
            None => &[],
        }
    }
}

struct BoxPainter<'t, 'a, 'c, 'p> {
    tree: &'t Tree<'a>,
    mode: Mode<'p>,
    dx: f32,
    dy: f32,
    /// Cumulative opacity of the ancestors.
    opacity: f32,
    /// Transform in effect (`None`: identity), for `debug` outlines.
    ctm: Option<[f32; 6]>,
    content: &'c mut dyn ContentPainter,
    ops: &'c mut Vec<Op>,
    debug: &'c mut DebugBoxes,
}

impl BoxPainter<'_, '_, '_, '_> {
    fn node(&mut self, i: NodeIdx) {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node() else {
            // Rows and cells: the table paints their backgrounds and borders.
            if !self.mode.shows(i) {
                return;
            }
            let debug = match ln.part {
                Part::Row(r) => r.debug,
                Part::Cell(c) => c.debug,
                _ => false,
            };
            if debug || self.debug.all {
                let (shift, extra) = self.mode.shifts(i);
                self.outline(&ln.geo, shift, extra, self.mode.window(), None);
            }
            for c in self.tree.children(i) {
                self.node(c);
            }
            return;
        };
        let s = &node.style;
        if s.display == Some(Display::None) {
            return;
        }
        if let Some(f) = &node.pages {
            let n = self.content.numbers();
            if !f.shows(n.current, n.total) {
                return;
            }
        }
        // Fixed nodes are painted by the fixed pass on every page.
        let fixed_here = node.brk.fixed && matches!(self.mode, Mode::Paged { .. });
        if fixed_here || !self.mode.shows(i) {
            return;
        }
        let window = self.mode.window();
        let g = ln.geo;
        let (shift, extra) = self.mode.shifts(i);
        let (x, y, w, h) = (g.x + self.dx, g.y + shift + self.dy, g.w, g.h + extra);
        let rotate = s.rotate.filter(|r| *r % 360.0 != 0.0);
        let transform = s.transform.as_deref().filter(|t| !t.is_empty());
        let own = (transform.is_some() || rotate.is_some()).then(|| box_transform(s, x, y, w, h));
        // Its outline, which its content adds lines to.
        let outlined = (node.debug || self.debug.all).then_some(self.debug.boxes.len());
        if outlined.is_some() {
            self.outline(&g, shift, extra, window, own);
        }
        if let Some(target) = &node.link {
            // Only the part of the box shown on this page.
            let (top, bottom) = match window {
                Some(win) => win.clip(y, y + h, self.dy),
                None => (y, y + h),
            };
            if bottom > top && w > 0.0 {
                let link = Op::Link {
                    rect: Rect::new(x, top, w, bottom - top),
                    target: target.clone(),
                };
                // Under the box's own transform, even when it is invisible
                // (opacity 0 still takes clicks, as in CSS).
                match own {
                    Some(m) => {
                        self.ops
                            .extend([Op::Save, Op::Transform(m), link, Op::Restore]);
                    }
                    None => self.ops.push(link),
                }
            }
        }

        let opacity = s.opacity.unwrap_or(1.0);
        if opacity <= 0.0 {
            return;
        }
        let clip = s.overflow == Some(Overflow::Hidden);
        let saved = opacity < 1.0 || own.is_some() || clip;
        let outer_opacity = self.opacity;
        let outer_ctm = self.ctm;
        if saved {
            self.ops.push(Op::Save);
        }
        if opacity < 1.0 {
            self.opacity *= opacity;
            self.ops.push(Op::SetOpacity(self.opacity));
        }
        if let Some(m) = own {
            self.ops.push(Op::Transform(m));
            self.ctm = Some(self.ctm.map_or(m, |c| mul(c, m)));
        }

        let border_box = Rect::new(x, y, w, h);
        let radii = radii(s, w, h);
        if let Some(bg) = s.background_color.filter(|c| c[3] > 0.0) {
            self.fill(rounded_rect(border_box, &radii), bg);
        }
        let padding_box = border_box.inset(g.border);
        let inner = inset_radii(&radii, g.border);
        if node.background_image.is_some() {
            self.content.background(node, padding_box, &inner, self.ops);
        }
        self.borders(s, g.border, border_box, &radii);

        if clip {
            self.ops.push(Op::ClipPath {
                path: rounded_rect(padding_box, &inner),
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
            line_shifts: self.mode.line_shifts(i),
            window,
            opacity: self.opacity,
            line_boxes: self.tree.wraps.get(&i).map(|b| &**b),
        };
        match outlined.and_then(|k| self.debug.boxes.get_mut(k)) {
            Some(b) => self
                .content
                .paint_outlined(node, &g, &ctx, self.ops, &mut b.lines),
            None => self.content.paint(node, &g, &ctx, self.ops),
        }
        let tree = self.tree;
        if has_z(tree, i) {
            for c in paint_order(tree, i) {
                self.node(c);
            }
        } else {
            for c in tree.children(i) {
                self.node(c);
            }
        }
        if let Some(t) = &table {
            self.repeat_header(i, &g, t);
        }

        if saved {
            self.ops.push(Op::Restore);
        }
        self.opacity = outer_opacity;
        self.ctm = outer_ctm;
    }

    /// Records the boxes of a node for its `debug` outline. `window` is the
    /// paged range shown on this page; `own` the node's own transform.
    fn outline(
        &mut self,
        g: &Geometry,
        shift: f32,
        extra: f32,
        window: Option<PageWindow>,
        own: Option<[f32; 6]>,
    ) {
        let y = g.y + shift + self.dy;
        let bottom = y + g.h + extra;
        let ctm = match (self.ctm, own) {
            (Some(c), Some(m)) => Some(mul(c, m)),
            (c, m) => c.or(m),
        };
        self.debug.boxes.push(DebugBox {
            rect: Rect::new(g.x + self.dx, y, g.w, g.h + extra),
            border: g.border,
            padding: g.padding,
            margin: g.margin,
            size: [g.w, g.h],
            shown: match window {
                Some(win) => win.clip(y, bottom, self.dy),
                None => (y, bottom),
            },
            ctm,
            lines: Vec::new(),
        });
    }

    /// Row and cell backgrounds and collapsed borders of the rows on this
    /// page (or of the header rows, when repeating them).
    fn table_body(&mut self, i: NodeIdx, g: &Geometry, t: &TableLayout) {
        let rows: Vec<NodeIdx> = self.tree.children(i).collect();
        let n = rows.len();
        let mode = self.mode;
        let header_pass = matches!(mode, Mode::Header { .. });
        let range = if header_pass { 0..t.header_rows } else { 0..n };
        // Rows on this page. Only `lo..hi` is scanned below, so painting a
        // long table costs O(rows on the page), not O(rows) per page.
        let in_page = |r: usize| range.contains(&r) && mode.shows(rows[r]);
        let lo = range.clone().find(|&r| in_page(r)).unwrap_or(n);
        let hi = range
            .clone()
            .rev()
            .find(|&r| in_page(r))
            .map_or(lo, |r| r + 1);
        let visible = |r: usize| r >= lo && r < hi && in_page(r);
        let shift = |r: usize| mode.shifts(rows[r]).0;
        let extra = |r: usize| mode.shifts(rows[r]).1;
        // Shift of the bottom of row `r`.
        let bshift = |r: usize| shift(r) + extra(r);
        let Rect { x: cx, y: cy, .. } = g.content_box();
        let (ox, oy) = (cx + self.dx, cy + self.dy);

        // Bands to paint: (rect, colour, style, horizontal). Backgrounds
        // and solid edges are filled, dashed and dotted edges stroked.
        let mut fills: Vec<(Rect, Color, BorderStyle, bool)> = Vec::new();
        let first_cell = t.cells.partition_point(|c| c.row < lo);
        if t.separate {
            let cells: Vec<usize> = (first_cell..t.cells.len())
                .take_while(|&k| t.cells[k].row < hi)
                .filter(|&k| visible(t.cells[k].row))
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
                    let y = oy + t.row_y[r] + shift(r);
                    let rect = Rect::new(ox, y, t.width, t.row_h[r] + extra(r));
                    fills.push((rect, bg, BorderStyle::Solid, true));
                }
            }
        }
        for cell in t.cells[first_cell..].iter().take_while(|c| c.row < hi) {
            if !visible(cell.row) {
                continue;
            }
            let style = self.tree.nodes[cell.idx as usize].style();
            if let Some(bg) = style.background_color.filter(|c| c[3] > 0.0) {
                let rect = Rect::new(
                    ox + cell.x,
                    oy + cell.y + shift(cell.row),
                    cell.w,
                    cell.h + extra(cell.row),
                );
                fills.push((rect, bg, BorderStyle::Solid, true));
            }
        }

        let repeating_header = t.header_rows > 0 && !header_pass;
        let first_h = t.h_edges.partition_point(|e| e.line < lo);
        for e in t.h_edges[first_h..].iter().take_while(|e| e.line <= hi) {
            let (x, w) = (ox + t.col_x[e.col], t.col_w[e.col]);
            // The edge's band, with its top at `y`.
            let band = |y: f32| (Rect::new(x, y, w, e.width), e.color, e.style, true);
            let l = e.line;
            if l == 0 {
                if visible(0) {
                    fills.push(band(oy + shift(0)));
                }
            } else if l == n {
                if visible(n - 1) {
                    let bottom = oy + t.row_y[n - 1] + t.row_h[n - 1];
                    fills.push(band(bottom - e.width + bshift(n - 1)));
                }
            } else if (bshift(l - 1) - shift(l)).abs() < SHIFT_EPS {
                if visible(l - 1) || visible(l) {
                    fills.push(band(oy + t.row_y[l] - e.width / 2.0 + shift(l)));
                }
            } else {
                // The line falls on a page break: close the fragment above,
                // and open the one below unless a repeated header does.
                if visible(l - 1) {
                    fills.push(band(oy + t.row_y[l] - e.width + bshift(l - 1)));
                }
                if visible(l) && !(repeating_header && l > t.header_rows) {
                    fills.push(band(oy + t.row_y[l] + shift(l)));
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
            let h: f32 = t.row_h[e.row..e.row + e.rows].iter().sum::<f32>()
                + (e.row..e.row + e.rows).map(extra).sum::<f32>();
            let y = oy + t.row_y[e.row] + shift(e.row);
            fills.push((Rect::new(ox + x, y, e.width, h), e.color, e.style, false));
        }
        for (rect, color, style, horizontal) in fills {
            if style == BorderStyle::Solid {
                self.fill(rounded_rect(rect, &NO_RADII), color);
            } else {
                self.dashed_edge(rect, color, style, horizontal);
            }
        }
    }

    /// A dashed or dotted collapsed edge: stroked along the middle of its
    /// band, clipped to the band so dots do not poke out of the table.
    fn dashed_edge(&mut self, band: Rect, color: Color, style: BorderStyle, horizontal: bool) {
        let Rect { x, y, w, h } = band;
        let (path, width) = if horizontal {
            (line(x, y + h / 2.0, x + w, y + h / 2.0), h)
        } else {
            (line(x + w / 2.0, y, x + w / 2.0, y + h), w)
        };
        self.ops.push(Op::Save);
        self.ops.push(Op::ClipPath {
            path: rounded_rect(band, &NO_RADII),
            rule: FillRule::NonZero,
        });
        self.stroke(path, color, width, style);
        self.ops.push(Op::Restore);
    }

    /// Separate borders: each visible cell paints the row background, its
    /// own background and its own borders, with its `borderRadius`.
    /// `cells`: the visible cells, as indices into `t.cells`.
    fn separate_cells(
        &mut self,
        t: &TableLayout,
        rows: &[NodeIdx],
        cells: &[usize],
        ox: f32,
        oy: f32,
    ) {
        for &k in cells {
            let cell = &t.cells[k];
            let (shift, extra) = self.mode.shifts(rows[cell.row]);
            let style = self.tree.nodes[cell.idx as usize].style();
            let rect = Rect::new(ox + cell.x, oy + cell.y + shift, cell.w, cell.h + extra);
            let radii = radii(style, rect.w, rect.h);
            let row_bg = match self.tree.nodes[rows[cell.row] as usize].part {
                Part::Row(row) => row.background_color,
                _ => None,
            };
            for bg in [row_bg, style.background_color].into_iter().flatten() {
                if bg[3] > 0.0 {
                    self.fill(rounded_rect(rect, &radii), bg);
                }
            }
            self.borders(style, cell.insets, rect, &radii);
        }
    }

    /// On continuation pages, repaints the header rows at the top of the
    /// page, above the first body row (which pagination pushed down).
    fn repeat_header(&mut self, i: NodeIdx, g: &Geometry, t: &TableLayout) {
        let Some((pl, page)) = self.mode.paged() else {
            return;
        };
        if t.header_rows == 0 {
            return;
        }
        let rows: Vec<NodeIdx> = self.tree.children(i).collect();
        // The table's own header is on this page: nothing to repeat.
        if pl.on_page(rows[0], page) {
            return;
        }
        let page_top = pl.page_top(page);
        // First body row on this page, if it starts below a reserved header.
        let Some(first) = (t.header_rows..rows.len()).find(|&r| pl.on_page(rows[r], page)) else {
            return;
        };
        let Rect { y: cy, .. } = g.content_box();
        let fr = rows[first];
        let first_top = cy + t.row_y[first] + pl.shift(fr);
        // A row split across pages continues below a repeated header.
        let continued = pl.first_page(fr) < page && pl.is_split(fr);
        if !continued && (first_top - (page_top + t.header_height())).abs() > PAGED_EPS {
            return;
        }
        let shift = page_top - (cy + t.row_y[0]);
        let outer = std::mem::replace(&mut self.mode, Mode::Header { shift });
        self.table_body(i, g, t);
        for &r in &rows[..t.header_rows] {
            self.node(r);
        }
        self.mode = outer;
    }

    fn fill(&mut self, path: Path, color: Color) {
        self.fill_with(path, color, FillRule::NonZero);
    }

    fn fill_with(&mut self, path: Path, color: Color, rule: FillRule) {
        with_alpha(self.ops, self.opacity, color[3], |ops| {
            ops.push(Op::FillPath { path, color, rule });
        });
    }

    /// Borders of `widths` around the border box `rect`, whose height is
    /// the painted (paged) one: it can exceed the laid-out height when the
    /// box is split across pages.
    fn borders(&mut self, s: &Style, widths: Edges<f32>, rect: Rect, radii: &Radii) {
        let Rect { x, y, w, h } = rect;
        // Per side, in the order of `colors` and `styles`.
        let side_w = widths.to_array();
        if side_w.iter().all(|w| *w <= 0.0) {
            return;
        }
        let colors: [Color; 4] = s.border_color.map_or([BLACK; 4], |c| c.get(BLACK));
        let [bt, br, bb, bl] = side_w;
        let inner_radii = inset_radii(radii, widths);
        let inner = rect.inset(widths);
        let Rect {
            x: ix,
            y: iy,
            w: iw,
            h: ih,
        } = inner;

        // The ring between the border box and the padding box.
        let mut ring = rounded_rect(rect, radii);
        ring.segs.extend(rounded_rect(inner, &inner_radii).segs);

        let active: Vec<usize> = (0..4).filter(|&k| side_w[k] > 0.0).collect();
        let same_color = active.windows(2).all(|p| colors[p[0]] == colors[p[1]]);
        let styles = s
            .border_style
            .map_or([BorderStyle::Solid; 4], BorderStyles::sides);
        let mixed = active.windows(2).any(|p| styles[p[0]] != styles[p[1]]);
        let style = styles[active[0]];

        let (r, b) = (x + w, y + h);
        let (ir, ib) = (ix + iw, iy + ih);
        // Mitred trapezoid per side.
        let sides = [
            [(x, y), (r, y), (ir, iy), (ix, iy)],
            [(r, y), (r, b), (ir, ib), (ir, iy)],
            [(r, b), (x, b), (ix, ib), (ir, ib)],
            [(x, b), (x, y), (ix, iy), (ix, ib)],
        ];
        // The line along the middle of each side, for dashes and dots.
        let mid = |k: usize| match k {
            0 => line(x, y + bt / 2.0, r, y + bt / 2.0),
            1 => line(r - br / 2.0, y, r - br / 2.0, b),
            2 => line(r, b - bb / 2.0, x, b - bb / 2.0),
            _ => line(x + bl / 2.0, b, x + bl / 2.0, y),
        };
        let square = crate::paint::shapes::is_square(radii);

        if mixed {
            self.ops.push(Op::Save);
            self.ops.push(Op::ClipPath {
                path: ring,
                rule: FillRule::EvenOdd,
            });
            for &k in &active {
                if styles[k] == BorderStyle::Solid {
                    self.fill(polygon(&sides[k]), colors[k]);
                } else {
                    self.stroke(mid(k), colors[k], side_w[k], styles[k]);
                }
            }
            self.ops.push(Op::Restore);
            return;
        }
        if style == BorderStyle::Solid && same_color && (active.len() == 4 || !square) {
            self.fill_with(ring, colors[active[0]], FillRule::EvenOdd);
            return;
        }
        if style == BorderStyle::Solid && square {
            // No ring needed: avoids hairline seams where the outer and
            // inner edges of a zero-width side coincide.
            for &k in &active {
                self.fill(polygon(&sides[k]), colors[k]);
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
                self.fill(polygon(&sides[k]), colors[k]);
            }
        } else {
            // Dashed / dotted: stroke along the middle of each side.
            let uniform =
                active.len() == 4 && side_w.windows(2).all(|p| p[0] == p[1]) && same_color;
            if uniform && !square {
                let half = bt / 2.0;
                let mid_radii = inset_radii(radii, Edges::all(half));
                let middle = Rect::new(x + half, y + half, w - bt, h - bt);
                let path = rounded_rect(middle, &mid_radii);
                self.stroke(path, colors[0], bt, style);
            } else {
                for &k in &active {
                    self.stroke(mid(k), colors[k], side_w[k], style);
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
        with_alpha(self.ops, self.opacity, color[3], |ops| {
            ops.push(Op::StrokePath {
                path,
                color,
                width,
                dash,
            });
        });
    }
}

fn z_index(tree: &Tree<'_>, j: NodeIdx) -> i32 {
    tree.nodes[j as usize]
        .node()
        .and_then(|n| n.style.z_index)
        .unwrap_or(0)
}

/// Whether a child of `i` sets a non-zero `zIndex`.
fn has_z(tree: &Tree<'_>, i: NodeIdx) -> bool {
    tree.holds_floats(i) || tree.children(i).any(|j| z_index(tree, j) != 0)
}

/// The children of `i` in paint order: by `zIndex`, floats after the
/// other children of their `zIndex` (so that a paragraph's background does
/// not cover the float it wraps around), then document order.
fn paint_order(tree: &Tree<'_>, i: NodeIdx) -> Vec<NodeIdx> {
    let mut c: Vec<NodeIdx> = tree.children(i).collect();
    let floats = |j: NodeIdx| {
        tree.nodes[j as usize]
            .node()
            .is_some_and(|n| n.style.floats().is_some())
    };
    c.sort_by_key(|&j| (z_index(tree, j), floats(j)));
    c
}

/// Corner radii of a box from its `borderRadius`.
fn radii(s: &Style, w: f32, h: f32) -> Radii {
    match s.border_radius {
        Some(BorderRadius::Uniform(r)) if r > 0.0 => uniform_radii(r, w, h),
        Some(BorderRadius::Corners(c)) => corner_radii(c.get(), w, h),
        _ => NO_RADII,
    }
}

/// Corner radii of a node's content box: its `borderRadius` inset by the
/// border and padding. Replaced content (images) is clipped to them, as in
/// CSS.
pub(crate) fn content_radii(s: &Style, g: &Geometry) -> Radii {
    let (b, p) = (g.border, g.padding);
    let inset = Edges {
        top: b.top + p.top,
        right: b.right + p.right,
        bottom: b.bottom + p.bottom,
        left: b.left + p.left,
    };
    inset_radii(&radii(s, g.w, g.h), inset)
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

/// Point `(x, y)` transformed by `m`.
pub(crate) fn apply(m: [f32; 6], (x, y): (f32, f32)) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn resolve(v: LengthPercent, of: f32) -> f32 {
    match v {
        LengthPercent::Points(p) => p,
        LengthPercent::Percent(p) => p * of,
    }
}

fn rotation(deg: f32) -> [f32; 6] {
    // y-down space: positive angles turn clockwise, as in CSS.
    let (sin, cos) = deg.to_radians().sin_cos();
    [cos, sin, -sin, cos, 0.0, 0.0]
}

/// The matrix of `rotate`, `transform` and `transformOrigin` for a border
/// box at `(x, y)` of size `w × h`, in page coordinates.
pub(crate) fn box_transform(s: &Style, x: f32, y: f32, w: f32, h: f32) -> [f32; 6] {
    let [ox, oy] = s
        .transform_origin
        .unwrap_or([LengthPercent::Percent(0.5); 2]);
    let (ox, oy) = (x + resolve(ox, w), y + resolve(oy, h));
    let mut m = [1.0, 0.0, 0.0, 1.0, ox, oy];
    if let Some(deg) = s.rotate {
        m = mul(m, rotation(deg));
    }
    for op in s.transform.iter().flat_map(|t| t.iter()) {
        let t = match *op {
            TransformOp::Translate([tx, ty]) => {
                [1.0, 0.0, 0.0, 1.0, resolve(tx, w), resolve(ty, h)]
            }
            TransformOp::Scale([sx, sy]) => [sx, 0.0, 0.0, sy, 0.0, 0.0],
            TransformOp::Rotate(deg) => rotation(deg),
            TransformOp::Skew([ax, ay]) => [
                1.0,
                ay.to_radians().tan(),
                ax.to_radians().tan(),
                1.0,
                0.0,
                0.0,
            ],
            TransformOp::Matrix(m) => m,
        };
        m = mul(m, t);
    }
    mul(m, [1.0, 0.0, 0.0, 1.0, -ox, -oy])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(json: &str) -> Style {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn scale_keeps_the_centre_fixed() {
        let m = box_transform(
            &style(r#"{"transform":[{"scale":[2,2]}]}"#),
            10.0,
            10.0,
            20.0,
            20.0,
        );
        assert_eq!(apply(m, (20.0, 20.0)), (20.0, 20.0));
        assert_eq!(apply(m, (10.0, 10.0)), (0.0, 0.0));
    }

    #[test]
    fn translate_percent_is_of_the_own_box() {
        let m = box_transform(
            &style(r#"{"transform":[{"translate":["50%",4]}]}"#),
            0.0,
            0.0,
            20.0,
            10.0,
        );
        assert_eq!(apply(m, (0.0, 0.0)), (10.0, 4.0));
    }

    #[test]
    fn functions_apply_right_to_left_like_css() {
        // translate then scale (CSS order): the scale happens first.
        let m = box_transform(
            &style(
                r#"{"transform":[{"translate":[10,0]},{"scale":[2,1]}],"transformOrigin":[0,0]}"#,
            ),
            0.0,
            0.0,
            10.0,
            10.0,
        );
        assert_eq!(apply(m, (1.0, 0.0)), (12.0, 0.0));
    }

    #[test]
    fn rotate_around_top_left_origin() {
        let m = box_transform(
            &style(r#"{"rotate":90,"transformOrigin":[0,0]}"#),
            0.0,
            0.0,
            10.0,
            10.0,
        );
        let (x, y) = apply(m, (10.0, 0.0));
        assert!(x.abs() < 1e-5 && (y - 10.0).abs() < 1e-5);
    }
}
