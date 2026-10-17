use super::*;
use crate::fonts::FontSet;
use crate::schema::Document;
use crate::text::{RustybuzzShaper, TextCache};
use crate::InputFormat;

fn doc(children: &str) -> Document {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":600,"height":800}},"children":[{children}]}}]}}"#
    );
    crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap()
}

fn layout(d: &Document, width: f32) -> Vec<(f32, f32, f32, f32)> {
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, width, 0.0, &mut NoContent).unwrap();
    tree.nodes
        .iter()
        .map(|n| (n.geo.x, n.geo.y, n.geo.w, n.geo.h))
        .collect()
}

#[test]
fn arena_is_breadth_first_with_contiguous_children() {
    let d = doc(
        r#"{"type":"box","children":[{"type":"box"},{"type":"box","children":[{"type":"spacer"}]}]},{"type":"spacer"}"#,
    );
    let tree = Tree::build(&d.pages[0].children);
    assert_eq!(tree.nodes.len(), 6);
    assert_eq!(tree.nodes[0].children, 1..3);
    assert_eq!(tree.nodes[1].children, 3..5);
    assert_eq!(tree.nodes[4].children, 5..6);
    assert_eq!(tree.nodes[5].parent, 4);
}

#[test]
fn column_flow_stacks_vertically_and_fills_width() {
    let d = doc(
        r#"{"type":"box","style":{"height":10}},{"type":"box","style":{"height":20,"margin":{"top":5}}}"#,
    );
    let g = layout(&d, 500.0);
    assert_eq!(g[0], (0.0, 0.0, 500.0, 35.0));
    assert_eq!(g[1], (0.0, 0.0, 500.0, 10.0));
    assert_eq!(g[2], (0.0, 15.0, 500.0, 20.0));
}

#[test]
fn row_with_grow_percent_and_gap() {
    let d = doc(
        r#"{"type":"box","style":{"flexDirection":"row","columnGap":10,"padding":{"left":5,"right":5}},"children":[
            {"type":"box","style":{"width":"50%","height":10}},
            {"type":"box","style":{"flexGrow":1,"height":10}}]}"#,
    );
    let g = layout(&d, 210.0);
    // Content box is 200 wide: 50% = 100, gap 10, rest 90.
    assert_eq!(g[2], (5.0, 0.0, 100.0, 10.0));
    assert_eq!(g[3], (115.0, 0.0, 90.0, 10.0));
}

#[test]
fn borders_take_space_and_positions_are_absolute() {
    let d = doc(
        r#"{"type":"box","style":{"margin":{"top":7},"borderWidth":{"top":3,"left":4},"padding":{"top":2,"left":1}},
            "children":[{"type":"box","style":{"height":10}}]}"#,
    );
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 100.0, 0.0, &mut NoContent).unwrap();
    let outer = tree.nodes[1].geo;
    assert_eq!((outer.y, outer.h), (7.0, 15.0));
    assert_eq!(outer.border, Edges::from([3.0, 0.0, 0.0, 4.0]));
    assert_eq!(outer.content_box(), Rect::new(5.0, 12.0, 95.0, 10.0));
    let inner = tree.nodes[2].geo;
    assert_eq!((inner.x, inner.y, inner.w), (5.0, 12.0, 95.0));
}

#[test]
fn display_none_takes_no_space() {
    let d = doc(
        r#"{"type":"box","style":{"display":"none","height":50}},{"type":"box","style":{"height":10}}"#,
    );
    let g = layout(&d, 100.0);
    assert_eq!(g[2].1, 0.0);
    assert_eq!(g[0].3, 10.0);
}

#[test]
fn absolute_children_are_positioned_from_the_parent() {
    let d = doc(r#"{"type":"box","style":{"height":100},"children":[
            {"type":"box","style":{"position":"absolute","inset":{"right":10,"bottom":20},"width":30,"height":30}}]}"#);
    let g = layout(&d, 200.0);
    assert_eq!(g[2], (160.0, 50.0, 30.0, 30.0));
}

