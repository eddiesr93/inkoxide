//! `debug` outlines through layout, pagination and paint: which boxes are
//! outlined on which page, and that nothing else changes.

use super::tests::render_with;
use super::*;
use crate::geom::Rect;
use crate::images::ImageStore;
use crate::paint::debug::{BASELINE, BORDER, CONTENT, LABEL_BG, LINE, MARGIN, PADDING, RUN};
use crate::paint::{Op, PathSeg};
use crate::test_fixtures::inter;
use crate::{InputFormat, RenderOptions};

const W: f32 = 200.0;
const H: f32 = 100.0;

fn pages(json: &str) -> Vec<PageContent> {
    render_with(&inter(), json, |pages, _, _| pages)
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

/// The rectangles in the paths filled or stroked in `color`; a line (a
/// baseline) is a rectangle with no height.
fn shapes(page: &PageContent, color: crate::schema::Color) -> Vec<Rect> {
    let mut out = Vec::new();
    for op in &page.ops {
        let path = match op {
            Op::FillPath { path, color: c, .. } | Op::StrokePath { path, color: c, .. }
                if *c == color =>
            {
                path
            }
            _ => continue,
        };
        for (i, seg) in path.segs.iter().enumerate() {
            let PathSeg::MoveTo(x, y) = *seg else {
                continue;
            };
            out.push(match path.segs[i..] {
                [_, PathSeg::LineTo(..), PathSeg::LineTo(..), ..] => first_rect(&path.segs[i..]),
                [_, PathSeg::LineTo(r, b), ..] => Rect::new(x, y, r - x, b - y),
                _ => panic!("{:?}", &path.segs[i..]),
            });
        }
    }
    out
}

/// A text node with `debug` and `runs` (Inter at `size`), in the page.
fn paragraph(page: &str, style: &str, runs: &[(&str, f32)]) -> String {
    let runs: Vec<String> = runs
        .iter()
        .map(|(t, size)| format!(r#"{{"text":"{t}","font":"Inter-Regular","size":{size}}}"#))
        .collect();
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}}{page},"children":[
              {{"type":"text","debug":true,"style":{{{style}}},"runs":[{}]}}]}}]}}"#,
        runs.join(",")
    )
}

#[test]
fn a_paragraph_outlines_each_line_and_the_runs_on_it() {
    let json = paragraph(
        r#","margin":{"left":10,"top":10}"#,
        r#""textAlign":"center""#,
        &[
            ("Small words ", 8.0),
            ("and larger words that wrap onto ", 14.0),
            ("more lines than one or two, in a smaller size", 8.0),
        ],
    );
    let pages = pages(&json);
    let p = &pages[0];
    let content = outlines(p)[0].content;
    let lines = shapes(p, LINE);
    let baselines = shapes(p, BASELINE);
    assert!(lines.len() >= 3, "{lines:?}");
    assert_eq!(baselines.len(), lines.len());
    // Line boxes stack from the top of the content box, centred in it.
    let mut top = content.y;
    for (l, b) in lines.iter().zip(&baselines) {
        assert!((l.y - top).abs() < 1e-3, "{l:?}");
        top += l.h;
        let (left, right) = (l.x - content.x, content.x + content.w - l.x - l.w);
        assert!((left - right).abs() < 1e-3, "{l:?} in {content:?}");
        // The baseline crosses the line box.
        assert_eq!((b.x, b.w), (l.x, l.w));
        assert!(b.y > l.y && b.y < l.y + l.h);
    }
    assert!((top - (content.y + content.h)).abs() < 1e-3);
    // Runs alternate colours; each is in a line box, as high as its size
    // (Inter: ascender 1984, descender -494, 2048 units per em).
    let even = shapes(p, RUN[0]);
    let odd = shapes(p, RUN[1]);
    let inter = |size: f32| size * (1984.0 + 494.0) / 2048.0;
    assert!(
        even.iter().all(|r| (r.h - inter(8.0)).abs() < 1e-3),
        "{even:?}"
    );
    assert!(
        odd.iter().all(|r| (r.h - inter(14.0)).abs() < 1e-3),
        "{odd:?}"
    );
    assert!(even.len() >= 2 && !odd.is_empty());
    for r in even.iter().chain(&odd) {
        let line = lines
            .iter()
            .find(|l| r.y >= l.y - 1e-3 && r.y < l.y + l.h)
            .unwrap_or_else(|| panic!("{r:?} in no line"));
        assert!(r.x >= line.x - 1e-3 && r.x + r.w <= line.x + line.w + 1e-3);
    }
    // Still one outlined content box, and no text drawn for the outline.
    assert_eq!(outlines(p).len(), 1);
}

