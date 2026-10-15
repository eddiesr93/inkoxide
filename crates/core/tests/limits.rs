//! Memory and stack guards: hostile sizes fail fast with an error,
//! the deepest nesting the parser accepts renders on a small stack.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, PdfALevel, RenderOptions};

/// Counts the heap each thread holds, for tests that bound a render's
/// memory: one-thread renders run on the calling thread.
struct Counting;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

fn grow(n: usize) {
    let _ = LIVE.try_with(|live| {
        let v = live.get() + n;
        live.set(v);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(v)));
    });
}

fn shrink(n: usize) {
    let _ = LIVE.try_with(|live| live.set(live.get().saturating_sub(n)));
}

// SAFETY: every call is passed on to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        shrink(layout.size());
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            shrink(layout.size());
            grow(size);
        }
        q
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// What `f` returns, how long it took, and the most heap this thread
/// held above what it held before.
fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration, usize) {
    let base = LIVE.with(Cell::get);
    PEAK.with(|p| p.set(base));
    let t = Instant::now();
    let out = f();
    let took = t.elapsed();
    (out, took, PEAK.with(Cell::get) - base)
}

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
    // The same, with a last level that paints nothing: no painter call.
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/limits/colr-fanout-empty.ttf"
    ))
    .unwrap();
    f.register("FanoutEmpty", bytes).unwrap();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/EmojiTest-COLR.ttf"
    ))
    .unwrap();
    f.register("EmojiTest-COLR", bytes).unwrap();
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

/// A `wavy` decoration repeated on every page counts its waves.
#[test]
fn wavy_decorations_repeated_on_every_page_are_counted() {
    let fixed = |pages: &str| {
        let text = "x".repeat(100);
        format!(
            r#"{{"type":"text","break":{{"fixed":true}},{pages}
                "runs":[{{"text":"{text}","font":"Inter-Regular","size":10,"letterSpacing":1000,
                "decoration":"underline","decorationStyle":"wavy","decorationThickness":0.1}}]}}"#
        )
    };
    // About 2400 pages.
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 2].join(",");
    let t = std::time::Instant::now();
    let err = render(&doc(&format!("{},{tall}", fixed(""))), Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 64 MB"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
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

/// A `wavy` decoration is drawn only where the page shows it, and with
/// a bounded number of waves per glyph however wide its text is set.
#[test]
fn wavy_decorations_are_drawn_where_the_page_shows_them() {
    let wide = |style: &str| {
        let text = "x".repeat(1000);
        format!(
            r#"{{"type":"text","style":{{"width":500{style}}},"runs":[{{"text":"{text}",
                "font":"Inter-Regular","size":10,"letterSpacing":1000,"decoration":"underline",
                "decorationStyle":"wavy","decorationThickness":0.1}}]}}"#
        )
    };
    for (what, style) in [
        ("a line wider than the page", ""),
        (
            "a line scaled down onto the page",
            r#","transform":[{"scale":[0.0001,0.0001]}]"#,
        ),
        (
            "a line beside the page",
            r#","transform":[{"translate":[-1000000,0]}]"#,
        ),
    ] {
        let page = doc(&[wide(style), wide(style), wide(style)].join(","));
        let (out, took, peak) = measure(|| render(&page, Some(1)));
        let pdf = out.unwrap();
        assert!(pdf.len() < 4 << 20, "{what}: {} KiB", pdf.len() >> 10);
        assert!(peak < 64 << 20, "{what}: {} MiB", peak >> 20);
        assert!(took.as_secs() < 5, "{what}: took {took:?}");
    }
}

