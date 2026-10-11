//! `debug` outlines through layout, pagination and paint: which boxes are
//! outlined on which page, and that nothing else changes.

use super::*;
use crate::images::{ImageCrateDecoder, ImageStore};
use crate::paint::debug::{BORDER, CONTENT, LABEL_BG, MARGIN, PADDING};
use crate::paint::{PathSeg, Rect};
use crate::{FontStore, InputFormat, RenderOptions};

const W: f32 = 200.0;
const H: f32 = 100.0;

fn pages(json: &str) -> Vec<PageContent> {
    let store = FontStore::new();
    let doc = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    run(&doc, &fonts, &mut text, &images, Ok).unwrap().0
}

/// One page of `W × H` with `page` properties and `children`.
fn doc(page: &str, children: &str) -> String {
    format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}}{page},
            "children":[{children}]}}]}}"#
    )
}

/// The rectangle a path starts with (`MoveTo`, three `LineTo`s).
fn first_rect(segs: &[PathSeg]) -> Rect {
    match segs[..3] {
        [PathSeg::MoveTo(x, y), PathSeg::LineTo(r, _), PathSeg::LineTo(_, b)] => {
            Rect::new(x, y, r - x, b - y)
        }
        _ => panic!("not a rectangle: {segs:?}"),
    }
}

/// An outlined content box, and whether it is drawn inside the clip of
/// the flow's content band.
#[derive(Debug, PartialEq)]
struct Outline {
    content: Rect,
    clipped: bool,
}

/// The content boxes outlined on a page, in drawing order.
fn outlines(page: &PageContent) -> Vec<Outline> {
    let mut clips = vec![false];
    let mut out = Vec::new();
    for op in &page.ops {
        match op {
            Op::Save => clips.push(*clips.last().unwrap()),
            Op::Restore => {
                clips.pop();
            }
            Op::ClipRect(_) => *clips.last_mut().unwrap() = true,
            Op::StrokePath { path, color, .. } if *color == CONTENT => out.push(Outline {
                content: first_rect(&path.segs),
                clipped: *clips.last().unwrap(),
            }),
            _ => {}
        }
    }
    out
}

/// Outer and inner rectangles of the rings filled in `color`.
fn rings(page: &PageContent, color: crate::schema::Color) -> Vec<(Rect, Rect)> {
    page.ops
        .iter()
        .filter_map(|op| match op {
            Op::FillPath { path, color: c, .. } if *c == color => {
                Some((first_rect(&path.segs), first_rect(&path.segs[5..])))
            }
            _ => None,
        })
        .collect()
}

/// Label backgrounds.
fn labels(page: &PageContent) -> Vec<Rect> {
    page.ops
        .iter()
        .filter_map(|op| match op {
            Op::FillRect { rect, color } if *color == LABEL_BG => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn nested_boxes_show_margin_border_padding_and_content() {
    let json = doc(
        r#","margin":{"top":10,"right":10,"bottom":10,"left":10}"#,
        r#"{"type":"box","debug":true,"style":{"height":40,
            "margin":{"top":5,"right":5,"bottom":5,"left":5},
            "borderWidth":{"top":2,"right":2,"bottom":2,"left":2},
            "padding":{"top":3,"right":3,"bottom":3,"left":3}},
            "children":[{"type":"box","debug":true,"style":{"height":10}},
                        {"type":"box","style":{"height":10}}]}"#,
    );
    let pages = pages(&json);
    assert_eq!(pages.len(), 1);
    let p = &pages[0];
    // The outer box and its first child; the second child has no `debug`.
    assert_eq!(
        outlines(p),
        [
            Outline {
                content: Rect::new(20.0, 20.0, 160.0, 30.0),
                clipped: true
            },
            Outline {
                content: Rect::new(20.0, 20.0, 160.0, 10.0),
                clipped: true
            },
        ]
    );
    let border_box = Rect::new(15.0, 15.0, 170.0, 40.0);
    assert_eq!(
        rings(p, MARGIN),
        [(Rect::new(10.0, 10.0, 180.0, 50.0), border_box)]
    );
    assert_eq!(
        rings(p, BORDER),
        [(border_box, Rect::new(17.0, 17.0, 166.0, 36.0))]
    );
    assert_eq!(
        rings(p, PADDING),
        [(
            Rect::new(17.0, 17.0, 166.0, 36.0),
            Rect::new(20.0, 20.0, 160.0, 30.0)
        )]
    );
    // One label each, at the top-left corner of the border box; the child's
    // moves to its top-right corner rather than cover the parent's.
    let l = labels(p);
    assert_eq!(l.len(), 2);
    assert_eq!((l[0].x, l[0].y), (15.0, 15.0));
    assert_eq!(l[1].y, 20.0);
    assert!((l[1].x + l[1].w - 180.0).abs() < 1e-3, "{l:?}");
}

#[test]
fn a_box_split_across_pages_is_outlined_on_each_page() {
    let json = doc(
        "",
        r#"{"type":"box","debug":true,"children":[
            {"type":"box","style":{"height":60}},
            {"type":"box","style":{"height":60}}]}"#,
    );
    let pages = pages(&json);
    assert_eq!(pages.len(), 2);
    // Laid out 120 high; pagination pushes the second child to page 2,
    // so the box is painted 160 high: sliced at each page's band.
    assert_eq!(
        outlines(&pages[0]),
        [Outline {
            content: Rect::new(0.0, 0.0, W, 160.0),
            clipped: true
        }]
    );
    assert_eq!(
        outlines(&pages[1]),
        [Outline {
            content: Rect::new(0.0, -100.0, W, 160.0),
            clipped: true
        }]
    );
    // Each slice carries its label at its own top.
    for p in &pages {
        let l = labels(p);
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].x, l[0].y), (0.0, 0.0));
    }
}

