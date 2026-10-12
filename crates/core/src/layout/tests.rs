use super::*;
use crate::schema::Document;
use crate::InputFormat;

fn doc(children: &str) -> Document {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":600,"height":800}},"children":[{children}]}}]}}"#
    );
    crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap()
}

fn layout(d: &Document, width: f32) -> Vec<(f32, f32, f32, f32)> {
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, width, 0.0, &mut NoContent)
        .unwrap();
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
    TaffyLayout
        .compute(&mut tree, 100.0, 0.0, &mut NoContent)
        .unwrap();
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
fn root_min_height_lets_flex_grow_fill_the_page() {
    let d = doc(
        r#"{"type":"box","style":{"height":10}},{"type":"box","style":{"flexGrow":1}},{"type":"box","style":{"height":10}}"#,
    );
    let mut tree = Tree::build(&d.pages[0].children);
    TaffyLayout
        .compute(&mut tree, 100.0, 300.0, &mut NoContent)
        .unwrap();
    assert_eq!(tree.nodes[0].geo.h, 300.0);
    assert_eq!(tree.nodes[3].geo.y, 290.0);
}

#[test]
fn a_cell_height_is_a_minimum() {
    let table = |height: u32| {
        doc(&format!(
            r#"{{"type":"table","columns":[{{"width":100}}],"rows":[
                {{"cells":[{{"style":{{"height":{height}}},"children":[{{"type":"box","style":{{"height":30}}}}]}}]}},
                {{"cells":[{{"children":[{{"type":"box","style":{{"height":5}}}}]}}]}}]}}"#
        ))
    };
    // Root, table, two rows, two cells, a box in each cell.
    let short = layout(&table(10), 100.0);
    assert_eq!(short[2].3, 30.0, "the row grows to the content");
    assert_eq!(short[3].1, 30.0);
    assert_eq!(short[7].1, 30.0, "the next row starts below the content");
    let tall = layout(&table(50), 100.0);
    assert_eq!((tall[2].3, tall[4].3, tall[3].1), (50.0, 50.0, 50.0));
}