/// The text of images in text counts as the file holds it: each image is
/// in a span with its text, an inline image's `alt` or a colour glyph's
/// text, UTF-16 in hex when it is not printable ASCII.
#[test]
fn image_text_repeated_on_every_page_is_counted() {
    let mut images = ImageStore::new();
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;
    images.insert("dot", svg.as_bytes().to_vec());
    let render = |doc: &str| {
        let options = RenderOptions::default().with_threads(1);
        inkoxide::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options,
            &fonts(),
            &images,
        )
    };
    let alt = "é".repeat(1000);
    let image =
        format!(r#"{{"font":"Inter-Regular","size":10,"image":{{"src":"dot","alt":"{alt}"}}}}"#);
    let inline = |pages: &str| {
        let runs = [image.as_str(); 100].join(",");
        format!(r#"{{"type":"text","break":{{"fixed":true}},{pages}"runs":[{runs}]}}"#)
    };
    // About 190 pages.
    let tall = r#"{"type":"box","style":{"height":160000}}"#;
    // A colour glyph's text is its cluster's: here with the marks after it.
    let emoji = |pages: &str| {
        let text = format!("😀{}", "\u{301}".repeat(1000)).repeat(100);
        format!(
            r#"{{"type":"text","break":{{"fixed":true}},{pages}
                "runs":[{{"text":"{text}","font":"EmojiTest-COLR","size":10}}]}}"#
        )
    };
    let fonts = r#"{"id":"Inter-Regular","family":"Inter"},{"id":"EmojiTest-COLR","family":"E"}"#;
    let doc = |children: &str| {
        format!(
            r#"{{"version":1,"fonts":[{fonts}],
               "pages":[{{"size":{{"width":595,"height":842}},"children":[{children}]}}]}}"#
        )
    };
    for (what, every, first_only) in [
        (
            "inline images",
            format!("{},{tall}", inline("")),
            format!("{},{tall}", inline(r#""pages":"first","#)),
        ),
        (
            "colour glyphs",
            format!("{},{tall}", emoji("")),
            format!("{},{tall}", emoji(r#""pages":"first","#)),
        ),
    ] {
        let t = std::time::Instant::now();
        let err = render(&doc(&every)).unwrap_err();
        assert_eq!(err.code(), "LAYOUT_ERROR", "{what}: {err}");
        assert!(
            err.to_string().contains("the limit is 64 MB"),
            "{what}: {err}"
        );
        assert!(t.elapsed().as_secs() < 5, "{what}: took {:?}", t.elapsed());
        render(&doc(&first_only)).unwrap();
    }
}

/// Each colour glyph is in a span with the text of its own cluster: a
/// glyph shaped once with many marks after it does not carry them to its
/// other copies.
#[test]
fn colour_glyphs_carry_their_own_text() {
    let marked = format!("😀{}", "\u{301}".repeat(20_000));
    let copies = "😀 ".repeat(2000);
    let mut images = ImageStore::new();
    let svg_copies = "😀".repeat(2000);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text font-size="10">{marked}{svg_copies}</text></svg>"#
    );
    images.insert("svg", svg.into_bytes());
    let text = |copies: &str| {
        format!(
            r#"{{"type":"text","runs":[{{"text":"{marked} {copies}","font":"EmojiTest-COLR","size":10}}]}}"#
        )
    };
    let image = r#"{"type":"image","src":"svg","style":{"width":100,"height":100}}"#;
    for (what, children) in [
        ("page text", text(&copies)),
        ("SVG text", image.to_string()),
    ] {
        let page = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"EmojiTest-COLR","family":"E"}}],
               "fallbackFonts":["EmojiTest-COLR"],"pages":[{{"size":{{"width":595,"height":842}},"children":[{children}]}}]}}"#
        );
        let options = RenderOptions::default()
            .with_threads(1)
            .with_compress(false);
        let render = || {
            let f = fonts();
            inkoxide::render(page.as_bytes(), InputFormat::Json, &options, &f, &images)
        };
        let (out, took, peak) = measure(render);
        let pdf = out.unwrap();
        assert!(pdf.len() < 4 << 20, "{what}: {} KiB", pdf.len() >> 10);
        assert!(peak < 64 << 20, "{what}: {} MiB", peak >> 20);
        assert!(took.as_secs() < 5, "{what}: took {took:?}");
        if what == "page text" {
            // The copies extract as the emoji alone.
            let own = b"/ActualText <FEFFD83DDE00>";
            let found = pdf.windows(own.len()).filter(|w| w == own).count();
            assert!(found >= 2000, "{found} spans");
        }
    }
}

/// A link's target counts as the file holds it, once for each link.
#[test]
fn link_targets_repeated_on_every_page_are_counted() {
    let target = format!("https://example.com/{}", "x".repeat(2000));
    let linked = |pages: &str| {
        format!(
            r#"{{"type":"box","break":{{"fixed":true}},{pages}"link":"{target}",
                "style":{{"width":10,"height":10}}}}"#
        )
    };
    // About 2400 pages.
    let tall = [r#"{"type":"box","style":{"height":1000000}}"#; 2].join(",");
    let every = vec![linked(""); 20].join(",");
    let t = std::time::Instant::now();
    let err = render(&doc(&format!("{every},{tall}")), Some(1)).unwrap_err();
    assert_eq!(err.code(), "LAYOUT_ERROR", "{err}");
    assert!(err.to_string().contains("the limit is 64 MB"), "{err}");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    let once = vec![linked(r#""pages":"first","#); 20].join(",");
    render(&doc(&format!("{once},{tall}")), Some(1)).unwrap();
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

/// A colour glyph that would take 10^19 paints stops at the work budget,
/// whether its paints call the painter or not: it draws without colour, as
/// its outline, and fast.
#[test]
fn colour_glyphs_that_fan_out_draw_without_colour() {
    for (font, threads) in [
        ("Fanout", Some(1)),
        ("Fanout", Some(4)),
        ("FanoutEmpty", Some(1)),
    ] {
        let d = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"{font}","family":"F"}}],
            "fallbackFonts":["{font}"],
            "pages":[{{"size":{{"width":595,"height":842}},"children":[
              {{"type":"text","runs":[{{"text":"a 😀 b 😀","font":"Inter-Regular","size":10}}]}}]}}]}}"#
        );
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