#[test]
fn grid_tracks_and_spans() {
    let d = doc(
        r#"{"type":"box","style":{"display":"grid","gridTemplateColumns":[50,"1fr","1fr"],"gridTemplateRows":[20,20],"gap":10},"children":[
            {"type":"box","style":{"gridRow":{"start":1,"span":2}}},
            {"type":"box"},{"type":"box"},
            {"type":"box","style":{"gridColumn":{"start":2,"end":4}}}]}"#,
    );
    let g = layout(&d, 250.0);
    assert_eq!(g[2], (0.0, 0.0, 50.0, 50.0));
    assert_eq!(g[3], (60.0, 0.0, 90.0, 20.0));
    assert_eq!(g[4], (160.0, 0.0, 90.0, 20.0));
    assert_eq!(g[5], (60.0, 30.0, 190.0, 20.0));
}

#[test]
fn grid_lines_win_over_span_and_end_takes_a_span() {
    // With both lines set, `span` is ignored (CSS: `span 5` then `1` and `3` gives 1 / 3).
    let d = doc(
        r#"{"type":"box","style":{"display":"grid","gridTemplateColumns":[10,10,10,10,10,10],"gridTemplateRows":[10,10]},"children":[
            {"type":"box","style":{"gridColumn":{"start":1,"end":3,"span":5}}},
            {"type":"box","style":{"gridRow":{"start":2},"gridColumn":{"end":5,"span":2}}}]}"#,
    );
    let g = layout(&d, 60.0);
    assert_eq!(g[2], (0.0, 0.0, 20.0, 10.0));
    assert_eq!(g[3], (20.0, 10.0, 20.0, 10.0));
}

#[test]
fn root_min_height_lets_flex_grow_fill_the_page() {
    let d = doc(
        r#"{"type":"box","style":{"height":10}},{"type":"box","style":{"flexGrow":1}},{"type":"box","style":{"height":10}}"#,
    );
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 100.0, 300.0, &mut NoContent).unwrap();
    assert_eq!(tree.nodes[0].geo.h, 300.0);
    assert_eq!(tree.nodes[3].geo.y, 290.0);
}

#[test]
fn page_children_take_percentage_heights_of_the_page() {
    let d = doc(
        r#"{"type":"box","style":{"height":"50%"},"children":[{"type":"box","style":{"height":"50%"}}]},
           {"type":"box","style":{"minHeight":"10%","maxHeight":"5%"}},
           {"type":"box","style":{"height":10}}"#,
    );
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 100.0, 400.0, &mut NoContent).unwrap();
    let g = |i: usize| (tree.nodes[i].geo.y, tree.nodes[i].geo.h);
    assert_eq!(g(1), (0.0, 200.0));
    assert_eq!(g(4), (0.0, 100.0), "half of its parent");
    assert_eq!(g(2), (200.0, 40.0), "minHeight wins over maxHeight");
    assert_eq!(g(3).0, 240.0);
    // Without a page height (a header or footer), they stay `auto`.
    let mut tree = Tree::build(&d.pages[0].children);
    compute(&mut tree, 100.0, 0.0, &mut NoContent).unwrap();
    assert_eq!(tree.nodes[1].geo.h, 0.0);
}

#[test]
fn a_cell_height_is_a_minimum() {
    let table = |prop: &str, height: u32| {
        doc(&format!(
            r#"{{"type":"table","columns":[{{"width":100}}],"rows":[
                {{"cells":[{{"style":{{"{prop}":{height}}},"children":[{{"type":"box","style":{{"height":30}}}}]}}]}},
                {{"cells":[{{"children":[{{"type":"box","style":{{"height":5}}}}]}}]}}]}}"#
        ))
    };
    // `minHeight` is the same minimum.
    for prop in ["height", "minHeight"] {
        // Root, table, two rows, two cells, a box in each cell.
        let short = layout(&table(prop, 10), 100.0);
        assert_eq!(short[2].3, 30.0, "{prop}: the row grows to the content");
        assert_eq!(short[3].1, 30.0, "{prop}");
        assert_eq!(
            short[7].1, 30.0,
            "{prop}: the next row starts below the content"
        );
        let tall = layout(&table(prop, 50), 100.0);
        assert_eq!(
            (tall[2].3, tall[4].3, tall[3].1),
            (50.0, 50.0, 50.0),
            "{prop}"
        );
    }
}

