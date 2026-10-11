//! Headers and footers with page numbers, through layout, pagination and
//! paint.

use super::*;
use crate::images::{ImageCrateDecoder, ImageStore};
use crate::FontStore;

/// Page size; the flow is one box sliced over the pages.
const W: f32 = 200.0;
const H: f32 = 100.0;
/// Horizontal padding of the footer.
const PAD: f32 = 10.0;

fn store() -> FontStore {
    let mut s = FontStore::new();
    for name in ["Inter-Regular", "NotoSansHebrew-Regular"] {
        let path = format!(
            "{}/../../fixtures/fonts/{name}.ttf",
            env!("CARGO_MANIFEST_DIR")
        );
        s.register(name, std::fs::read(path).unwrap()).unwrap();
    }
    s
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

/// The glyphs painted on every page.
fn painted(json: &str) -> Vec<Vec<Painted>> {
    let store = store();
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, _) = run(&doc, &fonts, &mut text, &images, Ok).unwrap();
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
