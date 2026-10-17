use crate::{FontStore, ImageStore, InputFormat, PdfALevel, RenderOptions};

/// Renders a 300 × 300 pt page holding `children`, with `meta`, at
/// `level`.
fn render(children: &str, meta: &str, level: PdfALevel) -> Result<Vec<u8>, crate::InkoxideError> {
    let doc = format!(
        r#"{{"version":1,"meta":{{{meta}}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
           "pages":[{{"size":{{"width":300,"height":300}},"children":[{children}]}}]}}"#
    );
    let fonts: FontStore = crate::test_fixtures::inter();
    let options = RenderOptions::default().with_pdf_a_level(level);
    crate::render(
        doc.as_bytes(),
        InputFormat::Json,
        &options,
        &fonts,
        &ImageStore::new(),
    )
}

#[test]
fn pdf_a_1b_fails_past_its_limits() {
    let nested = |n: usize| {
        let open = r#"{"type":"box","style":{"overflow":"hidden"},"children":["#.repeat(n);
        format!("{open}{}", "]}".repeat(n))
    };
    let title = format!(r#""title":"{}""#, "a".repeat(70_000));
    // A word of 4200 letters, each spaced: a `TJ` array of an entry
    // per letter and per adjustment.
    let spaced = format!(
        r#"{{"type":"text","style":{{"whiteSpace":"nowrap"}},"runs":[{{"text":"{}","font":"Inter-Regular","size":2,"letterSpacing":1}}]}}"#,
        "x".repeat(4200)
    );
    let cases = [
        (
            r#"{"type":"box","style":{"width":10,"height":10,"transform":[{"translate":[40000.5,0]}]}}"#.to_string(),
            "",
            "a real number past ±32767",
        ),
        (nested(30), "", "graphics states nested more than 28 deep"),
        (String::new(), title.as_str(), "a string longer than 65535 bytes"),
        (spaced, "", "an array of more than 8191 entries"),
    ];
    for (child, meta, what) in cases {
        let Err(e) = render(&child, meta, PdfALevel::A1b) else {
            panic!("not refused: {what}");
        };
        assert_eq!(e.code(), "PDF_ERROR", "{e}");
        assert!(e.to_string().contains(what), "{e}");
        render(&child, meta, PdfALevel::A2b).unwrap();
    }
    // Within the limits.
    render(&nested(20), "", PdfALevel::A1b).unwrap();
    render(
        r#"{"type":"box","style":{"width":10,"height":10,"transform":[{"translate":[32000.5,0]}]}}"#,
        "",
        PdfALevel::A1b,
    )
    .unwrap();
}