/// The closed-form cell layout (layout/fast.rs) must match Taffy exactly.
#[test]
fn simple_cell_fast_path_matches_taffy() {
    let store = crate::test_fixtures::fonts(&[
        "Inter-Regular",
        "Inter-Bold",
        "Inter-Italic",
        "NotoSans-Regular",
        "RobotoMono-Regular",
    ]);
    let cell = |style: &str, text: &str| {
        format!(
            r#"{{"style":{{{style}}},"children":[{{"type":"text","style":{{"maxLines":2,"ellipsis":true}},"runs":[{{"text":"{text}","font":"Inter-Regular","size":9}}]}}]}}"#
        )
    };
    let synthetic = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":400,"height":800}},"children":[
            {{"type":"table","columns":[{{"width":"auto"}},{{"width":"1fr"}},{{"width":60}}],"rows":[
              {{"minHeight":40,"cells":[{a},{b},{c}]}},
              {{"cells":[{d},{e},{f}]}}]}}]}}]}}"#,
        a = cell(
            r#""padding":{"top":2,"left":4},"verticalAlign":"middle","borderWidth":{"bottom":2}"#,
            "Short"
        ),
        b = cell(
            r#""verticalAlign":"bottom","padding":{"right":6}"#,
            "A longer cell that wraps over more than two lines so that the ellipsis cuts it"
        ),
        c = cell("", "Fixed-width column"),
        d = cell(
            r#""padding":{"top":1,"right":1,"bottom":1,"left":1},"borderWidth":{"top":1,"right":3,"bottom":1,"left":1}"#,
            "x"
        ),
        e = cell(
            r#""backgroundColor":[1,0,0,1]"#,
            "Hyphenation-less wordsthatareverylongindeed"
        ),
        f = cell(r#""verticalAlign":"middle""#, ""),
    );
    let mut docs = vec![synthetic.into_bytes()];
    for f in ["table-basic", "table-split-header-repeat", "invoice"] {
        docs.push(crate::test_fixtures::fixture(&format!("docs/{f}.json")));
    }
    for json in docs {
        let d = crate::validate::parse(&json, InputFormat::Json).unwrap();
        let fonts = FontSet::load(&d, &store).unwrap();
        let spec = &d.pages[0];
        let w = spec.size.width - spec.margin.left - spec.margin.right;
        let run = |fast: bool| {
            let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
            let mut tree = Tree::build(&spec.children);
            compute_tree(&mut tree, w, 0.0, &mut cache, fast).unwrap();
            tree.nodes.iter().map(|n| n.geo).collect::<Vec<_>>()
        };
        let (fast, slow) = (run(true), run(false));
        assert_eq!(fast.len(), slow.len());
        for (i, (a, b)) in fast.iter().zip(&slow).enumerate() {
            let close = |x: f32, y: f32| (x - y).abs() < 1e-3;
            let sides_close = |x: Edges<f32>, y: Edges<f32>| {
                x.to_array()
                    .into_iter()
                    .zip(y.to_array())
                    .all(|(x, y)| close(x, y))
            };
            let same = close(a.x, b.x)
                && close(a.y, b.y)
                && close(a.w, b.w)
                && close(a.h, b.h)
                && sides_close(a.border, b.border)
                && sides_close(a.padding, b.padding);
            assert!(same, "node {i}: fast {a:?} vs taffy {b:?}");
        }
    }
}

