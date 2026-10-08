//! `gridTemplateAreas`: rows of whitespace-separated area names, as in CSS
//! (`"head head" "side main"`). A token of dots is an unnamed cell.

use std::collections::HashMap;

use crate::error::ValidationCode as Code;

/// Most cells an area template may describe.
pub const MAX_AREA_CELLS: usize = 10_000;

/// A named area: 0-based track ranges, end exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridArea<'a> {
    pub name: &'a str,
    pub rows: (u16, u16),
    pub cols: (u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Areas<'a> {
    pub rows: u16,
    pub cols: u16,
    /// In order of first appearance.
    pub areas: Vec<GridArea<'a>>,
}

impl Areas<'_> {
    pub fn has(&self, name: &str) -> bool {
        self.areas.iter().any(|a| a.name == name)
    }
}

/// A CSS-like identifier: letters, digits, `_` and `-`, not starting with
/// a digit.
pub fn is_area_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

pub fn parse_areas(rows: &[String]) -> Result<Areas<'_>, (Code, String)> {
    let grid: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.split_whitespace().collect())
        .collect();
    let ncols = grid.first().map_or(0, Vec::len);
    if ncols == 0 {
        return Err((
            Code::InvalidValue,
            "every row needs at least one cell".into(),
        ));
    }
    if let Some(i) = grid.iter().position(|r| r.len() != ncols) {
        return Err((
            Code::InvalidValue,
            format!(
                "row {i} has {} cells, the first row has {ncols}",
                grid[i].len()
            ),
        ));
    }
    if grid.len().saturating_mul(ncols) > MAX_AREA_CELLS {
        return Err((Code::OutOfRange, format!("at most {MAX_AREA_CELLS} cells")));
    }
    // name -> (index in `order`, cell count)
    let mut seen: HashMap<&str, (usize, usize)> = HashMap::new();
    let mut order: Vec<GridArea<'_>> = Vec::new();
    for (r, row) in grid.iter().enumerate() {
        for (c, &tok) in row.iter().enumerate() {
            if tok.bytes().all(|b| b == b'.') {
                continue;
            }
            if !is_area_name(tok) {
                return Err((
                    Code::InvalidValue,
                    format!("`{tok}` is not a valid area name"),
                ));
            }
            let (r, c) = (r as u16, c as u16);
            let e = seen.entry(tok).or_insert_with(|| {
                order.push(GridArea {
                    name: tok,
                    rows: (r, r + 1),
                    cols: (c, c + 1),
                });
                (order.len() - 1, 0)
            });
            e.1 += 1;
            let a = &mut order[e.0];
            a.rows = (a.rows.0.min(r), a.rows.1.max(r + 1));
            a.cols = (a.cols.0.min(c), a.cols.1.max(c + 1));
        }
    }
    for a in &order {
        let cells = usize::from(a.rows.1 - a.rows.0) * usize::from(a.cols.1 - a.cols.0);
        if seen[a.name].1 != cells {
            return Err((
                Code::InvalidValue,
                format!("area `{}` is not a rectangle", a.name),
            ));
        }
    }
    Ok(Areas {
        rows: grid.len() as u16,
        cols: ncols as u16,
        areas: order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_rectangles() {
        let r = rows(&[
            "head head head",
            "side main main",
            "side main main",
            ". foot ...",
        ]);
        let a = parse_areas(&r).unwrap();
        assert_eq!((a.rows, a.cols), (4, 3));
        let names: Vec<_> = a.areas.iter().map(|a| (a.name, a.rows, a.cols)).collect();
        assert_eq!(
            names,
            vec![
                ("head", (0, 1), (0, 3)),
                ("side", (1, 3), (0, 1)),
                ("main", (1, 3), (1, 3)),
                ("foot", (3, 4), (1, 2)),
            ]
        );
    }

    #[test]
    fn rejects_bad_templates() {
        for (r, code) in [
            (&["a b", "a"][..], Code::InvalidValue),
            (&["a b a"][..], Code::InvalidValue),
            (&["a a", "a b"][..], Code::InvalidValue),
            (&["1x"][..], Code::InvalidValue),
            (&["a.b"][..], Code::InvalidValue),
            (&[""][..], Code::InvalidValue),
            (&[][..], Code::InvalidValue),
        ] {
            assert_eq!(parse_areas(&rows(r)).unwrap_err().0, code, "{r:?}");
        }
        let big = vec![". ".repeat(101); 100];
        assert_eq!(parse_areas(&big).unwrap_err().0, Code::OutOfRange);
    }
}