#[test]
fn header_and_footer_bands_are_outlined_on_every_page() {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}},
            "margin":{{"top":20,"bottom":20}},
            "header":{{"type":"box","debug":true,"style":{{"height":15}}}},
            "footer":{{"type":"box","debug":true,"style":{{"height":10}}}},
            "children":[{{"type":"box","style":{{"height":150}}}}]}}]}}"#
    );
    let pages = pages(&json);
    assert_eq!(pages.len(), 3);
    for p in &pages {
        assert_eq!(
            outlines(p),
            [
                Outline {
                    content: Rect::new(0.0, 0.0, W, 15.0),
                    clipped: false
                },
                Outline {
                    content: Rect::new(0.0, H - 10.0, W, 10.0),
                    clipped: false
                },
            ]
        );
    }
}

#[test]
fn a_fixed_element_is_outlined_on_every_page() {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}},
            "margin":{{"top":10,"left":10}},
            "children":[
                {{"type":"box","style":{{"height":150}}}},
                {{"type":"box","debug":true,"break":{{"fixed":true}},
                  "style":{{"position":"absolute","inset":{{"top":5,"left":5}},
                            "width":30,"height":20}}}}]}}]}}"#
    );
    let pages = pages(&json);
    assert_eq!(pages.len(), 2);
    // Placed in the page content box on every page, and not clipped to
    // the flow's band, like the element itself.
    for p in &pages {
        assert_eq!(
            outlines(p),
            [Outline {
                content: Rect::new(15.0, 15.0, 30.0, 20.0),
                clipped: false
            }]
        );
    }
}

#[test]
fn debug_on_the_page_outlines_every_element() {
    let json = doc(
        r#","margin":{"top":20,"right":10,"bottom":20,"left":10},"debug":true,
           "header":{"type":"box","style":{"height":15}}"#,
        r#"{"type":"table","columns":[{"width":"1fr"},{"width":"1fr"}],"rows":[
            {"cells":[{"children":[{"type":"box","style":{"height":10}}]},
                      {"children":[{"type":"spacer","style":{"height":10}}]}]}]}"#,
    );
    let pages = pages(&json);
    let o = outlines(&pages[0]);
    // Page, table, row, two cells, their two children, header.
    assert_eq!(o.len(), 8, "{o:?}");
    // The page: its content area, inside its margins.
    assert_eq!(o[0].content, Rect::new(10.0, 20.0, W - 20.0, H - 40.0));
    assert_eq!(
        rings(&pages[0], MARGIN)[0],
        (
            Rect::new(0.0, 0.0, W, H),
            Rect::new(10.0, 20.0, W - 20.0, H - 40.0)
        )
    );
    // Table, row, then each cell and its content.
    let flow: Vec<Rect> = o[1..7].iter().map(|o| o.content).collect();
    let half = (W - 20.0) / 2.0;
    assert_eq!(
        flow,
        [
            Rect::new(10.0, 20.0, W - 20.0, 10.0),
            Rect::new(10.0, 20.0, W - 20.0, 10.0),
            Rect::new(10.0, 20.0, half, 10.0),
            Rect::new(10.0, 20.0, half, 10.0),
            Rect::new(10.0 + half, 20.0, half, 10.0),
            Rect::new(10.0 + half, 20.0, half, 10.0),
        ]
    );
    assert!(o[1..7].iter().all(|o| o.clipped));
    assert_eq!(o[7].content, Rect::new(0.0, 0.0, W, 15.0));
}