/// xorshift64: deterministic pseudo-random numbers.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }

    fn pick<'s>(&mut self, xs: &[&'s str]) -> &'s str {
        xs[self.below(xs.len() as u64) as usize]
    }
}

/// A random column box, text or spacer, `depth` levels deep at most.
/// Boxes stretch across their parent: `flow.rs` sizes a box that does not
/// from its content more simply than Taffy (see the module).
fn random_node(r: &mut Rng, depth: u32) -> String {
    let kind = if depth == 0 {
        1 + r.below(2)
    } else {
        r.below(3)
    };
    let mut style: Vec<String> = Vec::new();
    let length = |r: &mut Rng| -> String {
        match r.below(4) {
            0 => format!("\"{}%\"", r.below(60)),
            _ => format!("{}", r.below(40)),
        }
    };
    if r.below(3) == 0 {
        let side = r.pick(&["top", "right", "bottom", "left"]);
        let v = if r.below(4) == 0 && kind != 0 && (side == "left" || side == "right") {
            "\"auto\"".to_string()
        } else {
            length(r)
        };
        style.push(format!(r#""margin":{{"{side}":{v}}}"#));
    }
    if r.below(3) == 0 {
        style.push(format!(
            r#""padding":{{"top":{},"left":{}}}"#,
            r.below(10),
            r.below(10)
        ));
    }
    if r.below(4) == 0 {
        style.push(format!(r#""borderWidth":{{"bottom":{}}}"#, r.below(5)));
    }
    // A minimum wider than a set width makes Taffy size a box's content
    // at the set width first: not generated.
    if r.below(4) == 0 {
        // Boxes narrower than their padding: not generated either.
        let w = if kind == 0 {
            format!("{}", 30 + r.below(200))
        } else {
            length(r)
        };
        style.push(format!(r#""width":{w}"#));
    } else if kind != 1 && r.below(5) == 0 {
        // A text leaf wider than its room is measured in the room by Taffy,
        // and at its own width here: not generated.
        style.push(format!(r#""minWidth":{}"#, r.below(80)));
    }
    // Taffy sizes the content of a stretched box or text with a maximum
    // width at another width than the one it ends up with: not generated.
    if kind == 2 && r.below(6) == 0 {
        style.push(format!(r#""maxWidth":{}"#, 20 + r.below(200)));
    }
    if kind == 0 {
        style.push(r#""alignSelf":"stretch""#.into());
    } else if r.below(5) == 0 {
        let a = r.pick(&[
            "flex-start",
            "center",
            "flex-end",
            "stretch",
            "start",
            "end",
        ]);
        style.push(format!(r#""alignSelf":"{a}""#));
    }
    if r.below(6) == 0 {
        style.push(format!(
            r#""inset":{{"top":{},"left":{}}}"#,
            r.below(9),
            r.below(9)
        ));
    }
    match kind {
        0 => {
            if r.below(3) == 0 {
                let a = r.pick(&["flex-start", "center", "flex-end", "stretch"]);
                style.push(format!(r#""alignItems":"{a}""#));
            }
            if r.below(3) == 0 {
                style.push(format!(r#""rowGap":{}"#, r.below(12)));
            }
            // No set height: a column flexbox shrinks its items to fit one,
            // which a box with floats does not do.
            if r.below(4) == 0 {
                style.push(format!(r#""minHeight":{}"#, 50 + r.below(150)));
            }
            let n = 1 + r.below(4);
            let kids: Vec<String> = (0..n).map(|_| random_node(r, depth - 1)).collect();
            format!(
                r#"{{"type":"box","style":{{{}}},"children":[{}]}}"#,
                style.join(","),
                kids.join(",")
            )
        }
        1 => {
            let words = [
                "Lorem",
                "ipsum",
                "dolor",
                "sit",
                "amet,",
                "consectetur",
                "elit",
            ];
            let n = 1 + r.below(20);
            let text: Vec<&str> = (0..n).map(|_| r.pick(&words)).collect();
            format!(
                r#"{{"type":"text","style":{{{}}},"runs":[{{"text":"{}","font":"Inter-Regular","size":{}}}]}}"#,
                style.join(","),
                text.join(" "),
                6 + r.below(10)
            )
        }
        _ => {
            style.push(format!(r#""height":{}"#, r.below(30)));
            format!(r#"{{"type":"spacer","style":{{{}}}}}"#, style.join(","))
        }
    }
}

/// `flow.rs` lays out column boxes without floats as Taffy does, so that a
/// box gaining a float changes only what the float touches.
#[test]
fn float_free_column_boxes_lay_out_as_taffy_does() {
    let store = crate::test_fixtures::fonts(&["Inter-Regular"]);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..300 {
        let n = 1 + r.below(4);
        let kids: Vec<String> = (0..n).map(|_| random_node(&mut r, 3)).collect();
        let json = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
                "pages":[{{"size":{{"width":400,"height":800}},"children":[{}]}}]}}"#,
            kids.join(",")
        );
        let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
        let fonts = FontSet::load(&d, &store).unwrap();
        let min_height = if case % 2 == 0 { 0.0 } else { 700.0 };
        let run = |flowing: bool| {
            let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
            let mut tree = Tree::build(&d.pages[0].children);
            if flowing {
                compute_tree_flowing(&mut tree, 380.0, min_height, &mut cache).unwrap();
            } else {
                compute_tree(&mut tree, 380.0, min_height, &mut cache, true).unwrap();
            }
            tree.nodes.iter().map(|n| n.geo).collect::<Vec<_>>()
        };
        let (taffy, flow) = (run(false), run(true));
        let close = |x: f32, y: f32| (x - y).abs() < 0.01;
        let differ: Vec<String> = taffy
            .iter()
            .zip(&flow)
            .enumerate()
            .filter(|(_, (a, b))| {
                !(close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h))
            })
            .map(|(i, (a, b))| {
                format!(
                    "node {i}: taffy ({}, {}, {}, {}) flow ({}, {}, {}, {})",
                    a.x, a.y, a.w, a.h, b.x, b.y, b.w, b.h
                )
            })
            .collect();
        assert!(
            differ.is_empty(),
            "case {case}:\n{}\n{json}",
            differ.join("\n")
        );
    }
}

/// Lays out `children` of a page `width` wide with Inter, and runs `f` on
/// the tree and the text cache.
fn with_floats<R>(
    children: &str,
    width: f32,
    f: impl FnOnce(&Tree<'_>, &mut TextCache<'_, RustybuzzShaper<'_, '_>>) -> R,
) -> R {
    let store = crate::test_fixtures::fonts(&["Inter-Regular"]);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":600,"height":800}},"children":[{children}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut tree = Tree::build(&d.pages[0].children);
    compute_tree(&mut tree, width, 0.0, &mut cache, true).unwrap();
    f(&tree, &mut cache)
}

const WORDS: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
    tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis \
    nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";

fn text(style: &str) -> String {
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"{WORDS}","font":"Inter-Regular","size":10}}]}}"#
    )
}

#[test]
fn text_wraps_beside_a_float_and_the_container_holds_it() {
    let children = format!(
        r#"{{"type":"box","children":[
            {{"type":"box","style":{{"float":"left","width":80,"height":50,"margin":{{"right":10}}}}}},
            {}]}}"#,
        text("")
    );
    with_floats(&children, 300.0, |tree, cache| {
        let (b, f, t) = (&tree.nodes[1], &tree.nodes[2], &tree.nodes[3]);
        assert_eq!((f.geo.x, f.geo.y, f.geo.w, f.geo.h), (0.0, 0.0, 80.0, 50.0));
        // The paragraph keeps its full width; its lines beside the float
        // are narrower.
        assert_eq!((t.geo.x, t.geo.y, t.geo.w), (0.0, 0.0, 300.0));
        let boxes = tree.line_boxes(3);
        assert!(boxes.banded());
        let layout = cache.layout_in(t.node().unwrap(), &boxes).unwrap();
        for (line, b) in layout.lines.iter().zip(&layout.boxes) {
            let (left, avail) = (b.left, b.width);
            if line.top < 50.0 {
                assert_eq!((left, avail), (90.0, 210.0), "{line:?}");
            } else {
                assert_eq!((left, avail), (0.0, 300.0), "{line:?}");
            }
            assert!(line.width <= avail + 1e-3);
        }
        assert!((t.geo.h - layout.height).abs() < 1e-3);
        assert!(b.geo.h >= t.geo.h.max(50.0) - 1e-3);
        // The group: the float and the lines beside it.
        let end = tree.float_groups[&2];
        assert!(end > 50.0 && end <= t.geo.h, "{end}");
    });
}

#[test]
fn a_box_beside_a_float_is_narrowed_or_moved_below_it() {
    let children = r#"{"type":"box","children":[
        {"type":"box","style":{"float":"right","width":100,"height":40}},
        {"type":"box","style":{"height":10}},
        {"type":"box","style":{"width":250,"height":10}},
        {"type":"box","style":{"height":10,"clear":"right"}}]}"#;
    with_floats(children, 300.0, |tree, _| {
        let g = |i: usize| {
            let g = tree.nodes[i].geo;
            (g.x, g.y, g.w, g.h)
        };
        assert_eq!(g(2), (200.0, 0.0, 100.0, 40.0));
        // Narrowed to the room on the left of the float.
        assert_eq!(g(3), (0.0, 0.0, 200.0, 10.0));
        // Too wide for that room: below the float.
        assert_eq!(g(4), (0.0, 40.0, 250.0, 10.0));
        assert_eq!(g(5), (0.0, 50.0, 300.0, 10.0));
        assert_eq!(g(1).3, 60.0);
    });
}

#[test]
fn clear_and_the_container_height_count_the_floats() {
    let children = r#"{"type":"box","children":[
        {"type":"box","style":{"float":"left","width":50,"height":70}},
        {"type":"box","style":{"float":"left","width":50,"height":30}},
        {"type":"box","style":{"height":5,"clear":"left","margin":{"top":4}}}]},
        {"type":"box","children":[{"type":"box","style":{"float":"left","width":10,"height":90}}]}"#;
    with_floats(children, 300.0, |tree, _| {
        let n = &tree.nodes;
        assert_eq!((n[4].geo.x, n[4].geo.y), (50.0, 0.0));
        // Its border box below both floats' margin boxes.
        assert_eq!(n[5].geo.y, 70.0);
        assert_eq!(n[1].geo.h, 75.0);
        // A box with only a float is as tall as the float.
        assert_eq!(n[2].geo.h, 90.0);
        assert_eq!(n[2].geo.y, 75.0);
    });
}

#[test]
fn floats_shrink_to_fit_and_work_in_cells_and_the_page() {
    let children = format!(
        r#"{{"type":"text","style":{{"float":"right"}},"runs":[{{"text":"Note","font":"Inter-Regular","size":10}}]}},
           {},
           {{"type":"table","columns":[{{"width":200}}],"rows":[{{"cells":[{{"children":[
             {{"type":"box","style":{{"float":"left","width":40,"height":20}}}},{}]}}]}}]}}"#,
        text(""),
        text("")
    );
    with_floats(&children, 300.0, |tree, _| {
        let note = tree.nodes[1].geo;
        assert!(
            note.w < 40.0 && (note.x + note.w - 300.0).abs() < 1e-3,
            "{note:?}"
        );
        assert!(tree.wraps.contains_key(&2));
        // In the cell: the text after the float wraps beside it.
        let wrapped: Vec<_> = tree.wraps.keys().copied().collect();
        assert_eq!(wrapped.len(), 2, "{wrapped:?}");
    });
}

#[test]
fn a_note_in_a_box_with_floats_is_a_zero_height_leaf() {
    let children = format!(
        r#"{{"type":"box","children":[
            {{"type":"box","style":{{"float":"left","width":40,"height":30}}}},
            {{"type":"note","note":{{"contents":"Checked"}}}},
            {}]}}"#,
        text("")
    );
    with_floats(&children, 300.0, |tree, _| {
        let note = tree.nodes[3].geo;
        assert_eq!((note.y, note.h), (0.0, 0.0));
        // The text after it still wraps beside the float.
        assert_eq!(tree.nodes[4].geo.y, 0.0);
        assert!(tree.wraps.contains_key(&4));
    });
}

#[test]
fn a_narrowed_box_keeps_clear_of_a_wider_float_lower_down() {
    // A second, wider float sits below the first; a box 80 tall beside
    // the first reaches down beside the second too.
    let floats = r#"{"type":"box","style":{"float":"left","width":100,"height":50}},
        {"type":"box","style":{"float":"left","width":350,"height":50}}"#;
    let children = format!(
        r#"{{"type":"box","children":[{floats},{{"type":"box","style":{{"width":50,"height":80}}}}]}},
           {{"type":"box","children":[{floats},{{"type":"box","style":{{"height":80}}}}]}},
           {{"type":"box","children":[{floats},{{"type":"box","style":{{"width":60,"height":80}}}}]}}"#
    );
    with_floats(&children, 400.0, |tree, _| {
        // In its parent's content box.
        let g = |i: usize| {
            let g = tree.nodes[i].geo;
            let p = tree.nodes[tree.nodes[i].parent as usize].geo;
            (g.x - p.x, g.y - p.y, g.w, g.h)
        };
        assert_eq!(g(5), (0.0, 50.0, 350.0, 50.0));
        // Beside both floats, right of the wider one.
        assert_eq!(g(6), (350.0, 0.0, 50.0, 80.0));
        // A box that stretches is narrowed to the room beside both.
        assert_eq!(g(9), (350.0, 0.0, 50.0, 80.0));
        // Too wide for that room: below both floats.
        assert_eq!(g(12), (0.0, 100.0, 60.0, 80.0));
        for (f, b) in [(4, 6), (5, 6), (7, 9), (8, 9), (10, 12), (11, 12)] {
            let (f, b) = (tree.nodes[f].geo, tree.nodes[b].geo);
            let apart = f.x + f.w <= b.x + 1e-3
                || b.x + b.w <= f.x + 1e-3
                || f.y + f.h <= b.y + 1e-3
                || b.y + b.h <= f.y + 1e-3;
            assert!(apart, "{f:?} meets {b:?}");
        }
    });
}

#[test]
fn auto_margins_share_the_free_room_beside_floats() {
    let children = r#"{"type":"box","style":{"height":200},"children":[
        {"type":"box","style":{"float":"left","width":50,"height":30}},
        {"type":"box","style":{"height":20}},
        {"type":"box","style":{"height":20,"margin":{"top":"auto"}}}]}"#;
    with_floats(children, 300.0, |tree, _| {
        let g = |i: usize| {
            let g = tree.nodes[i].geo;
            (g.x, g.y, g.w, g.h)
        };
        // Pushed to the bottom, as without the float.
        assert_eq!(g(4), (0.0, 180.0, 300.0, 20.0));
        assert_eq!(tree.nodes[4].geo.margin.top, 160.0);
        assert_eq!(g(3), (50.0, 0.0, 250.0, 20.0));
    });
}