#[test]
fn a_paragraph_split_across_pages_outlines_the_lines_on_each_page() {
    let words = "word ".repeat(120);
    let json = paragraph("", "", &[(words.as_str(), 10.0)]);
    let pages = pages(&json);
    assert!(pages.len() > 1);
    let mut total = 0;
    for (k, p) in pages.iter().enumerate() {
        let lines = shapes(p, LINE);
        assert!(!lines.is_empty(), "page {k}");
        for l in &lines {
            assert!(l.y >= -1e-3 && l.y + l.h <= H + 1e-3, "page {k}: {l:?}");
        }
        total += lines.len();
    }
    // One line outline per line of the paragraph: one run in one font
    // draws one text op per line.
    let texts: usize = pages
        .iter()
        .map(|p| p.ops.iter().filter(|op| matches!(op, Op::Text(_))).count())
        .sum();
    assert_eq!(total, texts);
    // Without `debug`, nothing of it.
    for p in &self::pages(&json.replace(r#""debug":true"#, r#""debug":false"#)) {
        assert!(shapes(p, LINE).is_empty() && shapes(p, RUN[0]).is_empty());
    }
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
fn an_overlay_under_the_flow_is_outlined_unclipped() {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}},
            "margin":{{"top":10,"left":10}},
            "children":[
                {{"type":"box","debug":true,"style":{{"height":20}}}},
                {{"type":"box","debug":true,"break":{{"fixed":true}},
                  "style":{{"position":"absolute","inset":{{"top":-5,"left":5}},
                            "width":30,"height":20,"zIndex":-1}}}}]}}]}}"#
    );
    let pages = pages(&json);
    // Outlined in paint order, the overlay first; reaching into the
    // margin, it is not clipped to the flow's band.
    assert_eq!(
        outlines(&pages[0]),
        [
            Outline {
                content: Rect::new(15.0, 5.0, 30.0, 20.0),
                clipped: false
            },
            Outline {
                content: Rect::new(10.0, 10.0, W - 10.0, 20.0),
                clipped: true
            },
        ]
    );
}

#[test]
fn an_overlay_with_page_numbers_is_outlined_as_laid_out_on_each_page() {
    // Shrink-wrapped text with page numbers, fixed in the top-left corner,
    // over a flow of three pages.
    let overlay = |runs: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
                "pages":[{{"size":{{"width":{W},"height":{H}}},
                "children":[
                  {{"type":"text","debug":true,"break":{{"fixed":true}},
                    "style":{{"position":"absolute"}},"runs":[{runs}]}},
                  {{"type":"box","style":{{"height":{}}}}}]}}]}}"#,
            3.0 * H - 1.0
        )
    };
    let run = |run: &str| format!(r#"{{{run},"font":"Inter-Regular","size":10}}"#);
    let numbered = overlay(
        &[
            run(r#""text":"Page ""#),
            run(r#""placeholder":"pageNumber""#),
            run(r#""text":" of ""#),
            run(r#""placeholder":"totalPages""#),
        ]
        .join(","),
    );
    let widths: Vec<f32> = pages(&numbered)
        .iter()
        .map(|p| {
            let o = outlines(p);
            assert_eq!(o.len(), 1, "{o:?}");
            o[0].content.w
        })
        .collect();
    assert_eq!(widths.len(), 3);
    // Each page's box is the one its own digits need: as wide as the same
    // text written out, not as wide as the `999` stand-ins.
    for (k, w) in widths.iter().enumerate() {
        let written = overlay(&run(&format!(r#""text":"Page {} of 3""#, k + 1)));
        let want = outlines(&pages(&written)[0])[0].content.w;
        assert!((w - want).abs() < 1e-3, "page {}: {w}, want {want}", k + 1);
    }
    assert!(
        widths[0] != widths[1] && widths[1] != widths[2],
        "{widths:?}"
    );
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
    let fonts = inter();
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
    let store = inter();
    let paint = |json: &str| {
        render_with(&store, json, |pages, marks, _| {
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
        })
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

/// Untrusted documents can hold many elements on one page: placing a
/// label must not look at every label already placed.
#[test]
fn outlined_elements_on_one_page_render_in_linear_time() {
    let side = 224;
    crate::test_timing::assert_linear("outlined elements", 12_500, |n| {
        let kids: Vec<String> = (0..n)
            .map(|k| {
                format!(
                    r#"{{"type":"box","style":{{"position":"absolute",
                        "inset":{{"top":{},"left":{}}},"width":3,"height":3}}}}"#,
                    (k / side) * 62,
                    (k % side) * 62
                )
            })
            .collect();
        let json = format!(
            r#"{{"version":1,"pages":[{{"size":{{"width":14400,"height":14400}},"debug":true,
                "children":[{}]}}]}}"#,
            kids.join(",")
        );
        let start = std::time::Instant::now();
        let pages = pages(&json);
        let took = start.elapsed();
        // The page and every box.
        assert_eq!(labels(&pages[0]).len(), n + 1);
        took
    });
}
