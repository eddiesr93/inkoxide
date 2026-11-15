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

#[test]
fn glyphs_far_apart_are_placed_rather_than_adjusted() {
    // Letters 40 pt apart at 1 pt: a `TJ` adjustment of about 40000, past
    // the largest real number of PDF/A-1. Each letter is placed instead.
    let spaced = r#"{"type":"text","style":{"whiteSpace":"nowrap"},"runs":[{"text":"xxx","font":"Inter-Regular","size":1,"letterSpacing":40}]}"#;
    for level in [PdfALevel::A1b, PdfALevel::A2b] {
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
               "pages":[{{"size":{{"width":300,"height":300}},"children":[{spaced}]}}]}}"#
        );
        let options = RenderOptions::default()
            .with_pdf_a_level(level)
            .with_compress(false);
        let pdf = crate::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options,
            &crate::test_fixtures::inter(),
            &ImageStore::new(),
        )
        .unwrap();
        let pdf = String::from_utf8_lossy(&pdf);
        // Three stretches of one glyph, 40 pt and a letter apart.
        let xs: Vec<f32> = pdf
            .split(" Tm\n")
            .filter_map(|s| s.rsplit("1 0 0 -1 ").next())
            .filter_map(|s| s.split(' ').next()?.parse().ok())
            .collect();
        assert_eq!(xs.len(), 3, "{xs:?}");
        assert!(xs[1] - xs[0] > 40.0 && xs[2] - xs[1] > 40.0, "{xs:?}");
        assert_eq!(pdf.matches("] TJ").count(), 3);
    }
    // Closer than the bound, the letters stay one `TJ` with adjustments.
    let near = spaced.replace(r#""letterSpacing":40"#, r#""letterSpacing":20"#);
    let pdf = render(&near, "", PdfALevel::A1b).unwrap();
    assert!(!pdf.is_empty());
}

#[test]
fn pdf_a_1b_splits_a_long_parent_tree() {
    // A key and a value per page: past 8191 entries, so the tree is
    // split into leaves of 4095 pages.
    let page = r#"{"size":{"width":300,"height":300},"children":[]}"#;
    let pages = vec![page; 4200].join(",");
    let doc = format!(r#"{{"version":1,"pages":[{pages}]}}"#);
    let render = |level| {
        let options = RenderOptions::default()
            .with_pdf_a_level(level)
            .with_tagged(true)
            .with_compress(false);
        crate::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options,
            &FontStore::new(),
            &ImageStore::new(),
        )
        .unwrap()
    };
    let pdf = String::from_utf8_lossy(&render(PdfALevel::A1b)).into_owned();
    assert!(pdf.contains("/Limits [0 4094]"), "first leaf");
    assert!(pdf.contains("/Limits [4095 4199]"), "last leaf");
    // Without the limit, one array.
    let pdf = String::from_utf8_lossy(&render(PdfALevel::A2b)).into_owned();
    assert!(!pdf.contains("/Limits"));
}