#[test]
fn the_max_content_width_counts_the_floats_beside_a_child() {
    // Floats shrink to fit: to their max-content width here.
    let children = r#"{"type":"box","children":[{"type":"box","style":{"float":"left"},"children":[
            {"type":"box","style":{"float":"left","width":50,"height":20}},
            {"type":"box","style":{"width":30,"height":10}}]}]},
        {"type":"box","children":[{"type":"box","style":{"float":"left"},"children":[
            {"type":"box","style":{"float":"left","width":50,"height":20}},
            {"type":"box","style":{"width":30,"height":10,"clear":"left"}}]}]}"#;
    with_floats(children, 300.0, |tree, _| {
        let n = &tree.nodes;
        // The box sits beside the float: the floated box holds both.
        assert_eq!(n[3].geo.w, 80.0);
        assert_eq!(n[6].geo.x - n[3].geo.x, 50.0);
        // Below it, after `clear`: as wide as the wider of the two.
        assert_eq!(n[4].geo.w, 50.0);
        assert_eq!(n[8].geo.y - n[4].geo.y, 20.0);
    });
}

#[test]
fn text_with_a_width_or_an_alignment_wraps_line_by_line() {
    for style in [
        r#""width":250"#,
        r#""alignSelf":"center""#,
        r#""margin":{"left":"auto","right":20}"#,
    ] {
        let children = format!(
            r#"{{"type":"box","children":[
                {{"type":"box","style":{{"float":"left","width":80,"height":50}}}},
                {}]}}"#,
            text(style)
        );
        with_floats(&children, 300.0, |tree, cache| {
            let t = tree.nodes[3].geo;
            // Its own box, beside the float: not moved below it.
            assert_eq!(t.y, 0.0, "{style}");
            assert!(tree.wraps.contains_key(&3), "{style}");
            let boxes = tree.line_boxes(3);
            let layout = cache
                .layout_in(tree.nodes[3].node().unwrap(), &boxes)
                .unwrap();
            let (cx, cw) = (t.content_box().x, t.content_box().w);
            for (line, b) in layout.lines.iter().zip(&layout.boxes) {
                assert!(line.width <= b.width + 1e-3, "{style}: {line:?}");
                if line.top < 50.0 {
                    // Right of the float.
                    assert!(cx + b.left >= 80.0 - 1e-3, "{style}: {b:?}");
                } else {
                    assert_eq!((b.left, b.width), (0.0, cw), "{style}");
                }
            }
            assert!(layout.boxes.iter().any(|b| b.left > 0.0), "{style}");
        });
    }
}

