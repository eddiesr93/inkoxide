//! Memory and stack guards: hostile sizes fail fast with an error,
//! the deepest nesting the parser accepts renders on a small stack.

use std::fmt::Write as _;

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, RenderOptions};

fn fonts() -> FontStore {
    let mut f = FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    f.register("Inter-Regular", bytes).unwrap();
    // Its GSUB substitutes `A` with `AA`, 14 times over: 16 384 glyphs.
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/limits/glyph-doubling.ttf"
    ))
    .unwrap();
    f.register("Doubling", bytes).unwrap();
    // A COLR glyph whose layers paint the next level 255 times, eight
    // levels deep.
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/limits/colr-fanout.ttf"
    ))
    .unwrap();
    f.register("Fanout", bytes).unwrap();
    f
}

fn doc(children: &str) -> String {
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
           "pages":[{{"size":{{"width":595,"height":842}},"children":[{children}]}}]}}"#
    )
}

fn render(doc: &str, threads: Option<usize>) -> Result<Vec<u8>, InkoxideError> {
    let options = RenderOptions::default().with_threads(threads);
    inkoxide::render(
        doc.as_bytes(),
        InputFormat::Json,
        &options,
        &fonts(),
        &ImageStore::new(),
    )
}

#[test]
fn runaway_page_count_is_an_error() {
    let t = std::time::Instant::now();
    // Lengths are bounded (±1000000 points), but enough of them still add
    // up to more pages than allowed: here about 118 000.
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 100].join(",");
    let err = render(&doc(&tall), Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 100000"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

/// What is painted on every page is bounded too: a few kilobytes repeated
/// on each of thousands of pages would make gigabytes.
#[test]
fn runaway_repeated_content_is_an_error() {
    let fixed = |pages: &str| {
        let text = "x".repeat(1000);
        format!(
            r#"{{"type":"text","break":{{"fixed":true}},{pages}
                "runs":[{{"text":"{text}","font":"Inter-Regular","size":10,"link":"https://example.com"}}]}}"#
        )
    };
    // About 2400 pages.
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 2].join(",");
    let t = std::time::Instant::now();
    let err = render(&doc(&format!("{},{tall}", fixed(""))), Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 64 MB"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    // Shown on the first page only, it is painted once.
    let once = doc(&format!("{},{tall}", fixed(r#""pages":"first","#)));
    render(&once, Some(1)).unwrap();
}

/// A note's text counts as the file holds it: anything but printable
/// ASCII makes it UTF-16 in hex, four bytes a character.
#[test]
fn runaway_notes_repeated_on_every_page_are_an_error() {
    let note = |pages: &str| {
        let contents = format!("{}é", "x".repeat(7999));
        format!(
            r#"{{"type":"note","break":{{"fixed":true}},{pages}"note":{{"contents":"{contents}"}}}}"#
        )
    };
    // About 2400 pages of 32 KB each.
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 2].join(",");
    let t = std::time::Instant::now();
    let err = render(&doc(&format!("{},{tall}", note(""))), Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 64 MB"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    let once = doc(&format!("{},{tall}", note(r#""pages":"first","#)));
    render(&once, Some(1)).unwrap();
}

/// A form field taller than a page is sliced across pages like any atomic
/// node, but it is one widget, on the page it starts on: not one per page.
#[test]
fn a_field_taller_than_a_page_is_one_widget() {
    let field = r#"{"type":"field","style":{"width":10,"aspectRatio":0.0001},
        "field":{"name":"f","kind":"select","font":"Inter-Regular","options":["a","b"]}}"#;
    let t = std::time::Instant::now();
    let pdf = render(&doc(field), Some(1)).unwrap();
    let count = |needle: &[u8]| pdf.windows(needle.len()).filter(|w| w == &needle).count();
    assert_eq!(count(b"/Type /Page\n"), 119);
    assert_eq!(count(b"/Widget"), 1);
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

fn doubling(text: &str) -> String {
    format!(r#"{{"type":"text","runs":[{{"text":"{text}","font":"Doubling","size":10}}]}}"#)
}

/// Text that shapes to far more glyphs than it has bytes, through a font's
/// layout tables, stops the render before it takes gigabytes: 400 `A`s
/// shape to 6.5 million glyphs.
#[test]
fn runaway_glyphs_from_shaping_are_an_error() {
    let doc = |paras: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"Doubling","family":"Doubling"}}],
               "pages":[{{"size":{{"width":595,"height":842}},"children":[{paras}]}}]}}"#
        )
    };
    // Enough paragraphs to be shaped in parallel on 4 threads.
    let many = doc(&vec![doubling("A A"); 200].join(","));
    for threads in [Some(1), Some(4)] {
        let t = std::time::Instant::now();
        let err = render(&many, threads).unwrap_err();
        assert_eq!(err.code(), "FONT_ERROR", "{err}");
        assert!(
            err.to_string()
                .contains("font `Doubling`: the text shaped to more than 4194304 glyphs"),
            "{err}"
        );
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    }
    render(&doc(&doubling(&["A"; 16].join(" "))), Some(1)).unwrap();
}

fn svg_doc(declared: &str, svg: &str) -> (String, ImageStore) {
    let mut images = ImageStore::new();
    images.insert("svg", svg.as_bytes().to_vec());
    let doc = format!(
        r#"{{"version":1,"fonts":[{declared}],
           "pages":[{{"size":{{"width":595,"height":842}},"children":[
             {{"type":"image","src":"svg","style":{{"width":100,"height":100}}}}]}}]}}"#
    );
    (doc, images)
}

