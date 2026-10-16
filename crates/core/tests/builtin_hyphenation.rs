//! A build without the `embedded-hyphenation` feature (the WebAssembly
//! engine) gets the built-in patterns a document needs at run time: a render
//! that needs some it has not been given fails, naming them, and renders the
//! same as an embedded build once they are registered.

use inkoxide::{
    FontStore, HyphenationStore, ImageStore, InkoxideError, InputFormat, RenderOptions,
};

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

/// German text in a narrow box, so that long words hyphenate, and an
/// English paragraph without hyphenation. Its own producer keeps the engine
/// version, and so the digest, out of the PDF.
const DOC: &str = r#"{"version":1,"meta":{"lang":"de-AT","producer":"Inkoxide tests"},"fonts":[{"id":"Inter-Regular","family":"Inter"}],
  "pages":[{"size":{"width":200,"height":400},"children":[
    {"type":"text","style":{"width":80,"hyphenate":true},
     "runs":[{"text":"Donaudampfschifffahrtsgesellschaft Wörterbuch","font":"Inter-Regular","size":10}]},
    {"type":"text","style":{"lang":"en"},"runs":[{"text":"No hyphens here","font":"Inter-Regular","size":10}]}]}]}"#;

fn render(store: &HyphenationStore) -> Result<Vec<u8>, InkoxideError> {
    let options = RenderOptions::default()
        .with_creation_date("2026-10-08T00:00:00Z")
        .with_hyphenation(store.clone());
    inkoxide::render(
        DOC.as_bytes(),
        InputFormat::Json,
        &options,
        &fonts(),
        &ImageStore::new(),
    )
}

fn automaton(file: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/hyphenation/{file}.bin",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn files_are_named_by_language() {
    assert_eq!(inkoxide::builtin_hyphenation_file("de-AT"), Some("de"));
    assert_eq!(inkoxide::builtin_hyphenation_file("nb"), Some("no"));
    assert_eq!(inkoxide::builtin_hyphenation_file("EN_gb"), Some("en"));
    assert_eq!(inkoxide::builtin_hyphenation_file("ja"), None);
}

#[test]
fn registering_checks_the_file() {
    let mut store = HyphenationStore::new();
    assert_eq!(
        store
            .register_builtin("xx", &automaton("de"))
            .unwrap_err()
            .code(),
        "HYPHENATION_ERROR"
    );
    assert_eq!(
        store.register_builtin("de", &[1, 2]).unwrap_err().code(),
        "HYPHENATION_ERROR"
    );
    store.register_builtin("de", &automaton("de")).unwrap();
}

#[cfg(not(feature = "embedded-hyphenation"))]
#[test]
fn a_render_names_the_patterns_it_needs() {
    let mut store = HyphenationStore::new();
    let err = render(&store).unwrap_err();
    assert!(
        matches!(&err, InkoxideError::NotLoaded(n) if n.hyphenation == ["de"] && n.fonts.is_empty()),
        "{err}"
    );
    assert_eq!(err.code(), "NOT_LOADED");
    store.register_builtin("de", &automaton("de")).unwrap();
    let pdf = render(&store).unwrap();
    // The PDF an embedded build renders (`builtin_hyphenation.sha256`, which the
    // test below checks against such a build).
    let digest = sha256(&pdf);
    let expected = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/builtin_hyphenation.sha256"
    ))
    .unwrap();
    assert_eq!(digest, expected.trim());
}

#[cfg(feature = "embedded-hyphenation")]
#[test]
fn an_embedded_build_renders_the_same() {
    let pdf = render(&HyphenationStore::new()).unwrap();
    let expected = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/builtin_hyphenation.sha256"
    ))
    .unwrap();
    assert_eq!(sha256(&pdf), expected.trim());
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
