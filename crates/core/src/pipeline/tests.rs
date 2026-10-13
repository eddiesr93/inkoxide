//! Pages through layout, pagination and paint: headers, footers and flow
//! text with page numbers, overlays, fixed nodes and tables, z-order, and
//! the destinations of ids and bookmarks.

use super::*;
use crate::images::{ImageCrateDecoder, ImageStore};
use crate::FontStore;

/// Page size; the flow is one box sliced over the pages.
const W: f32 = 200.0;
const H: f32 = 100.0;
/// Horizontal padding of the footer.
const PAD: f32 = 10.0;

fn store() -> FontStore {
    crate::test_fixtures::fonts(&["Inter-Regular", "NotoSansHebrew-Regular"])
}

/// A document of `pages` pages whose footer is `footer` (a node).
fn doc(pages: u32, footer: &str) -> String {
    // A box a little shorter than `pages` pages takes exactly `pages`.
    let flow = f32::from(u16::try_from(pages).unwrap()) * H - 1.0;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},
            {{"id":"NotoSansHebrew-Regular","family":"Noto Sans Hebrew"}}],
            "fallbackFonts":["Inter-Regular"],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"footer":{footer},
            "children":[{{"type":"box","style":{{"height":{flow}}}}}]}}]}}"#
    )
}

/// "Page N of M" in a text node with `style`.
fn page_n_of_m(style: &str) -> String {
    n_of_m(style, "Inter-Regular", "", ["Page ", " of "], None)
}

