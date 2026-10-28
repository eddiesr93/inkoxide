//! Pagination invariants on synthetic trees.

use super::*;
use crate::geom::Rect;
use crate::layout::{compute, NoContent};
use crate::schema::Document;
use crate::InputFormat;

const H: f32 = 100.0;

fn doc(children: &str) -> Document {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":200,"height":{H}}},"children":[{children}]}}]}}"#
    );
    crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap()
}

struct NoLines;
impl LineSource for NoLines {
    fn lines(
        &mut self,
        _: &Node,
        _: &crate::layout::LineBoxes,
    ) -> Option<std::sync::Arc<LineLayout>> {
        None
    }
}

fn run(d: &Document) -> (Tree<'_>, Placement) {
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut NoContent).unwrap();
    let p = paginate(&tree, H, &mut NoLines);
    (tree, p)
}

fn boxes(heights: &[f32]) -> String {
    heights
        .iter()
        .map(|h| format!(r#"{{"type":"box","style":{{"height":{h}}}}}"#))
        .collect::<Vec<_>>()
        .join(",")
}

/// Paged (top, bottom) of node i.
fn rect(t: &Tree<'_>, p: &Placement, i: u32) -> (f32, f32) {
    let (top, h) = p.paged_rect(t, i);
    (top, top + h)
}

/// Every atomic leaf no taller than a page sits on exactly one page, and
/// siblings keep document order.
fn check_invariants(t: &Tree<'_>, p: &Placement) {
    for i in 1..t.nodes.len() as u32 {
        let (top, bottom) = rect(t, p, i);
        let leaf = t.nodes[i as usize].children.is_empty();
        if leaf && bottom - top <= H + PAGED_EPS {
            assert_eq!(
                p.page_of(top),
                p.page_of_bottom(bottom).max(p.page_of(top)),
                "node {i} crosses a page boundary: {top}..{bottom}"
            );
        }
    }
    for i in 0..t.nodes.len() as u32 {
        let kids: Vec<u32> = t.children(i).collect();
        for w in kids.windows(2) {
            assert!(
                rect(t, p, w[0]).0 <= rect(t, p, w[1]).0 + PAGED_EPS,
                "order broken at {w:?}"
            );
        }
    }
}

#[test]
fn content_that_fits_stays_on_one_page() {
    let d = doc(&boxes(&[30.0, 30.0, 30.0]));
    let (t, p) = run(&d);
    assert_eq!(p.pages, 1);
    assert!(p.shift_top.iter().all(|s| *s == 0.0));
    check_invariants(&t, &p);
}

#[test]
fn overflowing_box_moves_to_the_next_page() {
    let d = doc(&boxes(&[60.0, 60.0, 30.0]));
    let (t, p) = run(&d);
    assert_eq!(p.pages, 2);
    assert_eq!(rect(&t, &p, 2), (100.0, 160.0));
    assert_eq!(rect(&t, &p, 3), (160.0, 190.0));
    check_invariants(&t, &p);
}

#[test]
fn nested_boxes_split_between_children() {
    let d = doc(&format!(
        r#"{{"type":"box","style":{{"padding":{{"top":5,"bottom":5}},"backgroundColor":[1,0,0,1]}},"children":[{}]}}"#,
        boxes(&[40.0, 40.0, 40.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(p.pages, 2);
    // Outer box spans both pages; third child starts page 2.
    let outer = rect(&t, &p, 1);
    assert_eq!(outer.0, 0.0);
    assert!(outer.1 > H);
    assert_eq!(rect(&t, &p, 4).0, 100.0);
    assert_eq!(p.page_range[1], (0, 1));
    check_invariants(&t, &p);
}

#[test]
fn keep_together_moves_whole_box() {
    let d = doc(&format!(
        r#"{},{{"type":"box","break":{{"inside":"avoid"}},"children":[{}]}}"#,
        boxes(&[50.0]),
        boxes(&[30.0, 30.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2), (100.0, 160.0));
    check_invariants(&t, &p);
}

#[test]
fn row_boxes_are_unsplittable() {
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"flexDirection":"row"}},"children":[{}]}}"#,
        boxes(&[50.0]),
        boxes(&[60.0, 60.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
}

#[test]
fn forced_breaks_never_create_blank_pages() {
    let d = doc(
        r#"{"type":"box","style":{"height":10},"break":{"before":"page"}},
        {"type":"pageBreak"},
        {"type":"box","style":{"height":10},"break":{"after":"page"}},
        {"type":"box","style":{"height":10},"break":{"before":"page"}},
        {"type":"box","style":{"height":10}}"#,
    );
    let (t, p) = run(&d);
    // Box 1 stays on page 0 (already at the top); pageBreak → page 1;
    // box 3 ends with a break → box 4 on page 2 (its own break is a no-op).
    assert_eq!(rect(&t, &p, 1).0, 0.0);
    assert_eq!(rect(&t, &p, 3).0, 100.0);
    assert_eq!(rect(&t, &p, 4).0, 200.0);
    assert_eq!(rect(&t, &p, 5).0, 210.0);
    assert_eq!(p.pages, 3);
}

#[test]
fn min_presence_ahead_pushes_headings() {
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"height":10}},"break":{{"minPresenceAhead":40}}}}"#,
        boxes(&[70.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
}

#[test]
fn box_whose_first_child_does_not_fit_moves_whole() {
    // Without the head rule the wrapper's top padding would be left alone
    // at the bottom of page 1.
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"padding":{{"top":10}}}},"children":[{}]}}"#,
        boxes(&[80.0]),
        boxes(&[30.0, 30.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
    assert_eq!(rect(&t, &p, 3).0, 110.0);
}

#[test]
fn a_box_takes_its_first_child_s_presence_and_forced_break() {
    // A 70pt box, then a card with 10pt top padding whose first child asks
    // to start a page or for 40pt of room. The card moves with it rather
    // than leaving its empty top at the foot of page 0.
    for brk in [r#"{"minPresenceAhead":40}"#, r#"{"before":"page"}"#] {
        let d = doc(&format!(
            r#"{},{{"type":"box","style":{{"padding":{{"top":10}}}},"children":[
                {{"type":"box","style":{{"height":10}},"break":{brk}}},{}]}}"#,
            boxes(&[70.0]),
            boxes(&[50.0])
        ));
        let (t, p) = run(&d);
        assert_eq!(rect(&t, &p, 2).0, 100.0, "{brk}: the card moves");
        assert_eq!(rect(&t, &p, 3).0, 110.0, "{brk}: its child follows");
        assert_eq!(p.pages, 2, "{brk}");
        check_invariants(&t, &p);
    }
}

