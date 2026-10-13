//! Memory and stack guards: hostile sizes fail fast with an error,
//! the deepest nesting the parser accepts renders on a small stack.

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, RenderOptions};

fn fonts() -> FontStore {
    let mut f = FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    f.register("Inter-Regular", bytes).unwrap();
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
