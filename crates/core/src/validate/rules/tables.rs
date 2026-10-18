//! The checks of tables: columns, rows, spans and borders.

use super::{Alpha, Checker, Ctx, Len, MAX_TABLE_SLOTS};
use crate::error::ValidationCode as Code;
use crate::schema::{ColumnWidth, Table};

impl Checker<'_> {
    pub(super) fn table(&mut self, t: &Table) {
        let ncols = t.columns.len();
        if ncols == 0 {
            self.err(
                Code::InvalidTable,
                Some("columns"),
                "a table needs at least one column",
            );
            return;
        }
        self.at("columns", |c| {
            for (i, col) in t.columns.iter().enumerate() {
                let width = match col.width {
                    ColumnWidth::Points(p) => Len::Points(p),
                    ColumnWidth::Fr(f) => Len::Fr(f),
                    ColumnWidth::Auto => continue,
                };
                c.at_index(i, |c| c.len(Some("width"), width, false));
            }
        });
        if t.header_rows as usize > t.rows.len() {
            self.err(
                Code::InvalidTable,
                Some("headerRows"),
                format!(
                    "headerRows ({}) exceeds the number of rows ({})",
                    t.header_rows,
                    t.rows.len()
                ),
            );
        }
        let nrows = t.rows.len();
        self.table_slots = self.table_slots.saturating_add(nrows.saturating_mul(ncols));
        if self.table_slots > MAX_TABLE_SLOTS {
            self.err(
                Code::OutOfRange,
                Some("rows"),
                format!("a document's tables have at most {MAX_TABLE_SLOTS} grid slots (rows times columns) in all"),
            );
            return;
        }
        // Occupancy: the row each column is covered until (exclusive) by a
        // rowSpan from above.
        let mut busy = vec![0usize; ncols];
        self.at("rows", |c| {
            for (ri, row) in t.rows.iter().enumerate() {
                c.at_index(ri, |c| {
                    if let Some(id) = &row.id {
                        c.id(Some("id"), id);
                    }
                    c.non_neg("minHeight", row.min_height);
                    c.debug(row.debug);
                    if let Some(bg) = &row.background_color {
                        c.color_as(Some("backgroundColor"), bg, Alpha::Fill);
                    }
                    let mut col = 0usize;
                    for (ci, cell) in row.cells.iter().enumerate() {
                        c.at("cells", |c| {
                            c.at_index(ci, |c| {
                                if let Some(id) = &cell.id {
                                    c.id(Some("id"), id);
                                }
                                while col < ncols && busy[col] > ri {
                                    col += 1;
                                }
                                // Saturating: on a 32-bit target a span near
                                // u32::MAX would wrap past the column count.
                                let span = cell.col_span.max(1) as usize;
                                let rows_to = ri.saturating_add(cell.row_span as usize);
                                if cell.col_span == 0 || cell.row_span == 0 {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        "colSpan and rowSpan must be at least 1",
                                    );
                                }
                                if col.saturating_add(span) > ncols
                                    || busy[col..col + span].iter().any(|&b| b > ri)
                                {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        format!("cell does not fit: row has {ncols} columns"),
                                    );
                                } else if rows_to > nrows {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "rowSpan extends past the last row",
                                    );
                                } else if ri < t.header_rows as usize
                                    && rows_to > t.header_rows as usize
                                {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "a header cell cannot span into body rows",
                                    );
                                } else {
                                    busy[col..col + span].fill(rows_to);
                                }
                                col = col.saturating_add(span);
                                c.debug(cell.debug);
                                c.at("style", |c| {
                                    c.style(&cell.style, false, true);
                                    c.table_only(&cell.style);
                                });
                                c.children(Some(&cell.style), &cell.children, Ctx::Cell);
                            });
                        });
                    }
                    while col < ncols && busy[col] > ri {
                        col += 1;
                    }
                    if col < ncols {
                        c.err(
                            Code::InvalidTable,
                            Some("cells"),
                            format!("row covers {col} of {ncols} columns"),
                        );
                    }
                });
            }
        });
    }
}