/// A paragraph of `size` pt, with `style`.
fn words(size: u32, style: &str) -> String {
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"Baseline","font":"Inter-Regular","size":{size}}}]}}"#
    )
}

#[test]
fn baseline_alignment_lines_up_first_baselines() {
    // Texts of different sizes, one below padding and one in a box below
    // its padding and border: their first baselines line up, as in CSS.
    let children = format!(
        r#"{{"type":"box","style":{{"flexDirection":"row","alignItems":"baseline"}},"children":[
            {},{},
            {{"type":"box","style":{{"padding":{{"top":7}},"borderWidth":{{"top":2}}}},"children":[{}]}},
            {}]}}"#,
        words(40, ""),
        words(10, r#""padding":{"top":3}"#),
        words(10, ""),
        words(20, r#""alignSelf":"flex-start""#),
    );
    with_floats(&children, 500.0, |tree, cache| {
        let baselines: Vec<f32> = tree
            .nodes
            .iter()
            .filter(|n| {
                n.node()
                    .is_some_and(|n| matches!(n.kind, NodeKind::Text { .. }))
            })
            .map(|n| {
                let c = n.geo.content_box();
                let l = cache.layout(n.node().unwrap(), c.w).unwrap();
                c.y + l.lines[0].top + l.lines[0].ascent
            })
            .collect();
        // Breadth first: the row's three texts, then the one in the box.
        let &[big, padded, start, nested] = &baselines[..] else {
            panic!("{baselines:?}");
        };
        for b in [padded, nested] {
            assert!((b - big).abs() < 1e-3, "{baselines:?}");
        }
        // The flex-start one is at the top, out of baseline alignment.
        assert!(start < big - 1.0, "{baselines:?}");
    });
}