/// `words[0]` N `words[1]` M in a text node with `style`, in `font` with
/// the run properties `extra`: placeholders, or the `written` numbers.
fn n_of_m(
    style: &str,
    font: &str,
    extra: &str,
    words: [&str; 2],
    written: Option<(u32, u32)>,
) -> String {
    let run = |r: &str| format!(r#"{{{r},"font":"{font}","size":10{extra}}}"#);
    let number = |placeholder: &str, n: Option<u32>| match n {
        Some(n) => run(&format!(r#""text":"{n}""#)),
        None => run(&format!(r#""placeholder":"{placeholder}""#)),
    };
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{},{},{},{}]}}"#,
        run(&format!(r#""text":"{}""#, words[0])),
        number("pageNumber", written.map(|w| w.0)),
        run(&format!(r#""text":"{}""#, words[1])),
        number("totalPages", written.map(|w| w.1)),
    )
}

/// A footer box with `style` (and the horizontal padding) around `child`.
fn footer(style: &str, child: &str) -> String {
    format!(
        r#"{{"type":"box","style":{{{style}"padding":{{"left":{PAD},"right":{PAD},"bottom":10}}}},
            "children":[{child}]}}"#
    )
}

/// A glyph painted on a page: baseline, horizontal extent, and the digit
/// it shows, if any.
struct Painted {
    y: f32,
    x0: f32,
    x1: f32,
    digit: Option<char>,
}

/// Parses `json` and runs it through layout, pagination and paint with
/// the fonts of `store` and no images, then runs `f` on the painted pages,
/// the marks and the fonts.
pub(super) fn render_with<R>(
    store: &FontStore,
    json: &str,
    f: impl FnOnce(Vec<PageContent>, &Marks<'_>, &FontSet<'_>) -> R,
) -> R {
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, marks) = run(&doc, &fonts, &mut text, &mut images, true, Ok).unwrap();
    f(pages, &marks, &fonts)
}

/// The glyphs painted on every page.
fn painted(json: &str) -> Vec<Vec<Painted>> {
    render_with(&store(), json, |pages, _, fonts| glyphs(&pages, fonts))
}

/// The glyphs painted on each of `pages`.
fn glyphs(pages: &[PageContent], fonts: &FontSet<'_>) -> Vec<Vec<Painted>> {
    pages
        .iter()
        .map(|page| {
            let mut out = Vec::new();
            for op in &page.ops {
                let Op::Text(t) = op else { continue };
                let font = fonts.get(t.font);
                for g in &t.glyphs {
                    let x0 = t.x + g.x;
                    out.push(Painted {
                        y: t.y,
                        x0,
                        x1: x0 + font.advance(g.id) * font.scale(t.size),
                        digit: ('0'..='9').find(|&c| font.glyph(c) == Some(g.id)),
                    });
                }
            }
            out
        })
        .collect()
}

/// The painted lines of every page: baseline and horizontal extent of the
/// glyphs on it, top to bottom.
fn painted_lines(json: &str) -> Vec<Vec<(f32, f32, f32)>> {
    painted(json)
        .iter()
        .map(|glyphs| {
            let mut lines: Vec<(f32, f32, f32)> = Vec::new();
            for g in glyphs {
                match lines.iter_mut().find(|l| (l.0 - g.y).abs() < 0.01) {
                    Some(l) => *l = (l.0, l.1.min(g.x0), l.2.max(g.x1)),
                    None => lines.push((g.y, g.x0, g.x1)),
                }
            }
            lines.sort_by(|a, b| a.0.total_cmp(&b.0));
            lines
        })
        .collect()
}

/// The numbers painted on every page, as they read from left to right.
fn painted_numbers(json: &str) -> Vec<Vec<String>> {
    painted(json)
        .into_iter()
        .map(|mut glyphs| {
            glyphs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            let mut numbers: Vec<String> = Vec::new();
            let mut in_number = false;
            for g in glyphs {
                match (g.digit, in_number) {
                    (Some(d), true) => numbers.last_mut().unwrap().push(d),
                    (Some(d), false) => numbers.push(d.to_string()),
                    (None, _) => {}
                }
                in_number = g.digit.is_some();
            }
            numbers
        })
        .collect()
}

/// Page counts with one and with four digits.
const COUNTS: [u32; 2] = [7, 1234];

/// Asserts `f` of every painted line of every page, with the page number.
fn each_line(json: &str, pages: u32, f: impl Fn(u32, &[(f32, f32, f32)])) {
    let painted = painted_lines(json);
    assert_eq!(painted.len(), pages as usize);
    for (k, lines) in painted.iter().enumerate() {
        assert!(!lines.is_empty(), "page {}: nothing painted", k + 1);
        f(k as u32 + 1, lines);
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn right_aligned_page_numbers_end_at_the_edge() {
    let right = W - PAD;
    for pages in COUNTS {
        // Pushed right by the footer: the text box is as wide as the text.
        let pushed = footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &page_n_of_m(""),
        );
        // Aligned right inside a full-width text box.
        let aligned = footer("", &page_n_of_m(r#""textAlign":"right""#));
        for f in [pushed, aligned] {
            each_line(&doc(pages, &f), pages, |n, lines| {
                assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
                let (_, _, x1) = lines[0];
                assert!(close(x1, right), "page {n} of {pages} ends at {x1}");
            });
        }
    }
}

#[test]
fn centred_page_numbers_are_centred() {
    for pages in COUNTS {
        let pushed = footer(
            r#""flexDirection":"row","justifyContent":"center","#,
            &page_n_of_m(""),
        );
        let aligned = footer("", &page_n_of_m(r#""textAlign":"center""#));
        for f in [pushed, aligned] {
            each_line(&doc(pages, &f), pages, |n, lines| {
                assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
                let (_, x0, x1) = lines[0];
                let mid = f32::midpoint(x0, x1);
                assert!(close(mid, W / 2.0), "page {n} of {pages} centred at {mid}");
            });
        }
    }
}

/// In a 60 pt box, "Page 9 of 9" fits on one line where the stand-in
/// "Page 999 of 999" would not, and "Page 1000 of" no longer fits where
/// "Page 999 of" would.
#[test]
fn page_numbers_wrap_at_their_own_width() {
    let width = 60.0;
    let right = W - PAD;
    let f = footer(
        r#""flexDirection":"row","justifyContent":"flex-end","#,
        &page_n_of_m(&format!(r#""width":{width},"textAlign":"right""#)),
    );
    each_line(&doc(9, &f), 9, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n} of 9: {lines:?}");
        assert!(close(lines[0].2, right), "page {n}: {lines:?}");
    });
    let pages = 1000;
    each_line(&doc(pages, &f), pages, |n, lines| {
        assert_eq!(lines.len(), 2, "page {n} of {pages}: {lines:?}");
        for &(_, x0, x1) in lines {
            assert!(close(x1, right), "page {n}: {lines:?}");
            assert!(x0 >= right - width - 0.01, "page {n} overflows: {lines:?}");
        }
    });
}

/// The extent of the one line of `footer` on its first page.
fn extent(footer: &str) -> (f32, f32) {
    let lines = &painted_lines(&doc(1, footer))[0];
    assert_eq!(lines.len(), 1, "{lines:?}");
    (lines[0].1, lines[0].2)
}

/// Asserts that `footer(None)` with page numbers lays out on each of
/// `pages` pages like `footer(Some((n, pages)))` with them written out.
fn like_written(pages: u32, footer: &dyn Fn(Option<(u32, u32)>) -> String) {
    each_line(&doc(pages, &footer(None)), pages, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
        let (x0, x1) = extent(&footer(Some((n, pages))));
        assert!(
            close(lines[0].1, x0) && close(lines[0].2, x1),
            "page {n} of {pages}: {lines:?}, written out {x0}..{x1}"
        );
    });
}

/// "עמוד N מתוך M" ("page N of M") in a right-to-left paragraph, on pages
/// whose numbers take one and two digits.
#[test]
fn right_to_left_page_numbers_read_left_to_right() {
    let pages = 12;
    let right = W - PAD;
    let hebrew = |style: &str, written| {
        n_of_m(
            &format!(r#""direction":"rtl"{style}"#),
            "NotoSansHebrew-Regular",
            "",
            [
                "\u{05e2}\u{05de}\u{05d5}\u{05d3} ",
                " \u{05de}\u{05ea}\u{05d5}\u{05da} ",
            ],
            written,
        )
    };
    // At the start of the line, which is its right end; pushed right by
    // the footer, in a box as wide as the text; and centred.
    let aligned = |w| footer("", &hebrew("", w));
    let pushed = |w| {
        footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &hebrew("", w),
        )
    };
    let centred = |w| footer("", &hebrew(r#","textAlign":"center""#, w));
    like_written(pages, &aligned);
    like_written(pages, &pushed);
    like_written(pages, &centred);
    for f in [aligned(None), pushed(None)] {
        each_line(&doc(pages, &f), pages, |n, lines| {
            assert!(close(lines[0].2, right), "page {n}: {lines:?}");
        });
        // The total is on the left, and each number reads left to right
        // ("10", not "01").
        for (k, numbers) in painted_numbers(&doc(pages, &f)).iter().enumerate() {
            assert_eq!(numbers, &["12".to_string(), (k + 1).to_string()]);
        }
    }
}

#[test]
fn letter_spaced_page_numbers_take_their_own_width() {
    let pages = 12;
    let spaced = |style: &str, written| {
        n_of_m(
            style,
            "Inter-Regular",
            r#","letterSpacing":3"#,
            ["Page ", " of "],
            written,
        )
    };
    let aligned = |w| footer("", &spaced(r#""textAlign":"right""#, w));
    let pushed = |w| {
        footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &spaced("", w),
        )
    };
    like_written(pages, &aligned);
    like_written(pages, &pushed);
    for f in [aligned(None), pushed(None)] {
        let ends: Vec<f32> = painted_lines(&doc(pages, &f))
            .iter()
            .map(|lines| lines[0].2)
            .collect();
        assert!(ends.iter().all(|&x| close(x, ends[0])), "{ends:?}");
        for (k, numbers) in painted_numbers(&doc(pages, &f)).iter().enumerate() {
            assert_eq!(numbers, &[(k + 1).to_string(), pages.to_string()]);
        }
    }
}

// Fixed nodes: overlays (fixed and absolutely positioned, at the top of the
// flow) and fixed nodes in the flow.

/// Page margin in the documents with fixed nodes; the content box is
/// `W - 2·M` by `H - 2·M`.
const M: f32 = 10.0;

/// A document of `pages` pages (margins `M`) whose flow is `fixed` (one or
/// more nodes) followed by a box that fills the pages.
fn with_fixed(pages: u32, fixed: &str) -> String {
    let flow = f32::from(u16::try_from(pages).unwrap()) * (H - 2.0 * M) - 1.0;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},
            "margin":{{"top":{M},"right":{M},"bottom":{M},"left":{M}}},
            "children":[{fixed},{{"type":"box","style":{{"height":{flow}}}}}]}}]}}"#
    )
}

/// A red box with the node properties `extra` and the style `style`.
fn red_box(extra: &str, style: &str) -> String {
    format!(r#"{{"type":"box"{extra},"style":{{"backgroundColor":[1,0,0,1],{style}}}}}"#)
}

/// Runs `f` on the painted pages and the marks of a document.
fn render_doc<R>(json: &str, f: impl FnOnce(&[PageContent], &Marks<'_>) -> R) -> R {
    render_with(&store(), json, |pages, marks, _| f(&pages, marks))
}

type Bounds = (f32, f32, f32, f32);

/// The red rectangles painted on every page, as `(x, y, w, h)`.
fn red_boxes(json: &str) -> Vec<Vec<Bounds>> {
    render_doc(json, |pages, _| {
        pages
            .iter()
            .map(|page| {
                let mut out = Vec::new();
                for op in &page.ops {
                    let Op::FillPath { path, color, .. } = op else {
                        continue;
                    };
                    if *color != [1.0, 0.0, 0.0, 1.0] {
                        continue;
                    }
                    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
                    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                    for seg in &path.segs {
                        if let paint::PathSeg::MoveTo(x, y) | paint::PathSeg::LineTo(x, y) = *seg {
                            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                        }
                    }
                    out.push((x0, y0, x1 - x0, y1 - y0));
                }
                out
            })
            .collect()
    })
}

fn close_bounds(a: Bounds, b: Bounds) -> bool {
    close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
}

/// Asserts that the document has `pages` pages and each shows one red box
/// at `want`.
fn on_every_page(json: &str, pages: u32, want: Bounds) {
    let boxes = red_boxes(json);
    assert_eq!(boxes.len(), pages as usize, "{boxes:?}");
    for (k, b) in boxes.iter().enumerate() {
        assert!(
            b.len() == 1 && close_bounds(b[0], want),
            "page {}: {b:?}, want {want:?}",
            k + 1
        );
    }
}

const FIXED: &str = r#","break":{"fixed":true}"#;

#[test]
fn overlays_are_placed_in_the_content_box_of_every_page() {
    let (cw, ch) = (W - 2.0 * M, H - 2.0 * M);
    // From the bottom-right corner of one page's content box, not of the
    // flow (several pages tall).
    let corner = red_box(
        FIXED,
        r#""position":"absolute","inset":{"bottom":5,"right":5},"width":20,"height":10"#,
    );
    on_every_page(
        &with_fixed(3, &corner),
        3,
        (M + cw - 25.0, M + ch - 15.0, 20.0, 10.0),
    );
    // Percentages are of the content box.
    let centre = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":"50%","left":"25%"},"width":"50%","height":"10%""#,
    );
    on_every_page(
        &with_fixed(2, &centre),
        2,
        (M + cw / 4.0, M + ch / 2.0, cw / 2.0, ch / 10.0),
    );
    // Insets on all sides size the box; negative ones reach into the margin.
    let border = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":-5,"right":-5,"bottom":-5,"left":-5}"#,
    );
    on_every_page(
        &with_fixed(4, &border),
        4,
        (M - 5.0, M - 5.0, cw + 10.0, ch + 10.0),
    );
}

#[test]
fn a_fixed_table_repeats_whole() {
    // A fixed table in the flow repeats with its row and cell fills, not
    // only its cells' content.
    let table = r#"{"type":"table","break":{"fixed":true},"columns":[{"width":"1fr"}],"rows":[
        {"backgroundColor":[1,0,0,1],"cells":[{"children":[{"type":"box","style":{"height":10}}]}]},
        {"cells":[{"style":{"backgroundColor":[1,0,0,1]},"children":[{"type":"box","style":{"height":10}}]}]}]}"#;
    // It takes room on every page, so the flow runs onto a fourth.
    let boxes = red_boxes(&with_fixed(3, table));
    assert_eq!(boxes.len(), 4, "{boxes:?}");
    for (k, page) in boxes.iter().enumerate() {
        assert_eq!(page.len(), 2, "page {}: {page:?}", k + 1);
        assert!(
            close_bounds(page[0], boxes[0][0]),
            "page {}: {page:?}",
            k + 1
        );
        assert!(
            close_bounds(page[1], boxes[0][1]),
            "page {}: {page:?}",
            k + 1
        );
    }
}