#[test]
fn a_page_break_first_in_a_box_moves_the_box() {
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"padding":{{"top":10}}}},"children":[{{"type":"pageBreak"}},{}]}}"#,
        boxes(&[20.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
    assert_eq!(rect(&t, &p, 4).0, 110.0);
    assert_eq!(p.pages, 2);
}

#[test]
fn a_first_row_taller_than_a_page_starts_where_the_table_is() {
    // The row splits inside its cell, so the table starts on page 0 when
    // the first piece of the cell fits there.
    for header in ["", r#"{"cells":[{"style":{"height":10}}]},"#] {
        let cell = [r#"{"type":"box","style":{"height":25}}"#; 6].join(",");
        let d = doc(&format!(
            r#"{},{{"type":"table","columns":[{{"width":"1fr"}}],"headerRows":{},"rows":[{header}
                {{"cells":[{{"children":[{cell}]}}]}}]}}"#,
            boxes(&[30.0]),
            u8::from(!header.is_empty()),
        ));
        let (t, p) = run(&d);
        assert_eq!(rect(&t, &p, 2).0, 30.0, "{header}: the table stays");
        let row = t.children(2).last().unwrap();
        let cell = t.children(row).next().unwrap();
        let first = t.children(cell).next().unwrap();
        assert_eq!(
            p.page_of(rect(&t, &p, first).0),
            0,
            "{header}: the row starts on page 0"
        );
        check_invariants(&t, &p);
    }
}

#[test]
fn oversized_atomic_content_is_sliced() {
    let d = doc(&boxes(&[250.0]));
    let (t, p) = run(&d);
    assert_eq!(p.pages, 3);
    assert_eq!(p.page_range[1], (0, 2));
    assert_eq!(rect(&t, &p, 1), (0.0, 250.0));
}

#[test]
fn absolute_children_travel_with_their_parent() {
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"height":40}},"children":[{{"type":"box","style":{{"position":"absolute","inset":{{"top":5}},"height":10,"width":10}}}}]}}"#,
        boxes(&[80.0])
    ));
    let (_, p) = run(&d);
    assert_eq!(p.shift_top[2], 20.0);
    assert_eq!(p.shift_top[3], 20.0);
}

