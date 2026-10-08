//! Table layout (§6.5): our own algorithm, since a CSS grid can neither
//! split rows across pages nor repeat header rows.
//!
//! - Column widths: fixed points, `fr` weights over the remaining width, or
//!   `auto` (max-content of the column's single-span cells, shrunk towards
//!   min-content when space is short; never more than the table width).
//! - Row height: the tallest single-row cell at its column width, at least
//!   the row's `minHeight`; a row-spanning cell taller than its rows adds
//!   the difference to its last row.
//! - Collapsed borders: every grid edge takes the wider of the two cell
//!   borders that meet there (later cell wins ties). Each cell reserves the
//!   full width of outer edges and half the width of inner edges.

// Grid code walks parallel arrays by index; iterators would obscure it.
#![allow(clippy::needless_range_loop)]

use taffy::prelude::AvailableSpace;

use super::{LayoutCtx, NodeIdx, Part, Tree};
use crate::schema::{Color, ColumnWidth, NodeKind, Table};
use crate::InkoxideError;

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

/// A cell's box, relative to the table's content box.
#[derive(Debug, Clone)]
pub(crate) struct CellBox {
    pub idx: NodeIdx,
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Collapsed border space reserved inside the cell `[t, r, b, l]`.
    pub insets: [f32; 4],
}

/// A resolved border on a horizontal grid line, spanning one column.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HEdge {
    /// Grid line index: 0 = top of the table, `rows` = bottom.
    pub line: usize,
    pub col: usize,
    pub width: f32,
    pub color: Color,
}

/// A resolved border on a vertical grid line, spanning one cell's rows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VEdge {
    /// Grid line index: 0 = left of the table, `cols` = right.
    pub line: usize,
    pub row: usize,
    pub rows: usize,
    pub width: f32,
    pub color: Color,
}

#[derive(Debug, Clone)]
pub(crate) struct TableLayout {
    pub width: f32,
    pub height: f32,
    pub col_x: Vec<f32>,
    pub col_w: Vec<f32>,
    pub row_y: Vec<f32>,
    pub row_h: Vec<f32>,
    pub cells: Vec<CellBox>,
    pub h_edges: Vec<HEdge>,
    pub v_edges: Vec<VEdge>,
    pub header_rows: usize,
    /// Atomic row groups (rows tied together by row spans), in order.
    pub groups: Vec<std::ops::Range<usize>>,
}

impl TableLayout {
    pub fn header_height(&self) -> f32 {
        self.row_h[..self.header_rows.min(self.row_h.len())]
            .iter()
            .sum()
    }
}

/// Structure of a table independent of sizes: cell positions in the grid
/// and resolved collapsed borders.
struct Grid {
    cols: usize,
    rows: usize,
    /// (arena idx, row, col, row_span, col_span) in document order.
    cells: Vec<(NodeIdx, usize, usize, usize, usize)>,
    h_edges: Vec<HEdge>,
    v_edges: Vec<VEdge>,
    /// Edge widths by slot: `h_w[line * cols + col]`, `v_w[line * rows + row]`.
    h_w: Vec<f32>,
    v_w: Vec<f32>,
}