#[test]
fn collapsed_borders_keep_their_style() {
    // Two cells with 2pt red borders. Every edge between them and around
    // them is drawn once: dashed ones stroked with a dash, solid ones
    // filled. At equal widths a solid border wins over a dashed one.
    let cell = |style: &str| {
        format!(
            r#"{{"style":{{"borderWidth":{{"top":2,"right":2,"bottom":2,"left":2}},
                "borderColor":{{"top":[1,0,0,1],"right":[1,0,0,1],"bottom":[1,0,0,1],"left":[1,0,0,1]}},"borderStyle":"{style}"}},
                "children":[{{"type":"box","style":{{"height":10}}}}]}}"#
        )
    };
    let table = |a: &str, b: &str| {
        format!(
            r#"{{"version":1,"pages":[{{"size":{{"width":200,"height":100}},"children":[
                {{"type":"table","columns":[{{"width":50}},{{"width":50}}],"rows":[{{"cells":[{},{}]}}]}}]}}]}}"#,
            cell(a),
            cell(b)
        )
    };
    let edges = |json: &str| {
        render_doc(json, |pages, _| {
            let (mut dashed, mut solid) = (0, 0);
            for op in &pages[0].ops {
                match op {
                    Op::StrokePath {
                        dash: Some(d),
                        color,
                        width,
                        ..
                    } if *color == [1.0, 0.0, 0.0, 1.0] => {
                        assert!(close(*width, 2.0) && close(d.pattern[0], 6.0), "{op:?}");
                        dashed += 1;
                    }
                    Op::FillPath { color, .. } if *color == [1.0, 0.0, 0.0, 1.0] => solid += 1,
                    _ => {}
                }
            }
            (dashed, solid)
        })
    };
    // Top and bottom of each cell, and three vertical lines.
    assert_eq!(edges(&table("dashed", "dashed")), (7, 0));
    assert_eq!(edges(&table("dashed", "solid")), (3, 4));
    assert_eq!(edges(&table("solid", "dashed")), (3, 4));
}