#[test]
fn fixed_nodes_are_collected() {
    let d = doc(
        r#"{"type":"box","style":{"height":10},"break":{"fixed":true}},{"type":"box","style":{"height":250}}"#,
    );
    let (_, p) = run(&d);
    assert_eq!(p.fixed, vec![1]);
}

#[test]
fn many_pages_of_small_boxes() {
    let heights: Vec<f32> = (0..500).map(|i| 7.0 + (i % 5) as f32 * 3.0).collect();
    let d = doc(&boxes(&heights));
    let (t, p) = run(&d);
    check_invariants(&t, &p);
    let total: f32 = heights.iter().sum();
    assert!(p.pages as f32 >= total / H);
}

#[test]
fn text_splits_between_lines() {
    use crate::fonts::FontSet;
    use crate::text::{HarfRustShaper, TextCache};
    let store = crate::test_fixtures::inter();
    let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(20);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"box","style":{{"height":30}}}},
            {{"type":"text","style":{{"lineHeight":1.5,"padding":{{"top":4,"bottom":4}}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}},
            {{"type":"box","style":{{"height":20}}}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(HarfRustShaper::new(&fonts), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut cache).unwrap();
    let p = paginate(&tree, H, &mut cache);

    let node = tree.nodes[2].node().unwrap();
    let g = tree.nodes[2].geo;
    let Rect { y: cy, w: cw, .. } = g.content_box();
    let layout = cache.layout(node, cw).unwrap();
    let shifts = &p.line_shifts[2];
    assert_eq!(
        shifts.len(),
        layout.lines.len(),
        "every line is placed exactly once"
    );
    assert!(p.pages >= 3);
    let mut prev_top = f32::MIN;
    for (line, shift) in layout.lines.iter().zip(shifts) {
        let top = cy + line.top + shift;
        assert!(top > prev_top, "lines stay in order");
        prev_top = top;
        assert_eq!(
            p.page_of(top),
            p.page_of_bottom(top + line.height),
            "line crosses a page: {top}"
        );
    }
    // The node's paged extent covers its lines; the box after it follows.
    let (_, after_bottom) = rect(&tree, &p, 3);
    assert!(after_bottom > prev_top);
    check_invariants(&tree, &p);
}

/// Page of each line of a paragraph placed after a box `before` tall.
fn line_pages(before: f32, text_style: &str, words: usize) -> Vec<u32> {
    use crate::fonts::FontSet;
    use crate::text::{HarfRustShaper, TextCache};
    let store = crate::test_fixtures::inter();
    let para = "lorem ".repeat(words);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"box","style":{{"height":{before}}}}},
            {{"type":"text","style":{{"lineHeight":1.5{text_style}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(HarfRustShaper::new(&fonts), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut cache).unwrap();
    let p = paginate(&tree, H, &mut cache);
    let node = tree.nodes[2].node().unwrap();
    let Rect { y: cy, w: cw, .. } = tree.nodes[2].geo.content_box();
    let layout = cache.layout(node, cw).unwrap();
    let shifts = &p.line_shifts[2];
    if shifts.is_empty() {
        let pg = p.page_of(rect(&tree, &p, 2).0);
        return vec![pg; layout.lines.len()];
    }
    layout
        .lines
        .iter()
        .zip(shifts)
        .map(|(l, s)| p.page_of(cy + l.top + s))
        .collect()
}

fn count(pages: &[u32], pg: u32) -> usize {
    pages.iter().filter(|&&p| p == pg).count()
}

#[test]
fn widows_and_orphans_move_the_break() {
    // 15 pt lines; 70 pt box leaves room for two lines on page 0.
    // ~15 words of "lorem " make three lines at 200 pt.
    let base = line_pages(70.0, "", 15);
    assert_eq!(base.len(), 3, "fixture text is three lines");
    assert_eq!((count(&base, 0), count(&base, 1)), (2, 1));

    let w = line_pages(70.0, r#","widows":2"#, 15);
    assert_eq!((count(&w, 0), count(&w, 1)), (1, 2));

    // Three lines cannot keep two on each side: the paragraph moves.
    let ow = line_pages(70.0, r#","widows":2,"orphans":2"#, 15);
    assert_eq!((count(&ow, 0), count(&ow, 1)), (0, 3));

    // Orphans alone: one line fits at the bottom (85 pt box), so it moves.
    let o = line_pages(85.0, r#","orphans":2"#, 15);
    assert_eq!(count(&o, 0), 0);
    let o1 = line_pages(85.0, "", 15);
    assert_eq!(count(&o1, 0), 1);
}

#[test]
fn tables_split_between_rows_and_reserve_header_space() {
    let rows: Vec<String> = (0..40)
        .map(|_| r#"{"cells":[{"style":{"height":10}}]}"#.to_string())
        .collect();
    let d = doc(&format!(
        r#"{},{{"type":"table","columns":[{{"width":"1fr"}}],"headerRows":1,"rows":[{}]}}"#,
        boxes(&[20.0]),
        rows.join(",")
    ));
    let (t, p) = run(&d);
    let rows: Vec<u32> = t.children(2).collect();
    // The table starts on page 0 right after the box.
    assert_eq!(p.shift_top[rows[0] as usize], 0.0);
    // Every page after the first starts with 10pt reserved for the header.
    for &r in &rows[1..] {
        let (top, h) = p.paged_rect(&t, r);
        let pg = p.page_of(top);
        assert_eq!(pg, p.page_of_bottom(top + h), "row crosses a page");
        if pg > 0 {
            assert!(
                top - pg as f32 * H >= 10.0 - PAGED_EPS,
                "row {r} overlaps the repeated header at {top}"
            );
        }
    }
    check_invariants(&t, &p);
}

#[test]
fn header_rows_as_tall_as_a_page_are_not_repeated() {
    // 150 points of header rows on 100-point pages: the body rows follow
    // them page after page, with no room kept above them.
    let header = r#"{"cells":[{"style":{"height":50}}]}"#;
    let body: Vec<String> = (0..20)
        .map(|_| r#"{"cells":[{"style":{"height":10}}]}"#.to_string())
        .collect();
    let d = doc(&format!(
        r#"{{"type":"table","columns":[{{"width":"1fr"}}],"headerRows":3,"rows":[{header},{header},{header},{}]}}"#,
        body.join(",")
    ));
    let (t, p) = run(&d);
    let rows: Vec<u32> = t.children(1).collect();
    for (k, &r) in rows[3..].iter().enumerate() {
        assert_eq!(rect(&t, &p, r).0, 150.0 + 10.0 * k as f32, "row {r}");
    }
    assert_eq!(p.pages, 4);
    check_invariants(&t, &p);
}

#[test]
fn horizontal_padding_narrows_measured_text() {
    // Regression: Taffy hands leaf measures the border-box width; text
    // must wrap at the content width, or it overflows its box.
    use crate::fonts::FontSet;
    use crate::text::{HarfRustShaper, TextCache};
    let store = crate::test_fixtures::inter();
    let para = "lorem ".repeat(60);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"text","style":{{"padding":{{"left":100}},"borderWidth":{{"right":4}}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(HarfRustShaper::new(&fonts), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut cache).unwrap();
    let Rect { w: cw, h: ch, .. } = tree.nodes[1].geo.content_box();
    assert!((cw - 96.0).abs() < 0.01);
    let layout = cache.layout(tree.nodes[1].node().unwrap(), cw).unwrap();
    assert!(
        (ch - layout.height).abs() < 0.01,
        "box {ch} vs lines {}",
        layout.height
    );
}

#[test]
fn forced_breaks_inside_a_box_that_fits_start_a_page() {
    for inner in [
        r#"{"type":"box","style":{"height":10}},{"type":"pageBreak"},{"type":"box","style":{"height":10}}"#,
        r#"{"type":"box","style":{"height":10},"break":{"after":"page"}},{"type":"box","style":{"height":10}}"#,
        r#"{"type":"box","style":{"height":10}},{"type":"box","style":{"height":10},"break":{"before":"page"}}"#,
        r#"{"type":"box","style":{"height":10}},{"type":"box","children":[{"type":"pageBreak"}]},{"type":"box","style":{"height":10}}"#,
    ] {
        // A forced break wins over `inside: avoid`, as in CSS.
        for brk in ["", r#","break":{"inside":"avoid"}"#] {
            let d = doc(&format!(r#"{{"type":"box"{brk},"children":[{inner}]}}"#));
            let (t, p) = run(&d);
            assert_eq!(p.pages, 2, "{brk} {inner}");
            let last = t.children(1).last().unwrap();
            assert_eq!(rect(&t, &p, last).0, 100.0, "{brk} {inner}");
            check_invariants(&t, &p);
        }
    }
}

#[test]
fn keep_together_box_with_a_forced_break_keeps_its_first_part_together() {
    // 70 pt on page 0 leaves 30: the 40 pt before the break moves whole.
    let d = doc(&format!(
        r#"{},{{"type":"box","break":{{"inside":"avoid"}},"children":[{},{{"type":"pageBreak"}},{}]}}"#,
        boxes(&[70.0]),
        boxes(&[20.0, 20.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    let kids: Vec<u32> = t.children(2).collect();
    assert_eq!(rect(&t, &p, 2).0, 100.0);
    assert_eq!(rect(&t, &p, kids[1]).0, 120.0);
    assert_eq!(rect(&t, &p, kids[3]).0, 200.0);
    assert_eq!(p.pages, 3);
}

#[test]
fn a_break_before_the_first_child_moves_its_box() {
    // The box's top padding goes with its first child, not alone on page 0.
    for first in [
        r#"{"type":"box","style":{"height":10},"break":{"before":"page"}}"#,
        r#"{"type":"pageBreak"},{"type":"box","style":{"height":10}}"#,
        r#"{"type":"box","children":[{"type":"box","style":{"height":10},"break":{"before":"page"}}]}"#,
    ] {
        let d = doc(&format!(
            r#"{},{{"type":"box","style":{{"padding":{{"top":5}}}},"children":[{first},{}]}}"#,
            boxes(&[10.0]),
            boxes(&[10.0])
        ));
        let (t, p) = run(&d);
        assert_eq!(rect(&t, &p, 2).0, 100.0, "{first}");
        assert_eq!(p.page_range[2], (1, 1), "{first}");
        assert_eq!(p.pages, 2, "{first}");
    }
    // After a pending break, too: one new page, not two.
    let d = doc(&format!(
        r#"{{"type":"box","style":{{"height":10}},"break":{{"after":"page"}}}},{{"type":"box","style":{{"padding":{{"top":5}}}},"children":[{{"type":"pageBreak"}},{}]}}"#,
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
    assert_eq!(p.pages, 2);
}

#[test]
fn a_page_break_marks_the_top_of_the_next_page() {
    // A link to a `pageBreak`'s id goes to the page it starts.
    let d = doc(&format!(
        r#"{},{{"type":"pageBreak"}},{}"#,
        boxes(&[10.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 100.0);
    assert_eq!(p.first_page(2), 1);
}

#[test]
fn a_trailing_forced_break_adds_no_page() {
    for children in [
        format!(r#"{},{{"type":"pageBreak"}}"#, boxes(&[10.0])),
        r#"{"type":"box","style":{"height":10},"break":{"after":"page"}}"#.to_owned(),
        format!(
            r#"{{"type":"box","children":[{},{{"type":"pageBreak"}}]}}"#,
            boxes(&[10.0])
        ),
        format!(
            r#"{},{{"type":"box","break":{{"inside":"avoid"}},"children":[{}]}},{{"type":"pageBreak"}},{{"type":"box","style":{{"position":"absolute","height":5,"width":5}}}}"#,
            boxes(&[10.0]),
            boxes(&[10.0])
        ),
    ] {
        let d = doc(&children);
        let (_, p) = run(&d);
        assert_eq!(p.pages, 1, "{children}");
    }
}

#[test]
fn absolute_page_children_stay_at_the_flow_top() {
    let abs = r#"{"type":"box","style":{"position":"absolute","inset":{"top":5},"height":10,"width":10}}"#;
    let d = doc(&format!(
        r#"{},{{"type":"pageBreak"}},{},{abs}"#,
        boxes(&[10.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 4), (5.0, 15.0));
    assert_eq!(p.page_range[4], (0, 0));
}

#[test]
fn fixed_nodes_are_collected_in_paint_order() {
    let fixed = |z: i32| {
        format!(
            r#"{{"type":"box","break":{{"fixed":true}},"style":{{"position":"absolute","zIndex":{z},"height":10,"width":10}}}}"#
        )
    };
    // Document order, depth first: the nested one comes first.
    let d = doc(&format!(
        r#"{{"type":"box","children":[{{"type":"box","children":[{}]}}]}},{{"type":"box","children":[{}]}}"#,
        fixed(0),
        fixed(0)
    ));
    let (t, p) = run(&d);
    let deep = t.children(t.children(1).next().unwrap()).next().unwrap();
    let shallow = t.children(2).next().unwrap();
    assert!(shallow < deep, "arena order is breadth first");
    assert_eq!(p.fixed, vec![deep, shallow]);
    // Then by `zIndex` among siblings, as the flow paints them.
    let d = doc(&format!(
        r#"{{"type":"box","children":[{},{}]}}"#,
        fixed(1),
        fixed(0)
    ));
    let (_, p) = run(&d);
    assert_eq!(p.fixed, vec![3, 2]);
}

#[test]
fn keep_together_box_taller_than_a_page_splits_between_children() {
    let d = doc(&format!(
        r#"{{"type":"box","break":{{"inside":"avoid"}},"children":[{}]}}"#,
        boxes(&[40.0, 40.0, 40.0, 40.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(p.pages, 2);
    check_invariants(&t, &p);
}

#[test]
fn row_and_grid_boxes_taller_than_a_page_split_by_rows() {
    let column = |hs: &[f32]| format!(r#"{{"type":"box","children":[{}]}}"#, boxes(hs));
    for style in [
        r#""flexDirection":"row""#,
        r#""flexDirection":"row-reverse""#,
        r#""flexDirection":"row","flexWrap":"wrap""#,
        r#""display":"grid","gridTemplateColumns":["1fr","1fr"]"#,
    ] {
        let d = doc(&format!(
            r#"{},{{"type":"box","style":{{{style}}},"children":[{},{}]}},{}"#,
            boxes(&[30.0]),
            column(&[40.0, 40.0, 40.0]),
            column(&[30.0, 30.0, 30.0, 30.0, 30.0]),
            boxes(&[10.0])
        ));
        let (t, p) = run(&d);
        // The row starts on page 0, below the first box.
        assert_eq!(rect(&t, &p, 2).0, 30.0, "{style}");
        check_invariants(&t, &p);
        // The row ends below its longer column, and the next box follows it.
        let (_, row_bottom) = rect(&t, &p, 2);
        let tall = t.children(2).find(|&c| t.children(c).count() == 5).unwrap();
        let last = t.children(tall).last().unwrap();
        assert!(row_bottom >= rect(&t, &p, last).1 - PAGED_EPS, "{style}");
        assert!(rect(&t, &p, 3).0 >= row_bottom - PAGED_EPS, "{style}");
        // Both columns are stretched to the row and end with it.
        for c in t.children(2) {
            assert_eq!(rect(&t, &p, c).1, row_bottom, "{style}");
        }
    }
    // A grid's second row starts below the first row's longer cell.
    let d = doc(&format!(
        r#"{{"type":"box","style":{{"display":"grid","gridTemplateColumns":["1fr","1fr"]}},"children":[{},{},{},{}]}}"#,
        column(&[40.0, 40.0, 40.0]),
        column(&[10.0]),
        boxes(&[10.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    check_invariants(&t, &p);
    let kids: Vec<u32> = t.children(1).collect();
    let first_row_bottom = rect(&t, &p, kids[0]).1;
    assert!(first_row_bottom > H);
    assert_eq!(rect(&t, &p, kids[2]).0, first_row_bottom);
    assert_eq!(rect(&t, &p, kids[3]).0, first_row_bottom);
}

/// Lays out and paginates `children` with text, checks that every line of
/// every paragraph sits on one page, and returns the page count.
fn check_lines(children: &str) -> u32 {
    use crate::fonts::FontSet;
    use crate::text::{HarfRustShaper, TextCache};
    let store = crate::test_fixtures::inter();
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[{children}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(HarfRustShaper::new(&fonts), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut cache).unwrap();
    let p = paginate(&tree, H, &mut cache);
    for i in 0..tree.nodes.len() as NodeIdx {
        let Some(node) = tree.nodes[i as usize].node() else {
            continue;
        };
        if !matches!(node.kind, NodeKind::Text { .. }) {
            continue;
        }
        let Rect { y: cy, w: cw, .. } = tree.nodes[i as usize].geo.content_box();
        let layout = cache.layout(node, cw).unwrap();
        let shifts = p.line_shifts(i);
        for (k, line) in layout.lines.iter().enumerate() {
            let shift = shifts.get(k).copied().unwrap_or(p.shift(i));
            let top = cy + line.top + shift;
            assert_eq!(
                p.page_of(top),
                p.page_of_bottom(top + line.height),
                "line {k} of node {i} crosses a page: {top}"
            );
        }
    }
    p.pages
}

#[test]
fn text_in_a_row_taller_than_a_page_splits_between_lines() {
    let text = |words: usize, brk: &str| {
        format!(
            r#"{{"type":"text"{brk},"style":{{"flexGrow":1,"flexBasis":0}},"runs":[{{"text":"{}","font":"Inter-Regular","size":10}}]}}"#,
            "lorem ".repeat(words)
        )
    };
    let pages = check_lines(&format!(
        r#"{{"type":"box","style":{{"height":30}}}},{{"type":"box","style":{{"flexDirection":"row"}},"children":[{},{}]}}"#,
        text(120, ""),
        text(60, "")
    ));
    assert!(pages >= 3);
    // Taller than a page, a paragraph with `inside: avoid` splits too.
    let pages = check_lines(&format!(
        r#"{{"type":"box","style":{{"height":30}}}},{}"#,
        text(120, r#","break":{"inside":"avoid"}"#)
    ));
    assert!(pages >= 3);
}

#[test]
fn a_box_holding_only_a_trailing_page_break_adds_no_page() {
    // Nothing follows the break inside the box or after it: the box stays
    // where it is, and its break adds no page.
    for children in [
        format!(
            r#"{},{{"type":"box","children":[{{"type":"pageBreak"}}]}}"#,
            boxes(&[10.0])
        ),
        format!(
            r#"{{"type":"box","children":[{},{{"type":"box","children":[{{"type":"pageBreak"}}]}}]}}"#,
            boxes(&[10.0])
        ),
        format!(
            r#"{},{{"type":"box","style":{{"padding":{{"top":5,"bottom":5}}}},"children":[{{"type":"pageBreak"}},{{"type":"pageBreak"}}]}}"#,
            boxes(&[10.0])
        ),
    ] {
        let d = doc(&children);
        let (t, p) = run(&d);
        assert_eq!(p.pages, 1, "{children}");
        check_invariants(&t, &p);
    }
    // Between content, such a box keeps its place and the content after it
    // starts the next page.
    let d = doc(&format!(
        r#"{},{{"type":"box","children":[{{"type":"pageBreak"}}]}},{},{{"type":"box","children":[{{"type":"pageBreak"}}]}}"#,
        boxes(&[10.0]),
        boxes(&[10.0])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, 10.0);
    assert_eq!(rect(&t, &p, 3).0, 100.0);
    assert_eq!(p.pages, 2);
}

#[test]
fn a_trailing_page_break_at_the_page_bottom_adds_no_page() {
    let d = doc(&format!(r#"{},{{"type":"pageBreak"}}"#, boxes(&[H])));
    let (_, p) = run(&d);
    assert_eq!(p.pages, 1);
    // A link to it goes to the foot of the last page.
    assert_eq!(p.page_range[2], (0, 0));
    let d = doc(&format!(
        r#"{{"type":"box","children":[{},{{"type":"pageBreak"}}]}}"#,
        boxes(&[H])
    ));
    let (_, p) = run(&d);
    assert_eq!(p.pages, 1);
}

#[test]
fn a_break_with_nothing_above_it_on_its_page_adds_no_page() {
    // At the start of the flow, below a top margin.
    for children in [
        format!(
            r#"{{"type":"box","style":{{"margin":{{"top":10}}}},"children":[{{"type":"pageBreak"}},{}]}}"#,
            boxes(&[10.0])
        ),
        r#"{"type":"box","style":{"height":10,"margin":{"top":10}},"break":{"before":"page"}}"#
            .to_owned(),
        format!(
            r#"{{"type":"box","style":{{"margin":{{"top":10}}}},"children":[{{"type":"box","style":{{"margin":{{"top":10}}}},"children":[{{"type":"pageBreak"}},{}]}}]}}"#,
            boxes(&[10.0])
        ),
    ] {
        let d = doc(&children);
        let (t, p) = run(&d);
        assert_eq!(p.pages, 1, "{children}");
        assert_eq!(p.page_range[1], (0, 0), "{children}");
        check_invariants(&t, &p);
    }
    // On a page whose content above it ended on the page before.
    let d = doc(&format!(
        r#"{},{{"type":"box","style":{{"height":10,"margin":{{"top":10}}}},"break":{{"before":"page"}}}}"#,
        boxes(&[H])
    ));
    let (t, p) = run(&d);
    assert_eq!(rect(&t, &p, 2).0, H + 10.0);
    assert_eq!(p.pages, 2);
}

/// Lays out and paginates `children` (200 wide, pages `H` tall) with
/// Inter, and returns the tree, the placement and the line pages of each
/// text node.
fn run_text<R>(
    children: &str,
    f: impl FnOnce(&Tree<'_>, &Placement, &mut dyn LineSource) -> R,
) -> R {
    use crate::fonts::FontSet;
    use crate::text::{HarfRustShaper, TextCache};
    let store = crate::test_fixtures::inter();
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[{children}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(HarfRustShaper::new(&fonts), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 200.0, H, &mut cache).unwrap();
    let p = paginate(&tree, H, &mut cache);
    f(&tree, &p, &mut cache)
}

const FLOAT_WORDS: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
    eiusmod tempor incididunt ut labore et dolore magna aliqua, ut enim ad minim veniam.";

#[test]
fn a_float_moves_to_the_next_page_with_the_lines_beside_it() {
    let children = format!(
        r#"{{"type":"box","style":{{"height":78}}}},
           {{"type":"box","children":[
             {{"type":"box","style":{{"float":"left","width":60,"height":20}}}},
             {{"type":"text","runs":[{{"text":"{FLOAT_WORDS}","font":"Inter-Regular","size":10}}]}}]}}"#
    );
    run_text(&children, |t, p, lines| {
        // Node 2 is the container, 3 the float, 4 the text. The float fits
        // on the first page, the lines beside it do not: they move
        // together.
        let (ftop, fbottom) = rect(t, p, 3);
        assert_eq!(p.page_of(ftop), 1, "{ftop}");
        assert_eq!(p.page_of_bottom(fbottom), 1);
        let node = t.nodes[4].node().unwrap();
        let layout = lines.lines(node, &t.line_boxes(4)).unwrap();
        let cy = t.nodes[4].geo.content_box().y;
        for (k, line) in layout.lines.iter().enumerate() {
            let top = cy + line.top + p.line_shift(4, k);
            if line.top < 20.0 {
                // Beside the float, on its page, next to it.
                assert_eq!(p.page_of(top), 1, "line {k}");
                assert!(top >= ftop - PAGED_EPS && top < fbottom, "line {k}: {top}");
            }
        }
        check_invariants(t, p);
    });
}

#[test]
fn a_float_taller_than_a_page_is_sliced() {
    let children = format!(
        r#"{{"type":"box","style":{{"height":30}}}},
           {{"type":"box","children":[
             {{"type":"box","style":{{"float":"right","width":50,"height":150}}}},
             {{"type":"text","runs":[{{"text":"{FLOAT_WORDS}","font":"Inter-Regular","size":10}}]}}]}}"#
    );
    run_text(&children, |t, p, _| {
        // Moved whole to a new page, then sliced across two.
        let (ftop, fbottom) = rect(t, p, 3);
        assert_eq!(ftop, 100.0);
        assert_eq!(fbottom - ftop, 150.0);
        assert!(p.pages >= 3);
    });
}

#[test]
fn a_nested_table_continues_below_the_repeated_outer_header() {
    // An outer row taller than a page holds an inner table, with or without
    // its own header rows: its rows on later pages start below the outer
    // header (20pt), and below the inner one (10pt) when it has one.
    let row = r#"{"cells":[{"style":{"height":10}}]}"#;
    for inner_header in [0u8, 1] {
        let inner_rows = vec![row; 30].join(",");
        let d = doc(&format!(
            r#"{{"type":"table","columns":[{{"width":"1fr"}}],"headerRows":1,"rows":[
                {{"cells":[{{"style":{{"height":20}}}}]}},
                {{"cells":[{{"children":[{{"type":"table","columns":[{{"width":"1fr"}}],
                    "headerRows":{inner_header},"rows":[{inner_rows}]}}]}}]}}]}}"#
        ));
        let (t, p) = run(&d);
        let outer_row = t.children(1).nth(1).unwrap();
        let cell = t.children(outer_row).next().unwrap();
        let inner = t.children(cell).next().unwrap();
        let rows: Vec<u32> = t.children(inner).collect();
        assert!(p.pages > 1);
        let reserved = 20.0 + 10.0 * f32::from(inner_header);
        for &r in &rows[usize::from(inner_header)..] {
            let (top, bottom) = rect(&t, &p, r);
            let pg = p.page_of(top);
            assert_eq!(pg, p.page_of_bottom(bottom), "row {r} crosses a page");
            if pg > 0 {
                assert!(
                    top - pg as f32 * H >= reserved - PAGED_EPS,
                    "inner header {inner_header}: row {r} at {top} is under the repeated headers"
                );
            }
        }
        check_invariants(&t, &p);
    }
}