fn grid(tree: &Tree<'_>, idx: NodeIdx, table: &Table) -> Grid {
    let cols = table.columns.len();
    let rows = table.rows.len();
    let mut cells = Vec::new();
    // owner[r][c] = index into `cells` covering that slot.
    let mut owner: Vec<Vec<Option<usize>>> = vec![vec![None; cols]; rows];
    for (r, row_idx) in tree.children(idx).enumerate() {
        let mut c = 0usize;
        for cell_idx in tree.children(row_idx) {
            let Part::Cell(cell) = tree.nodes[cell_idx as usize].part else {
                continue;
            };
            while c < cols && owner[r][c].is_some() {
                c += 1;
            }
            if c >= cols {
                break;
            }
            let cs = (cell.col_span as usize).clamp(1, cols - c);
            let rs = (cell.row_span as usize).clamp(1, rows - r);
            let k = cells.len();
            for rr in r..r + rs {
                for cc in c..c + cs {
                    owner[rr][cc] = Some(k);
                }
            }
            cells.push((cell_idx, r, c, rs, cs));
            c += cs;
        }
    }

    let border = |k: usize, side: usize| -> (f32, Color) {
        let s = tree.nodes[cells[k].0 as usize].style();
        let w = s.border_width.map_or(0.0, |b| b.get(0.0)[side]);
        let c = s.border_color.map_or(BLACK, |b| b.get(BLACK)[side]);
        (w, c)
    };
    // The wider border wins; on a tie the later cell (document order) wins.
    let resolve = |a: Option<(usize, usize)>, b: Option<(usize, usize)>| -> Option<(f32, Color)> {
        let ca = a.map(|(k, s)| (k, border(k, s)));
        let cb = b.map(|(k, s)| (k, border(k, s)));
        let best = match (ca, cb) {
            (Some(x), Some(y)) => {
                if y.1 .0 > x.1 .0 || (y.1 .0 == x.1 .0 && y.0 > x.0) {
                    y
                } else {
                    x
                }
            }
            (Some(x), None) | (None, Some(x)) => x,
            (None, None) => return None,
        };
        (best.1 .0 > 0.0).then_some(best.1)
    };

    const TOP: usize = 0;
    const RIGHT: usize = 1;
    const BOTTOM: usize = 2;
    const LEFT: usize = 3;
    let mut h_edges = Vec::new();
    for line in 0..=rows {
        for c in 0..cols {
            let above = if line > 0 { owner[line - 1][c] } else { None };
            let below = if line < rows { owner[line][c] } else { None };
            if above.is_some() && above == below {
                continue; // inside a row span
            }
            if let Some((width, color)) =
                resolve(above.map(|k| (k, BOTTOM)), below.map(|k| (k, TOP)))
            {
                h_edges.push(HEdge {
                    line,
                    col: c,
                    width,
                    color,
                });
            }
        }
    }
    let mut v_edges = Vec::new();
    for line in 0..=cols {
        let mut r = 0;
        while r < rows {
            let left = if line > 0 { owner[r][line - 1] } else { None };
            let right = if line < cols { owner[r][line] } else { None };
            // One segment per owning cell (a row span covers several rows).
            let span_rows = [left, right]
                .iter()
                .flatten()
                .map(|&k| cells[k].1 + cells[k].3 - r)
                .min()
                .unwrap_or(1)
                .max(1);
            if left.is_none() || left != right {
                if let Some((width, color)) =
                    resolve(left.map(|k| (k, RIGHT)), right.map(|k| (k, LEFT)))
                {
                    v_edges.push(VEdge {
                        line,
                        row: r,
                        rows: span_rows,
                        width,
                        color,
                    });
                }
            }
            r += span_rows;
        }
    }
    let mut h_w = vec![0.0f32; (rows + 1) * cols];
    for e in &h_edges {
        h_w[e.line * cols + e.col] = e.width;
    }
    let mut v_w = vec![0.0f32; (cols + 1) * rows];
    for e in &v_edges {
        for r in e.row..e.row + e.rows {
            v_w[e.line * rows + r] = e.width;
        }
    }
    Grid {
        cols,
        rows,
        cells,
        h_edges,
        v_edges,
        h_w,
        v_w,
    }
}

/// Collapsed border space a cell reserves: full outer edges, half inner.
fn insets(g: &Grid, r: usize, c: usize, rs: usize, cs: usize) -> [f32; 4] {
    let h = |line: usize| -> f32 {
        let outer = line == 0 || line == g.rows;
        let w = g.h_w[line * g.cols + c..line * g.cols + c + cs]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        if outer {
            w
        } else {
            w / 2.0
        }
    };
    let v = |line: usize| -> f32 {
        let outer = line == 0 || line == g.cols;
        let w = g.v_w[line * g.rows + r..line * g.rows + r + rs]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        if outer {
            w
        } else {
            w / 2.0
        }
    };
    [h(r), v(c + cs), h(r + rs), v(c)]
}