#[test]
fn a_repeated_header_is_painted_once_per_page() {
    // A table at the top of the flow, over two pages: its header row on
    // each page once, including the first.
    let row = r#"{"cells":[{"children":[{"type":"box","style":{"height":30}}]}]}"#;
    let rows = [row; 12].join(",");
    let table = format!(
        r#"{{"type":"table","headerRows":1,"columns":[{{"width":"1fr"}}],"rows":[
            {{"backgroundColor":[1,0,0,1],"cells":[{{"children":[{{"type":"box","style":{{"height":10}}}}]}}]}},{rows}]}}"#
    );
    for collapse in ["", r#","style":{"borderCollapse":"separate"}"#] {
        let table = table.replacen(
            r#""headerRows":1"#,
            &format!(r#""headerRows":1{collapse}"#),
            1,
        );
        // The pages after the table hold only the box that follows it.
        let boxes = red_boxes(&with_fixed(1, &table));
        let on_table = boxes.iter().take_while(|page| !page.is_empty()).count();
        assert!(on_table >= 2, "{collapse} {boxes:?}");
        for (k, page) in boxes.iter().enumerate() {
            assert!(page.len() <= 1, "{collapse} page {}: {page:?}", k + 1);
        }
    }
}

#[test]
fn overlays_take_no_space() {
    // Below the content box: painted off the page, without adding pages.
    let low = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":1000},"width":20,"height":10"#,
    );
    assert_eq!(red_boxes(&with_fixed(2, &low)).len(), 2);
    // The flow starts at the top of the content box, as without it.
    let flow = with_fixed(
        1,
        &format!(
            "{},{}",
            red_box(FIXED, r#""position":"absolute","width":20,"height":10"#),
            red_box("", r#""height":5"#)
        ),
    );
    let boxes = &red_boxes(&flow)[0];
    assert_eq!(boxes.len(), 2, "{boxes:?}");
    assert!(close(boxes[0].1, M) && close(boxes[1].1, M), "{boxes:?}");
}

#[test]
fn overlays_show_the_page_they_are_on() {
    let pages = 12;
    let numbers = page_n_of_m(r#""position":"absolute","inset":{"bottom":0,"right":0}"#);
    let overlay = numbers.replacen(r#""type":"text""#, &format!(r#""type":"text"{FIXED}"#), 1);
    let json = with_fixed(pages, &overlay);
    for (k, numbers) in painted_numbers(&json).iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), pages.to_string()]);
    }
    // Laid out with each page's digits: it ends at the content box's edge.
    each_line(&json, pages, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n}: {lines:?}");
        assert!(close(lines[0].2, W - M), "page {n}: {lines:?}");
    });
}