#[test]
fn a_rotated_box_is_outlined_in_its_own_transform() {
    let json = doc(
        "",
        r#"{"type":"box","debug":true,"style":{"width":40,"height":20,"rotate":90}}"#,
    );
    let pages = pages(&json);
    let ops = &pages[0].ops;
    let stroke = ops
        .iter()
        .position(|op| matches!(op, Op::StrokePath { color, .. } if *color == CONTENT))
        .unwrap();
    let painted = ops
        .iter()
        .find_map(|op| match op {
            Op::Transform(m) => Some(*m),
            _ => None,
        })
        .unwrap();
    // The outline uses the transform the box was painted with.
    let outline = ops[..stroke]
        .iter()
        .rev()
        .find_map(|op| match op {
            Op::Transform(m) => Some(*m),
            _ => None,
        })
        .unwrap();
    assert_eq!(outline, painted);
    let transforms = ops.iter().filter(|op| matches!(op, Op::Transform(_)));
    assert_eq!(transforms.count(), 2);
}

/// A document with text, a link, a bookmark and a form field.
fn rich(debug: &str) -> String {
    format!(
        r##"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}}{debug},
            "header":{{"type":"text"{debug},"runs":[{{"text":"Head","font":"Inter-Regular","size":8}}]}},
            "children":[
              {{"type":"text","id":"top"{debug},"bookmark":{{"title":"Top"}},
                "runs":[{{"text":"Hello world","font":"Inter-Regular","size":10,"link":"https://example.com"}}]}},
              {{"type":"box","link":"#top"{debug},"style":{{"height":150,"padding":{{"top":4}}}}}},
              {{"type":"field"{debug},"field":{{"name":"f","kind":"checkbox"}}}}]}}]}}"##
    )
}

fn render_bytes(json: &str, threads: Option<usize>) -> Vec<u8> {
    let mut fonts = FontStore::new();
    let path = format!(
        "{}/../../fixtures/fonts/Inter-Regular.ttf",
        env!("CARGO_MANIFEST_DIR")
    );
    fonts
        .register("Inter-Regular", std::fs::read(path).unwrap())
        .unwrap();
    let options = RenderOptions {
        threads,
        ..RenderOptions::default()
    };
    crate::render(
        json.as_bytes(),
        InputFormat::Json,
        &options,
        &fonts,
        &ImageStore::new(),
    )
    .unwrap()
}

#[test]
fn debug_false_renders_the_same_bytes_as_no_debug() {
    let plain = render_bytes(&rich(""), None);
    assert_eq!(render_bytes(&rich(r#","debug":false"#), None), plain);
    assert_ne!(render_bytes(&rich(r#","debug":true"#), None), plain);
}

#[test]
fn debug_output_is_deterministic_and_thread_count_independent() {
    let json = rich(r#","debug":true"#);
    let a = render_bytes(&json, None);
    assert_eq!(render_bytes(&json, None), a);
    assert_eq!(render_bytes(&json, Some(1)), a);
}

#[test]
fn debug_adds_no_text_links_or_fields() {
    let store = {
        let mut s = FontStore::new();
        let path = format!(
            "{}/../../fixtures/fonts/Inter-Regular.ttf",
            env!("CARGO_MANIFEST_DIR")
        );
        s.register("Inter-Regular", std::fs::read(path).unwrap())
            .unwrap();
        s
    };
    let paint = |json: &str| {
        let doc = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
        let fonts = FontSet::load(&doc, &store).unwrap();
        let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
        let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
        let (pages, marks) = run(&doc, &fonts, &mut text, &images, Ok).unwrap();
        let kept: Vec<Vec<Op>> = pages
            .into_iter()
            .map(|p| {
                p.ops
                    .into_iter()
                    .filter(|op| matches!(op, Op::Text(_) | Op::Link { .. } | Op::Field(_)))
                    .collect()
            })
            .collect();
        (kept, format!("{marks:?}"))
    };
    let (plain, plain_marks) = paint(&rich(""));
    let (debug, debug_marks) = paint(&rich(r#","debug":true"#));
    // Several pages, each with the header's text.
    assert!(plain.len() > 1);
    assert!(plain.iter().all(|p| !p.is_empty()));
    let all = plain.concat();
    assert!(all.iter().any(|op| matches!(op, Op::Link { .. })));
    assert!(all.iter().any(|op| matches!(op, Op::Field(_))));
    assert_eq!(debug, plain);
    assert_eq!(debug_marks, plain_marks);
}