fn render_svg(declared: &str, svg: &str) -> Result<Vec<u8>, InkoxideError> {
    let (doc, images) = svg_doc(declared, svg);
    let options = RenderOptions::default().with_threads(1);
    inkoxide::render(
        doc.as_bytes(),
        InputFormat::Json,
        &options,
        &fonts(),
        &images,
    )
}

/// SVG text copied by `use` many times over is counted per copy: past a
/// million characters the SVG fails, before anything is laid out or
/// drawn.
#[test]
fn copies_of_svg_text_are_counted() {
    let inter = r#"{"id":"Inter-Regular","family":"Inter"}"#;
    let line = "x".repeat(1000);
    // `levels` levels of ten copies each: 1000 × 10^levels characters.
    let svg = |levels: usize| {
        let mut defs = format!(r#"<text id="l0">{line}</text>"#);
        for i in 1..=levels {
            let uses = format!(r##"<use href="#l{}"/>"##, i - 1).repeat(10);
            let _ = write!(defs, r#"<g id="l{i}">{uses}</g>"#);
        }
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs>{defs}</defs><use href="#l{levels}"/></svg>"##
        )
    };
    let t = std::time::Instant::now();
    let err = render_svg(inter, &svg(10)).unwrap_err();
    assert_eq!(err.code(), "INVALID_IMAGE", "{err}");
    assert!(err.to_string().contains("characters of text"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    // Two hundred copies render.
    render_svg(inter, &svg(2)).unwrap();
}

/// SVG text shapes through the render's shaper, under the same glyph
/// limit as the document's text.
#[test]
fn svg_text_spends_the_glyph_limit() {
    let doubling = r#"{"id":"Doubling","family":"Doubling"}"#;
    let line = ["A"; 300].join(" ");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text font-family="Doubling">{line}</text></svg>"#
    );
    let t = std::time::Instant::now();
    let err = render_svg(doubling, &svg).unwrap_err();
    assert_eq!(err.code(), "FONT_ERROR", "{err}");
    assert!(
        err.to_string().contains("more than 4194304 glyphs"),
        "{err}"
    );
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

/// Glyphs count as what is painted on every page too: one `A` shapes to
/// 16 384 glyphs, on each of about 2400 pages.
#[test]
fn runaway_glyphs_repeated_on_every_page_are_an_error() {
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 2].join(",");
    let d = format!(
        r#"{{"version":1,"fonts":[{{"id":"Doubling","family":"Doubling"}}],
           "pages":[{{"size":{{"width":595,"height":842}},"header":{},"children":[{tall}]}}]}}"#,
        doubling("A")
    );
    let t = std::time::Instant::now();
    let err = render(&d, Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 64 MB"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

/// The parser's recursion limit bounds node depth; everything below it
/// (layout, pagination, paint) must fit a 2 MiB stack at that depth.
#[test]
fn deepest_accepted_nesting_renders_on_a_small_stack() {
    let mut deepest = None;
    for depth in (1..200).rev() {
        let mut node =
            r#"{"type":"text","runs":[{"text":"deep","font":"Inter-Regular","size":10}]}"#
                .to_string();
        for _ in 0..depth {
            node = format!(
                r#"{{"type":"box","style":{{"padding":{{"left":0.1}}}},"children":[{node}]}}"#
            );
        }
        let d = doc(&node);
        let errors = inkoxide::validate(
            d.as_bytes(),
            InputFormat::Json,
            &fonts(),
            &ImageStore::new(),
        );
        if errors.is_empty() {
            deepest = Some(d);
            break;
        }
        assert_eq!(
            errors[0].code.as_str(),
            "PARSE_ERROR",
            "depth {depth}: {errors:?}"
        );
    }
    let d = deepest.expect("some depth parses");
    for threads in [Some(1), Some(2)] {
        let d = d.clone();
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || render(&d, threads).unwrap())
            .unwrap()
            .join()
            .unwrap();
    }
}

/// A colour glyph that would take 10^19 paints stops at the painter's
/// work budget: it draws without colour, as its outline, and fast.
#[test]
fn colour_glyphs_that_fan_out_draw_without_colour() {
    let d = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"},{"id":"Fanout","family":"Fanout"}],
        "fallbackFonts":["Fanout"],
        "pages":[{"size":{"width":595,"height":842},"children":[
          {"type":"text","runs":[{"text":"a \u{1F600} b \u{1F600}","font":"Inter-Regular","size":10}]}]}]}"#
        .replace("\\u{1F600}", "\u{1F600}");
    for threads in [Some(1), Some(4)] {
        let t = std::time::Instant::now();
        let options = RenderOptions::default()
            .with_threads(threads)
            .with_compress(false);
        let pdf = inkoxide::render(
            d.as_bytes(),
            InputFormat::Json,
            &options,
            &fonts(),
            &ImageStore::new(),
        )
        .unwrap();
        let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        assert!(!has(b"/Subtype /Image") && !has(b"/Subtype /Form"));
        assert!(has(b"COLRFanout-Regular"));
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    }
}

/// Nearly the most SVG text allowed, in one text element, renders in
/// seconds (about 3 in a debug build): nothing in its layout grows faster
/// than its length.
#[test]
fn the_largest_svg_text_renders_quickly() {
    let inter = r#"{"id":"Inter-Regular","family":"Inter"}"#;
    let words = "lorem ipsum dolor sit amet ".repeat(36_000);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text x="0" y="10">{words}</text></svg>"#
    );
    let t = std::time::Instant::now();
    render_svg(inter, &svg).unwrap();
    assert!(t.elapsed().as_secs() < 30, "took {:?}", t.elapsed());
}