#[test]
fn overlays_follow_their_page_filter() {
    let overlay = red_box(
        &format!(r#"{FIXED},"pages":"notFirst""#),
        r#""position":"absolute","width":20,"height":10"#,
    );
    let counts: Vec<usize> = red_boxes(&with_fixed(3, &overlay))
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(counts, [0, 1, 1]);
}

#[test]
fn fixed_nodes_in_the_flow_repeat_where_they_are() {
    // Not positioned: it takes its room once, in the flow (which then needs
    // a fourth page), and is painted at the same place on every page.
    let fixed = red_box(FIXED, r#""height":10"#);
    on_every_page(&with_fixed(3, &fixed), 4, (M, M, W - 2.0 * M, 10.0));
}

/// The colours of the shapes filled on each page, in paint order.
fn fills(json: &str) -> Vec<Vec<f32>> {
    render_doc(json, |pages, _| {
        pages
            .iter()
            .map(|page| {
                page.ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::FillPath { color, .. } => Some(color[0]),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    })
}

#[test]
fn overlays_with_a_negative_z_index_are_painted_under_the_flow() {
    // Boxes told apart by their red component.
    let shade = |red: f32, extra: &str, style: &str| {
        format!(r#"{{"type":"box"{extra},"style":{{"backgroundColor":[{red},0,0,1],{style}}}}}"#)
    };
    let overlay = |red: f32, z: &str| {
        shade(
            red,
            FIXED,
            &format!(r#""position":"absolute","width":20,"height":10{z}"#),
        )
    };
    let children = [
        overlay(0.1, r#","zIndex":-1"#),
        overlay(0.2, r#","zIndex":2"#),
        shade(0.5, "", r#""height":10"#),
        overlay(0.3, ""),
        overlay(0.4, r#","zIndex":-2"#),
        overlay(0.6, r#","zIndex":-1"#),
    ];
    let pages = fills(&with_fixed(1, &children.join(",")));
    // By `zIndex` then document order: the negative ones before the flow
    // (0.5, on the first page), the others after it.
    assert_eq!(
        pages,
        [
            vec![0.4, 0.1, 0.6, 0.5, 0.3, 0.2],
            vec![0.4, 0.1, 0.6, 0.3, 0.2]
        ]
    );
    // Under the flow, an overlay still shows the page it is on.
    let numbers =
        page_n_of_m(r#""position":"absolute","inset":{"bottom":0,"right":0},"zIndex":-1"#);
    let overlay = numbers.replacen(r#""type":"text""#, &format!(r#""type":"text"{FIXED}"#), 1);
    for (k, numbers) in painted_numbers(&with_fixed(3, &overlay)).iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), "3".to_owned()]);
    }
}

#[test]
fn ids_and_bookmarks_in_overlays_point_at_the_first_page() {
    let overlay = red_box(
        &format!(r#"{FIXED},"id":"stamp","bookmark":{{"title":"Stamp"}}"#),
        r#""position":"absolute","inset":{"bottom":0,"left":0},"width":20,"height":10"#,
    );
    let heading = red_box(r#","bookmark":{"title":"Body"}"#, r#""height":5"#);
    let after = red_box(r#","bookmark":{"title":"After"}"#, r#""height":5"#);
    let json = with_fixed(2, &format!("{heading},{overlay},{after}"));
    render_doc(&json, |pages, marks| {
        assert_eq!(pages.len(), 3);
        // Once each, in document order.
        let titles: Vec<&str> = marks.outline.iter().map(|e| e.title).collect();
        assert_eq!(titles, ["Body", "Stamp", "After"]);
        // PDF coordinates: y up from the bottom of the page.
        let want = Dest {
            page: 0,
            x: M,
            y: M + 10.0,
        };
        assert_eq!(marks.outline[1].dest, want);
        assert_eq!(marks.anchors["stamp"], want);
    });
}

// Page numbers in the flow.

/// A document (no margins) whose flow fills `before` pages with a box,
/// then shows `child` at the top of the next.
fn in_flow(before: u32, child: &str) -> String {
    let filler = f32::from(u16::try_from(before).unwrap()) * H;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},
            "children":[{{"type":"box","style":{{"height":{filler}}}}},{child}]}}]}}"#
    )
}

#[test]
fn flow_page_numbers_take_their_own_width() {
    let right = W - PAD;
    // Pushed right in a row: the text box is as wide as the text.
    let pushed = |written| {
        footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &n_of_m("", "Inter-Regular", "", ["Page ", " of "], written),
        )
    };
    for pages in COUNTS {
        let last = pages as usize - 1;
        let lines = painted_lines(&in_flow(pages - 1, &pushed(None)));
        assert_eq!(lines.len(), pages as usize);
        let [line] = lines[last][..] else {
            panic!("page {pages}: {:?}", lines[last]);
        };
        assert!(close(line.2, right), "{pages} pages: ends at {}", line.2);
        // Laid out as if the numbers were written out.
        let written = painted_lines(&in_flow(pages - 1, &pushed(Some((pages, pages)))));
        let [(_, x0, x1)] = written[last][..] else {
            panic!("{:?}", written[last]);
        };
        assert!(
            close(line.1, x0) && close(line.2, x1),
            "{line:?}, written out {x0}..{x1}"
        );
    }
}

#[test]
fn page_numbers_can_bring_flow_text_back_a_page() {
    // 13 pt are left at the foot of the page. "Page 999 of 999" takes two
    // lines in 70 pt and moves on to a second page; "Page 1 of 1" takes
    // one and fits.
    let text = page_n_of_m(r#""width":70"#);
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},
            "children":[{{"type":"box","style":{{"height":{}}}}},{text}]}}]}}"#,
        H - 13.0
    );
    assert_eq!(painted_numbers(&json), [["1", "1"]]);
    let stand_in = n_of_m(
        r#""width":70"#,
        "Inter-Regular",
        "",
        ["Page ", " of "],
        Some((999, 999)),
    );
    assert_eq!(painted(&json.replace(&text, &stand_in)).len(), 2);
}

