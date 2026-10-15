//! Fonts a caller gives only when a render needs them
//! (`RenderOptions::deferred_fonts`): a render that does not need one
//! renders the bytes it would with the font registered, and one that does
//! (a character only it has, a run in it) names it.

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, NotLoaded, RenderOptions};

fn font(id: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/fonts/{id}.ttf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn store(ids: &[&str]) -> FontStore {
    let mut s = FontStore::new();
    for id in ids {
        s.register(*id, font(id)).unwrap();
    }
    s
}

/// A page of `text` in `run_font`, with Noto Sans declared and as the fallback, as
/// `@inkoxide/render` writes documents.
fn doc(text: &str, run_font: &str) -> String {
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"NotoSans-Regular","family":"Noto Sans"}},
            {{"id":"RobotoMono-Regular","family":"Roboto Mono"}}],"fallbackFonts":["NotoSans-Regular"],
          "pages":[{{"size":{{"width":300,"height":300}},"children":[
            {{"type":"text","runs":[{{"text":"{text}","font":"{run_font}","size":12}}]}},
            {{"type":"text","runs":[{{"text":"mono","font":"RobotoMono-Regular","size":12}}]}}]}}]}}"#
    )
}

fn render(doc: &str, fonts: &FontStore, deferred: &[&str]) -> Result<Vec<u8>, InkoxideError> {
    let options = RenderOptions::default()
        .with_creation_date("2026-10-08T00:00:00Z")
        .with_deferred_fonts(deferred.iter().copied());
    inkoxide::render(
        doc.as_bytes(),
        InputFormat::Json,
        &options,
        fonts,
        &ImageStore::new(),
    )
}

const ALL: [&str; 3] = ["Inter-Regular", "NotoSans-Regular", "RobotoMono-Regular"];
const WITHOUT_NOTO: [&str; 2] = ["Inter-Regular", "RobotoMono-Regular"];

#[test]
fn a_fallback_no_character_needs_renders_as_if_registered() {
    let d = doc("Hello, world", "Inter-Regular");
    let expected = render(&d, &store(&ALL), &[]).unwrap();
    let pdf = render(&d, &store(&WITHOUT_NOTO), &["NotoSans-Regular"]).unwrap();
    assert!(pdf == expected, "the PDF differs");
    // Registered after all, it is an ordinary font.
    let registered = render(&d, &store(&ALL), &["NotoSans-Regular"]).unwrap();
    assert!(
        registered == expected,
        "registered, it renders as an ordinary font"
    );
}

#[test]
fn a_character_only_the_fallback_has_needs_it() {
    // Inter has no U+01C4 (Ǆ); Noto Sans has.
    let d = doc("Hello \u{01C4}", "Inter-Regular");
    let err = render(&d, &store(&WITHOUT_NOTO), &["NotoSans-Regular"]).unwrap_err();
    assert_eq!(err.code(), "NOT_LOADED");
    let InkoxideError::NotLoaded(NotLoaded {
        fonts, hyphenation, ..
    }) = &err
    else {
        panic!("{err}");
    };
    assert_eq!(
        (fonts.as_slice(), hyphenation.as_slice()),
        (&["NotoSans-Regular".to_string()][..], &[][..])
    );
    assert!(err.to_string().contains("NotoSans-Regular"), "{err}");
    let expected = render(&d, &store(&ALL), &[]).unwrap();
    let registered = render(&d, &store(&ALL), &["NotoSans-Regular"]).unwrap();
    assert!(
        registered == expected,
        "registered, it renders as an ordinary font"
    );
}

#[test]
fn a_run_in_a_deferred_font_needs_it() {
    let d = doc("Hello", "NotoSans-Regular");
    let err = render(&d, &store(&WITHOUT_NOTO), &["NotoSans-Regular"]).unwrap_err();
    assert_eq!(err.code(), "NOT_LOADED", "{err}");
}

#[test]
fn only_deferred_fonts_may_be_missing() {
    let d = doc("Hello", "Inter-Regular");
    let err = render(&d, &store(&WITHOUT_NOTO), &[]).unwrap_err();
    assert_eq!(err.code(), "FONT_NOT_REGISTERED", "{err}");
    // With no registered font to stand in, every deferred one is needed.
    let err = render(&d, &store(&[]), &ALL).unwrap_err();
    assert_eq!(err.code(), "NOT_LOADED", "{err}");
}
