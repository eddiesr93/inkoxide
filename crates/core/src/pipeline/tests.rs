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
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    s.register("Inter-Regular", bytes).unwrap();
    s
}

/// A document of `pages` pages whose footer is `footer` (a node).
fn doc(pages: u32, footer: &str) -> String {
    // A box a little shorter than `pages` pages takes exactly `pages`.
    let flow = f32::from(u16::try_from(pages).unwrap()) * H - 1.0;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"footer":{footer},
            "children":[{{"type":"box","style":{{"height":{flow}}}}}]}}]}}"#
    )
}

/// "Page N of M" in a text node with `style`.
fn page_n_of_m(style: &str) -> String {
    let run = |r: &str| format!(r#"{{{r},"font":"Inter-Regular","size":10}}"#);
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{},{},{},{}]}}"#,
        run(r#""text":"Page ""#),
        run(r#""placeholder":"pageNumber""#),
        run(r#""text":" of ""#),
        run(r#""placeholder":"totalPages""#),
    )
}

/// A footer box with `style` (and the horizontal padding) around `child`.
fn footer(style: &str, child: &str) -> String {
    format!(
        r#"{{"type":"box","style":{{{style}"padding":{{"left":{PAD},"right":{PAD},"bottom":10}}}},
            "children":[{child}]}}"#
    )
}

/// The painted lines of every page: baseline and horizontal extent of the
/// glyphs on it, top to bottom.
fn painted_lines(json: &str) -> Vec<Vec<(f32, f32, f32)>> {
    let store = store();
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, _) = run(&doc, &fonts, &mut text, &images, Ok).unwrap();
    pages
        .iter()
        .map(|page| {
            let mut lines: Vec<(f32, f32, f32)> = Vec::new();
            for op in &page.ops {
                let Op::Text(t) = op else { continue };
                let (Some(first), Some(last)) = (t.glyphs.first(), t.glyphs.last()) else {
                    continue;
                };
                let font = fonts.get(t.font);
                let x0 = t.x + first.x;
                let x1 = t.x + last.x + font.advance(last.id) * font.scale(t.size);
                match lines.iter_mut().find(|l| (l.0 - t.y).abs() < 0.01) {
                    Some(l) => *l = (l.0, l.1.min(x0), l.2.max(x1)),
                    None => lines.push((t.y, x0, x1)),
                }
            }
            lines.sort_by(|a, b| a.0.total_cmp(&b.0));
            lines
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