#[test]
fn repeated_flow_text_shows_each_page_s_numbers() {
    let numbers = page_n_of_m("");
    // A fixed node in the flow is painted on every page.
    let fixed = numbers.replacen(r#""type":"text""#, &format!(r#""type":"text"{FIXED}"#), 1);
    let pages = painted_numbers(&with_fixed(3, &fixed));
    assert!(pages.len() >= 3, "{pages:?}");
    let total = pages.len().to_string();
    for (k, numbers) in pages.iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), total.clone()]);
    }
    // So is a table's header row, on every page the table is on.
    let row = r#"{"cells":[{"children":[{"type":"box","style":{"height":30}}]}]}"#;
    let rows = [row; 8].join(",");
    let table = format!(
        r#"{{"type":"table","headerRows":1,"columns":[{{"width":"1fr"}}],"rows":[
            {{"cells":[{{"children":[{numbers}]}}]}},{rows}]}}"#
    );
    let pages = painted_numbers(&in_flow(0, &table));
    assert!(pages.len() >= 3, "{pages:?}");
    let total = pages.len().to_string();
    for (k, numbers) in pages.iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), total.clone()]);
    }
}

/// The ops of each page of a document whose store holds image `logo`.
fn ops_with_logo(json: &str) -> Vec<Vec<Op>> {
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let store = store();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut images = ImageStore::new();
    images.insert("logo", crate::test_fixtures::fixture("images/logo.png"));
    let mut images = ImageSet::load(&doc.pages, &images, &ImageCrateDecoder).unwrap();
    let shaper = RustybuzzShaper::new(&fonts)
        .unwrap()
        .with_objects(std::sync::Arc::new(images.inline_objects()));
    let mut text = TextCache::new(shaper, None);
    let (pages, _) = run(&doc, &fonts, &mut text, &mut images, false, Ok).unwrap();
    pages.into_iter().map(|p| p.ops).collect()
}