/// SVG text at scale: thousands of texts on one point, of spans with
/// their own paint, of text nodes split by comments. Each takes time and
/// memory in proportion to its size (the unit tests of `svg_text` and
/// `text/svg.rs` push each step further).
#[test]
fn svg_text_at_scale_renders_quickly() {
    let inter = r#"{"id":"Inter-Regular","family":"Inter"}"#;
    let svg = |body: &str| {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">{body}</svg>"#)
    };
    let same_point = svg(&"<text>a</text>".repeat(5_000));
    let mut spans = String::from(r#"<text text-decoration="underline">"#);
    for i in 0..5_000 {
        let _ = write!(spans, r##"<tspan fill="#{i:06x}" dx="1">a</tspan>"##);
    }
    spans.push_str("</text>");
    let spans = svg(&spans);
    let comments = svg(&format!("<text>{}</text>", "a<!---->".repeat(50_000)));
    let one_b = RenderOptions::default()
        .with_threads(1)
        .with_pdf_a(true)
        .with_pdf_a_level(PdfALevel::A1b);
    for (what, svg, options) in [
        ("texts on one point", &same_point, RenderOptions::default()),
        ("texts on one point, PDF/A-1b", &same_point, one_b),
        (
            "spans with their own paint",
            &spans,
            RenderOptions::default(),
        ),
        (
            "text nodes between comments",
            &comments,
            RenderOptions::default(),
        ),
    ] {
        let (doc, images) = svg_doc(inter, svg);
        let options = options.with_threads(1);
        let render = || {
            let f = fonts();
            inkoxide::render(doc.as_bytes(), InputFormat::Json, &options, &f, &images)
        };
        let (out, took, peak) = measure(render);
        out.unwrap();
        assert!(took.as_secs() < 15, "{what}: took {took:?}");
        assert!(peak < 256 << 20, "{what}: {} MiB", peak >> 20);
    }
    // Text nodes count as elements.
    let comments = svg(&format!("<text>{}</text>", "a<!---->".repeat(300_000)));
    let (out, took, _) = measure(|| render_svg(inter, &comments));
    let err = out.unwrap_err();
    assert!(
        err.to_string().contains("more than 250000 elements"),
        "{err}"
    );
    assert!(took.as_secs() < 10, "took {took:?}");
}

/// The placeholders of SVG text are counted before they are written: a
/// long effect in a clip path is repeated for each paint style.
#[test]
fn svg_text_placeholders_are_bounded_before_they_are_written() {
    let inter = r#"{"id":"Inter-Regular","family":"Inter"}"#;
    let transform = "translate(0 0)".repeat(70_000);
    let mut spans = String::new();
    for i in 0..300 {
        let _ = write!(spans, r##"<tspan fill="#{i:06x}">a</tspan>"##);
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
             <clipPath id="c"><text transform="{transform}">{spans}</text></clipPath>
             <rect width="10" height="10" clip-path="url(#c)"/></svg>"#
    );
    let (out, took, peak) = measure(|| render_svg(inter, &svg));
    let err = out.unwrap_err();
    assert_eq!(err.code(), "INVALID_IMAGE", "{err}");
    assert!(
        err.to_string().contains("its text takes more than"),
        "{err}"
    );
    assert!(took.as_secs() < 10, "took {took:?}");
    assert!(peak < 64 << 20, "{} MiB", peak >> 20);
}

/// What a render converts with its fonts costs the SVG checks no more
/// than what validation checks: probes declare what a style sheet could
/// change, which is work for the CSS parser, on every copy.
#[test]
fn validation_and_render_agree_on_svg_text() {
    let inter = r#"{"id":"Inter-Regular","family":"Inter"}"#;
    let sheet = "path { fill: none; fill-opacity: 1; fill-rule: nonzero; stroke: none; \
        stroke-width: 1; stroke-opacity: 1; stroke-linecap: butt; stroke-linejoin: miter; \
        stroke-miterlimit: 4; stroke-dasharray: none; stroke-dashoffset: 0; paint-order: normal; \
        visibility: visible; color: black; transform: none; opacity: 1; clip-path: none; \
        mask: none; filter: none; display: inline; mix-blend-mode: normal; isolation: auto }";
    // `copies` copies of a text, by `use`s of `use`s.
    let svg = |copies: usize| {
        let mut defs = String::from(r#"<text id="l0">a</text>"#);
        let mut left = copies;
        let mut level = 0;
        while left > 1 {
            let k = left.min(40);
            let refs = format!(r##"<use href="#l{level}"/>"##).repeat(k);
            level += 1;
            let _ = write!(defs, r#"<g id="l{level}">{refs}</g>"#);
            left = left.div_ceil(k);
        }
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><style>{sheet}</style><defs>{defs}</defs><use href="#l{level}"/></svg>"##
        )
    };
    for (copies, valid) in [(1_600, true), (40_000, false)] {
        let (doc, images) = svg_doc(inter, &svg(copies));
        let errors = inkoxide::validate(doc.as_bytes(), InputFormat::Json, &fonts(), &images);
        assert_eq!(errors.is_empty(), valid, "{copies} copies: {errors:?}");
        let rendered = render_svg(inter, &svg(copies));
        assert_eq!(rendered.is_ok(), valid, "{copies} copies: {rendered:?}");
    }
}