pub(crate) fn compute(
    ctx: &mut LayoutCtx<'_>,
    tree: &Tree<'_>,
    idx: NodeIdx,
    space: AvailableSpace,
) -> Result<TableLayout, InkoxideError> {
    let Some(NodeKind::Table(table)) = tree.nodes[idx as usize].node.map(|n| &n.kind) else {
        return Err(InkoxideError::Internal(
            "table layout on a non-table node".into(),
        ));
    };
    let g = grid(tree, idx, table);
    let cell_insets: Vec<[f32; 4]> = g
        .cells
        .iter()
        .map(|&(_, r, c, rs, cs)| insets(&g, r, c, rs, cs))
        .collect();

    // --- Column widths ------------------------------------------------
    let ncols = g.cols;
    let mut col_w = vec![0.0f32; ncols];
    let available = match space {
        AvailableSpace::Definite(w) => Some(w),
        _ => None,
    };
    let needs_content = table.columns.iter().any(|c| {
        c.width == ColumnWidth::Auto
            || (available.is_none() && matches!(c.width, ColumnWidth::Fr(_)))
    });
    // (min-content, max-content) per column from single-span cells.
    let mut content = vec![(0.0f32, 0.0f32); ncols];
    if needs_content {
        for (k, &(cidx, _, c, _, cs)) in g.cells.iter().enumerate() {
            if cs != 1 {
                continue;
            }
            let wants = match table.columns[c].width {
                ColumnWidth::Auto => true,
                ColumnWidth::Fr(_) => available.is_none(),
                ColumnWidth::Points(_) => false,
            };
            if !wants {
                continue;
            }
            let min = ctx
                .cell_size(tree, cidx, AvailableSpace::MinContent, cell_insets[k])?
                .width;
            let max = ctx
                .cell_size(tree, cidx, AvailableSpace::MaxContent, cell_insets[k])?
                .width;
            content[c].0 = content[c].0.max(min);
            content[c].1 = content[c].1.max(max.max(min));
        }
    }
    let mut fixed = 0.0;
    let mut fr_total = 0.0;
    for (c, spec) in table.columns.iter().enumerate() {
        match spec.width {
            ColumnWidth::Points(w) => {
                col_w[c] = w;
                fixed += w;
            }
            ColumnWidth::Auto => {
                let cap = available.unwrap_or(f32::INFINITY);
                col_w[c] = content[c].1.min(cap);
                fixed += col_w[c];
            }
            ColumnWidth::Fr(f) => fr_total += f,
        }
    }
    match available {
        Some(avail) => {
            let mut remaining = avail - fixed;
            if remaining < 0.0 {
                // Shrink auto columns towards their min-content widths.
                let shrinkable: f32 = table
                    .columns
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.width == ColumnWidth::Auto)
                    .map(|(c, _)| col_w[c] - content[c].0)
                    .sum();
                if shrinkable > 0.0 {
                    let ratio = ((-remaining) / shrinkable).min(1.0);
                    for (c, s) in table.columns.iter().enumerate() {
                        if s.width == ColumnWidth::Auto {
                            col_w[c] -= (col_w[c] - content[c].0) * ratio;
                        }
                    }
                }
                remaining = avail - col_w.iter().sum::<f32>();
            }
            if fr_total > 0.0 {
                let share = remaining.max(0.0) / fr_total;
                for (c, s) in table.columns.iter().enumerate() {
                    if let ColumnWidth::Fr(f) = s.width {
                        col_w[c] = share * f;
                    }
                }
            }
        }
        None => {
            // Content sizing: fr columns take their content width.
            for (c, s) in table.columns.iter().enumerate() {
                if let ColumnWidth::Fr(_) = s.width {
                    col_w[c] = match space {
                        AvailableSpace::MinContent => content[c].0,
                        _ => content[c].1,
                    };
                }
            }
        }
    }
    let mut col_x = Vec::with_capacity(ncols);
    let mut x = 0.0;
    for w in &col_w {
        col_x.push(x);
        x += w;
    }
    let width = x;

    // --- Row heights ----------------------------------------------------
    let nrows = g.rows;
    let mut row_h: Vec<f32> = table
        .rows
        .iter()
        .map(|r| r.min_height.unwrap_or(0.0))
        .collect();
    let span_w = |c: usize, cs: usize| col_w[c..c + cs].iter().sum::<f32>();
    let mut cell_h = Vec::with_capacity(g.cells.len());
    for (k, &(cidx, r, c, rs, cs)) in g.cells.iter().enumerate() {
        let h = ctx
            .cell_size(
                tree,
                cidx,
                AvailableSpace::Definite(span_w(c, cs)),
                cell_insets[k],
            )?
            .height;
        if rs == 1 {
            row_h[r] = row_h[r].max(h);
        }
        cell_h.push(h);
    }
    for (k, &(_, r, _, rs, _)) in g.cells.iter().enumerate() {
        if rs > 1 {
            let have: f32 = row_h[r..r + rs].iter().sum();
            if cell_h[k] > have {
                row_h[r + rs - 1] += cell_h[k] - have;
            }
        }
    }
    let mut row_y = Vec::with_capacity(nrows);
    let mut y = 0.0;
    for h in &row_h {
        row_y.push(y);
        y += h;
    }
    let height = y;

    // --- Cells and row groups ---------------------------------------------
    let cells: Vec<CellBox> = g
        .cells
        .iter()
        .enumerate()
        .map(|(k, &(cidx, r, c, rs, cs))| CellBox {
            idx: cidx,
            row: r,
            col: c,
            row_span: rs,
            col_span: cs,
            x: col_x[c],
            y: row_y[r],
            w: span_w(c, cs),
            h: row_h[r..r + rs].iter().sum(),
            insets: cell_insets[k],
        })
        .collect();

    let mut reach: Vec<usize> = (0..nrows).map(|r| r + 1).collect();
    for c in &cells {
        reach[c.row] = reach[c.row].max(c.row + c.row_span);
    }
    let mut groups = Vec::new();
    let mut start = 0;
    while start < nrows {
        let mut end = reach[start];
        let mut r = start;
        while r < end {
            end = end.max(reach[r]);
            r += 1;
        }
        groups.push(start..end);
        start = end;
    }

    Ok(TableLayout {
        width,
        height,
        col_x,
        col_w,
        row_y,
        row_h,
        cells,
        h_edges: g.h_edges,
        v_edges: g.v_edges,
        header_rows: (table.header_rows as usize).min(nrows),
        groups,
    })
}