#[test]
fn a_background_image_fills_the_padding_box_under_the_borders() {
    // 150 pt high on 100 pt pages: split over two.
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}},"children":[
            {{"type":"box","style":{{"height":150,"padding":{{"top":5,"left":5}},
              "backgroundColor":[0,1,0,1],"borderWidth":{{"top":2,"right":2,"bottom":2,"left":2}},
              "borderColor":{{"top":[0,0,1,1],"right":[0,0,1,1],"bottom":[0,0,1,1],"left":[0,0,1,1]}},
              "borderRadius":10}},
             "backgroundImage":{{"src":"logo","fit":"cover"}},
             "children":[{{"type":"box","style":{{"height":10,"backgroundColor":[1,0,0,1]}}}}]}}]}}]}}"#
    );
    let pages = ops_with_logo(&json);
    assert_eq!(pages.len(), 2);
    for (k, ops) in pages.iter().enumerate() {
        // Background colour, then the image clipped to the rounded padding
        // box, then the border ring, then the child.
        let fill = |c: [f32; 4]| {
            ops.iter()
                .position(|op| matches!(op, Op::FillPath { color, .. } if *color == c))
                .unwrap_or(usize::MAX)
        };
        let image = ops
            .iter()
            .position(|op| matches!(op, Op::Image { .. }))
            .expect("an image op");
        assert!(fill([0.0, 1.0, 0.0, 1.0]) < image, "page {k}: {ops:#?}");
        assert!(image < fill([0.0, 0.0, 1.0, 1.0]), "page {k}");
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, Op::Image { .. }))
                .count(),
            1
        );
        let Op::ClipPath { path, .. } = &ops[image - 1] else {
            panic!("page {k}: no clip before the image: {:#?}", ops[image - 1]);
        };
        assert!(matches!(ops[image - 2], Op::Save) && matches!(ops[image + 1], Op::Restore));
        // The padding box: inside the 2 pt border, the whole box shifted up
        // by a page on the second.
        let top = 2.0 - k as f32 * H;
        let ys = path.segs.iter().filter_map(|s| match *s {
            paint::PathSeg::MoveTo(_, y) | paint::PathSeg::LineTo(_, y) => Some(y),
            _ => None,
        });
        let (y0, y1) = ys.fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), y| {
            (a.min(y), b.max(y))
        });
        assert!(
            close(y0, top) && close(y1, top + 146.0),
            "page {k}: {y0} {y1}"
        );
        assert!(path
            .segs
            .iter()
            .any(|s| matches!(s, paint::PathSeg::CubicTo(..))));
        // The child, on the first page only, over the image.
        let child = fill([1.0, 0.0, 0.0, 1.0]);
        if k == 0 {
            assert!(image < child && child < usize::MAX);
        } else {
            assert_eq!(child, usize::MAX);
        }
    }
}

#[test]
fn a_box_without_a_background_image_paints_no_image() {
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":{W},"height":{H}}},"children":[
            {{"type":"box","style":{{"height":50,"backgroundColor":[0,1,0,1]}}}}]}}]}}"#
    );
    let pages = ops_with_logo(&json);
    assert!(!pages[0].iter().any(|op| matches!(op, Op::Image { .. })));
}

/// The ops of each page of `text` in Inter at 20 pt, with the colour test
/// font `fallback` as fallback.
fn emoji_ops(text: &str, fallback: &str) -> (Vec<Vec<Op>>, Vec<String>) {
    let mut store = crate::test_fixtures::fonts(&["Inter-Regular"]);
    for id in ["EmojiTest-COLR", "EmojiTest-CBDT"] {
        let bytes = crate::test_fixtures::fixture(&format!("fonts/{id}.ttf"));
        store.register(id, bytes).unwrap();
    }
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},
            {{"id":"EmojiTest-COLR","family":"Emoji"}},{{"id":"EmojiTest-CBDT","family":"Emoji Bitmap"}}],
            "fallbackFonts":["{fallback}"],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"children":[
            {{"type":"text","runs":[{{"text":"{text}","font":"Inter-Regular","size":20}}]}}]}}]}}"#
    );
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let mut images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, _) = run(&doc, &fonts, &mut text, &mut images, false, Ok).unwrap();
    let ids = (0..fonts.fonts.len())
        .map(|k| fonts.get(k as u16).id.to_string())
        .collect();
    (pages.into_iter().map(|p| p.ops).collect(), ids)
}

#[test]
fn colour_glyphs_are_images_in_spans_with_their_text() {
    for fallback in ["EmojiTest-COLR", "EmojiTest-CBDT"] {
        let (pages, ids) = emoji_ops("a \u{1F600}\u{1F1EB}\u{1F1F7} b", fallback);
        let ops = &pages[0];
        // Each colour glyph: a span with its text, the image, and an
        // invisible space of Inter as wide as the glyph's advance (22 pt:
        // 1100 units of a 1000-unit em at 20 pt).
        let spans: Vec<&str> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Span { actual_text } => Some(&**actual_text),
                _ => None,
            })
            .collect();
        assert_eq!(spans, ["\u{1F600}", "\u{1F1EB}\u{1F1F7}"], "{fallback}");
        let images = ops
            .iter()
            .filter(|op| matches!(op, Op::Image { .. }))
            .count();
        assert_eq!(images, 2, "{fallback}");
        for op in ops {
            if let Op::InvisibleText(g) = op {
                assert_eq!(ids[g.font as usize], "Inter-Regular");
                assert!(close(g.width, 22.0), "{}", g.width);
            }
        }
        let at = |want: fn(&Op) -> bool| ops.iter().position(want).unwrap();
        let (span, image) = (
            at(|op| matches!(op, Op::Span { .. })),
            at(|op| matches!(op, Op::Image { .. })),
        );
        let (space, end) = (
            at(|op| matches!(op, Op::InvisibleText(_))),
            at(|op| matches!(op, Op::EndSpan)),
        );
        assert!(span < image && image < space && space < end, "{ops:#?}");
        // The image fills the glyph's box: 1000 units from x 50 (COLR) or
        // 46.875 (CBDT), at 20 pt.
        let Op::Image { matrix, .. } = &ops[image] else {
            unreachable!()
        };
        assert!(close(matrix[0], 20.0), "{fallback}: {matrix:?}");
        // The letters stay text in Inter; the colour font draws no text
        // but the spaces next to the emoji.
        for op in ops {
            if let Op::Text(t) = op {
                if ids[t.font as usize] != "Inter-Regular" {
                    assert_eq!(fallback, "EmojiTest-COLR");
                    assert_eq!(t.glyphs.len(), 1, "{t:?}");
                }
            }
        }
    }
}

