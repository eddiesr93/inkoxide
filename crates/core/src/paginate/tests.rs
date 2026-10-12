//! Pagination invariants on synthetic trees.

use super::*;
use crate::layout::{LayoutEngine, NoContent, TaffyLayout};
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
    fn lines(&mut self, _: &Node, _: f32) -> Option<std::sync::Arc<LineLayout>> {
        None
    }
}

fn run(d: &Document) -> (Tree<'_>, Placement) {
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, 200.0, H, &mut NoContent)
        .unwrap();
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

fn page(y: f32) -> u32 {
    ((y + EPS) / H).floor() as u32
}

fn page_of_bottom(y: f32) -> u32 {
    ((y - EPS) / H).floor().max(0.0) as u32
}

/// Every atomic leaf no taller than a page sits on exactly one page, and
/// siblings keep document order.
fn check_invariants(t: &Tree<'_>, p: &Placement) {
    for i in 1..t.nodes.len() as u32 {
        let (top, bottom) = rect(t, p, i);
        let leaf = t.nodes[i as usize].children.is_empty();
        if leaf && bottom - top <= H + EPS {
            assert_eq!(
                page(top),
                page_of_bottom(bottom).max(page(top)),
                "node {i} crosses a page boundary: {top}..{bottom}"
            );
        }
    }
    for i in 0..t.nodes.len() as u32 {
        let kids: Vec<u32> = t.children(i).collect();
        for w in kids.windows(2) {
            assert!(
                rect(t, p, w[0]).0 <= rect(t, p, w[1]).0 + EPS,
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
    use crate::text::{RustybuzzShaper, TextCache};
    let mut store = crate::FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    store.register("Inter-Regular", bytes).unwrap();
    let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(20);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"box","style":{{"height":30}}}},
            {{"type":"text","style":{{"lineHeight":1.5,"padding":{{"top":4,"bottom":4}}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}},
            {{"type":"box","style":{{"height":20}}}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, 200.0, H, &mut cache)
        .unwrap();
    let p = paginate(&tree, H, &mut cache);

    let node = tree.nodes[2].node.unwrap();
    let g = tree.nodes[2].geo;
    let (_, cy, cw, _) = g.content_box();
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
            page(top),
            page_of_bottom(top + line.height),
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
    use crate::text::{RustybuzzShaper, TextCache};
    let mut store = crate::FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    store.register("Inter-Regular", bytes).unwrap();
    let para = "lorem ".repeat(words);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"box","style":{{"height":{before}}}}},
            {{"type":"text","style":{{"lineHeight":1.5{text_style}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, 200.0, H, &mut cache)
        .unwrap();
    let p = paginate(&tree, H, &mut cache);
    let node = tree.nodes[2].node.unwrap();
    let (_, cy, cw, _) = tree.nodes[2].geo.content_box();
    let layout = cache.layout(node, cw).unwrap();
    let shifts = &p.line_shifts[2];
    if shifts.is_empty() {
        let pg = page(rect(&tree, &p, 2).0);
        return vec![pg; layout.lines.len()];
    }
    layout
        .lines
        .iter()
        .zip(shifts)
        .map(|(l, s)| page(cy + l.top + s))
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
        let pg = page(top);
        assert_eq!(pg, page_of_bottom(top + h), "row crosses a page");
        if pg > 0 {
            assert!(
                top - pg as f32 * H >= 10.0 - EPS,
                "row {r} overlaps the repeated header at {top}"
            );
        }
    }
    check_invariants(&t, &p);
}

#[test]
fn horizontal_padding_narrows_measured_text() {
    // Regression: Taffy hands leaf measures the border-box width; text
    // must wrap at the content width, or it overflows its box.
    use crate::fonts::FontSet;
    use crate::text::{RustybuzzShaper, TextCache};
    let mut store = crate::FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    store.register("Inter-Regular", bytes).unwrap();
    let para = "lorem ".repeat(60);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":{H}}},"children":[
            {{"type":"text","style":{{"padding":{{"left":100}},"borderWidth":{{"right":4}}}},"runs":[{{"text":"{para}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, 200.0, H, &mut cache)
        .unwrap();
    let (_, _, cw, ch) = tree.nodes[1].geo.content_box();
    assert!((cw - 96.0).abs() < 0.01);
    let layout = cache.layout(tree.nodes[1].node.unwrap(), cw).unwrap();
    assert!(
        (ch - layout.height).abs() < 0.01,
        "box {ch} vs lines {}",
        layout.height
    );
}
