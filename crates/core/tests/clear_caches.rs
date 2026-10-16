//! `clear_caches` empties the caches renders share (decoded images, font
//! subsets, SVG converted with its text), and renders after it give the
//! same bytes as before.

use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Text in a subset font, a JPEG and an SVG with text: something in each cache.
const DOC: &str = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
  "pages":[{"size":{"width":300,"height":300},"children":[
    {"type":"text","runs":[{"text":"Cached and cleared","font":"Inter-Regular","size":12}]},
    {"type":"image","src":"photo","style":{"width":80,"height":60}},
    {"type":"image","src":"text","style":{"width":120,"height":60}}]}]}"#;

#[test]
fn renders_the_same_after_clearing_the_caches() {
    let mut fonts = FontStore::new();
    fonts
        .register("Inter-Regular", fixture("fonts/Inter-Regular.ttf"))
        .unwrap();
    let mut images = ImageStore::new();
    images.insert("photo", fixture("images/photo.jpg"));
    images.insert("text", fixture("images/text.svg"));
    let options = RenderOptions::default().with_creation_date("2026-10-08T00:00:00Z");
    let render =
        || inkoxide::render(DOC.as_bytes(), InputFormat::Json, &options, &fonts, &images).unwrap();

    let first = render();
    assert_eq!(render(), first, "a render from the caches");
    inkoxide::clear_caches();
    assert_eq!(render(), first, "a render after clearing them");
    // Clearing empty caches is harmless.
    inkoxide::clear_caches();
    inkoxide::clear_caches();
    assert_eq!(render(), first, "a render after clearing them twice");
}