#[test]
fn text_without_colour_fonts_paints_no_spans() {
    let (pages, _) = emoji_ops("plain text", "EmojiTest-COLR");
    assert!(!pages[0].iter().any(|op| matches!(
        op,
        Op::Span { .. } | Op::Image { .. } | Op::InvisibleText(_)
    )));
}

#[test]
fn superscripts_and_subscripts_are_painted_off_the_baseline() {
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"children":[
              {{"type":"text","runs":[
                {{"text":"x","font":"Inter-Regular","size":10}},
                {{"text":"2","font":"Inter-Regular","size":6,"verticalAlign":"super","decoration":"underline"}},
                {{"text":"y","font":"Inter-Regular","size":10}},
                {{"text":"3","font":"Inter-Regular","size":6,"verticalAlign":"sub"}}]}}]}}]}}"#
    );
    let pages = render_with(&store(), &json, |pages, _, _| pages);
    let ys: Vec<f32> = pages[0]
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Text(t) => Some(t.y),
            _ => None,
        })
        .collect();
    assert_eq!(ys.len(), 4, "{ys:?}");
    // Inter's OS/2 offsets for a size of 1229 units: 717 up, 154 down.
    let (up, down) = (6.0 * 717.0 / 1229.0, 6.0 * 154.0 / 1229.0);
    assert_eq!(ys[0], ys[2]);
    assert!((ys[0] - ys[1] - up).abs() < 1e-3, "{ys:?}");
    assert!((ys[3] - ys[0] - down).abs() < 1e-3, "{ys:?}");
    // The superscript's underline is drawn under its own baseline.
    let underline = pages[0]
        .ops
        .iter()
        .find_map(|op| match op {
            Op::FillRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!(underline.y > ys[1] && underline.y < ys[0], "{underline:?}");
}

#[test]
fn justified_pretty_lines_are_painted_within_their_box() {
    let prose = "In olden times when wishing still helped one, there lived a king whose \
        daughters were all beautiful, but the youngest was so beautiful that the sun itself, \
        which has seen so much, was astonished whenever it shone in her face.";
    let json = |wrap: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":400}},"margin":{{"left":20,"right":20}},"children":[
              {{"type":"text","style":{{"textAlign":"justify","textWrap":"{wrap}"}},
                "runs":[{{"text":"{prose}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
        )
    };
    for wrap in ["wrap", "pretty"] {
        let lines = painted_lines(&json(wrap));
        assert!(lines[0].len() > 4, "{wrap}");
        let n = lines[0].len();
        for (k, &(_, x0, x1)) in lines[0].iter().enumerate() {
            assert!(x0 >= 20.0 - 1e-3, "{wrap} {k}: {x0}");
            // Justified lines but the last end at the right edge, the
            // spaces of some shrunk to get there.
            if k + 1 < n {
                assert!((x1 - (W - 20.0)).abs() < 0.5, "{wrap} {k}: {x1}");
            } else {
                assert!(x1 <= W - 20.0, "{wrap} {k}: {x1}");
            }
        }
    }
    assert_ne!(painted_lines(&json("wrap")), painted_lines(&json("pretty")));
}

#[test]
fn inline_images_are_painted_in_their_line_with_their_alternative_text() {
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"children":[
            {{"type":"text","style":{{"lineHeight":1}},"runs":[
              {{"text":"ab","font":"Inter-Regular","size":10}},
              {{"font":"Inter-Regular","size":10,"image":{{"src":"logo","height":16,"alt":"Logo"}}}},
              {{"font":"Inter-Regular","size":10,"image":{{"src":"logo","width":4}},"verticalAlign":"top"}}]}}]}}]}}"#
    );
    let pages = ops_with_logo(&json);
    let ops = &pages[0];
    let spans: Vec<&str> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Span { actual_text } => Some(&**actual_text),
            _ => None,
        })
        .collect();
    // Only the image with alternative text has a span.
    assert_eq!(spans, ["Logo"]);
    let images: Vec<[f32; 6]> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Image { matrix, .. } => Some(*matrix),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 2);
    // [w 0 0 -h x y+h]: 16 pt square on the baseline (16 pt down, as it
    // is the tallest), then a 4 pt square at the top of the line.
    let (first, second) = (images[0], images[1]);
    assert!(close(first[0], 16.0) && close(-first[3], 16.0), "{first:?}");
    assert!(close(first[5] + first[3], 0.0), "{first:?}");
    assert!(
        close(second[0], 4.0) && close(second[5] + second[3], 0.0),
        "{second:?}"
    );
    // The second follows the first.
    assert!(close(second[4], first[4] + 16.0), "{first:?} {second:?}");
    // The letters are text.
    assert!(ops
        .iter()
        .any(|op| matches!(op, Op::Text(t) if t.glyphs.len() == 2)));
}
