use crate::{validate, FontStore, ImageStore, InputFormat, ValidationCode as C, ValidationError};

use crate::test_fixtures::inter as fonts;

fn run(doc: &str) -> Vec<ValidationError> {
    let mut images = ImageStore::new();
    images.insert(
        "logo",
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/images/logo.png"
        ))
        .unwrap(),
    );
    validate(doc.as_bytes(), InputFormat::Json, &fonts(), &images)
}

/// Wraps `children` into a minimal valid document.
fn doc(children: &str) -> String {
    format!(
        r#"{{"version":1,
            "fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":595,"height":842}},"children":[{children}]}}]}}"#
    )
}

fn one(doc: &str) -> (C, String) {
    let errs = run(doc);
    assert_eq!(errs.len(), 1, "expected exactly one error, got {errs:#?}");
    (errs[0].code, errs[0].path.clone())
}

#[test]
fn valid_document_has_no_errors() {
    let d = doc(
        r#"{"type":"box","style":{"width":"50%","padding":{"top":4,"left":"10%"},"margin":{"left":"auto"},
             "backgroundColor":[1,0,0,1],"gridTemplateColumns":[100,"1fr","auto"],"gridColumn":{"start":1,"span":2}},
            "children":[
              {"type":"text","style":{"textAlign":"justify","lineHeight":1.4},"runs":[
                 {"text":"Hello","font":"Inter-Regular","size":11},
                 {"placeholder":"pageNumber","font":"Inter-Regular","size":11}]},
              {"type":"image","src":"logo","fit":"contain"},
              {"type":"spacer","style":{"height":10}},
              {"type":"pageBreak"},
              {"type":"table","columns":[{"width":100},{"width":"1fr"},{"width":"auto"}],"headerRows":0,"rows":[
                 {"cells":[{"colSpan":2},{"rowSpan":2}]},
                 {"cells":[{},{"style":{"verticalAlign":"middle"}}]}]}
            ]}"#,
    );
    assert_eq!(run(&d), vec![]);
}

#[test]
fn parse_error() {
    assert_eq!(one("{\"version\":1,"), (C::ParseError, String::new()));
    assert_eq!(
        one(r#"{"version":1,"pages":[{"size":{"width":1,"height":1}}]} trailing"#),
        (C::ParseError, String::new())
    );
}

#[test]
fn unsupported_version_wins_over_structure() {
    assert_eq!(
        one(r#"{"version":2,"pages":[{"sizes":1}]}"#),
        (C::UnsupportedVersion, "version".into())
    );
    assert_eq!(
        one(r#"{"version":2,"pages":[{"size":{"width":1,"height":1}}]}"#),
        (C::UnsupportedVersion, "version".into())
    );
}

#[test]
fn unknown_and_missing_fields_have_paths() {
    let (code, path) = one(&doc(r#"{"type":"box","style":{"colour":[0,0,0,1]}}"#));
    assert_eq!(code, C::UnknownField);
    assert_eq!(path, "pages[0].children[0].style.colour");

    let (code, path) = one(&doc(r#"{"type":"box","runs":[]}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownField, "pages[0].children[0].runs")
    );

    let (code, path) = one(&doc(r#"{"type":"text"}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::MissingField, "pages[0].children[0]")
    );

    let (code, path) = one(r#"{"version":1,"pages":[{}]}"#);
    assert_eq!((code, path.as_str()), (C::MissingField, "pages[0].size"));
}

#[test]
fn invalid_values() {
    let (code, path) = one(&doc(r#"{"type":"box","style":{"width":"50px"}}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidValue, "pages[0].children[0].style.width")
    );

    let (code, _) = one(&doc(r#"{"type":"box","style":{"display":"inline"}}"#));
    assert_eq!(code, C::InvalidValue);

    let (code, _) = one(&doc(
        r#"{"type":"box","style":{"backgroundColor":[1,0,0]}}"#,
    ));
    assert_eq!(code, C::InvalidValue);

    let (code, path) = one(&doc(r#"{"type":"box","style":{"opacity":"high"}}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidType, "pages[0].children[0].style.opacity")
    );

    let (code, path) = one(&doc(r#"{"type":"wat"}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidValue, "pages[0].children[0].type")
    );

    // A PDF without pages is not a valid PDF.
    assert_eq!(
        one(r#"{"version":1,"pages":[]}"#),
        (C::InvalidValue, "pages".into())
    );
}

#[test]
fn semantic_ranges() {
    let (code, path) = one(&doc(
        r#"{"type":"box","style":{"backgroundColor":[1,0,2,1]}}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (C::OutOfRange, "pages[0].children[0].style.backgroundColor")
    );

    let (code, path) = one(&doc(r#"{"type":"box","style":{"padding":{"left":-1}}}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::OutOfRange, "pages[0].children[0].style.padding.left")
    );

    let (code, path) = one(
        r#"{"version":1,"pages":[{"size":{"width":100,"height":100},"margin":{"left":60,"right":60}}]}"#,
    );
    assert_eq!((code, path.as_str()), (C::OutOfRange, "pages[0].margin"));
}

#[test]
fn fonts_must_be_declared_and_registered() {
    let (code, path) = one(&doc(
        r#"{"type":"text","runs":[{"text":"x","font":"Nope","size":10}]}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownFont, "pages[0].children[0].runs[0].font")
    );

    let d = r#"{"version":1,"fonts":[{"id":"Ghost","family":"G"}],"fallbackFonts":["Missing"],
               "pages":[{"size":{"width":10,"height":10}}]}"#;
    let codes: Vec<_> = run(d).into_iter().map(|e| (e.code, e.path)).collect();
    assert_eq!(
        codes,
        vec![
            (C::FontNotRegistered, "fonts[0].id".to_string()),
            (C::UnknownFont, "fallbackFonts[0]".to_string())
        ]
    );
}

#[test]
fn runs_need_text_xor_placeholder() {
    let (code, _) = one(&doc(
        r#"{"type":"text","runs":[{"font":"Inter-Regular","size":10}]}"#,
    ));
    assert_eq!(code, C::MissingField);
    let (code, _) = one(&doc(
        r#"{"type":"text","runs":[{"text":"1","placeholder":"pageNumber","font":"Inter-Regular","size":10}]}"#,
    ));
    assert_eq!(code, C::InvalidValue);
}

#[test]
fn style_applicability() {
    let (code, path) = one(&doc(r#"{"type":"box","style":{"lineHeight":1.2}}"#));
    assert_eq!(
        (code, path.as_str()),
        (
            C::StyleNotApplicable,
            "pages[0].children[0].style.lineHeight"
        )
    );
    let (code, _) = one(&doc(r#"{"type":"box","style":{"verticalAlign":"top"}}"#));
    assert_eq!(code, C::StyleNotApplicable);
}

#[test]
fn images_must_exist() {
    let (code, path) = one(&doc(r#"{"type":"image","src":"nope"}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownImage, "pages[0].children[0].src")
    );
    let (code, _) = one(&doc(r#"{"type":"image","src":"logo","data":"AAAA"}"#));
    assert_eq!(code, C::InvalidValue);
}

#[test]
fn table_structure() {
    let t = |rows: &str| {
        doc(&format!(
            r#"{{"type":"table","columns":[{{"width":10}},{{"width":"1fr"}}],"rows":[{rows}]}}"#
        ))
    };
    assert_eq!(one(&t(r#"{"cells":[{}]}"#)).0, C::InvalidTable);
    assert_eq!(one(&t(r#"{"cells":[{},{},{}]}"#)).0, C::InvalidTable);
    assert_eq!(
        one(&t(r#"{"cells":[{"rowSpan":2},{}]}"#)).0,
        C::InvalidTable
    );
    assert_eq!(
        run(&t(r#"{"cells":[{"rowSpan":2},{}]},{"cells":[{}]}"#)),
        vec![]
    );
    let (code, path) = one(&doc(
        r#"{"type":"table","columns":[{"width":"2px"}],"rows":[]}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidValue, "pages[0].children[0].columns[0].width")
    );
}

#[test]
fn page_breaks_only_in_flow() {
    let d = r#"{"version":1,"pages":[{"size":{"width":100,"height":100},"header":{"type":"pageBreak"}}]}"#;
    assert_eq!(one(d), (C::InvalidNode, "pages[0].header".into()));
}

#[test]
fn pagination_controls_only_apply_in_the_flow() {
    let page = |fixed: &str, children: &str| {
        format!(
            r#"{{"version":1,"pages":[{{"size":{{"width":100,"height":100}}{fixed},"children":[{children}]}}]}}"#
        )
    };
    let brk = r#""break":{"inside":"avoid"}"#;
    let deep = format!(r#"{{"type":"box","children":[{{"type":"box",{brk}}}]}}"#);
    let cell = |child: &str| {
        format!(
            r#"{{"type":"table","columns":[{{"width":10}}],"rows":[{{"cells":[{{"children":[{child}]}}]}}]}}"#
        )
    };
    for (d, path) in [
        (
            page(&format!(r#","header":{{"type":"box",{brk}}}"#), ""),
            "pages[0].header.break",
        ),
        (
            page(&format!(r#","footer":{deep}"#), ""),
            "pages[0].footer.children[0].break",
        ),
        (
            page("", &cell(&deep)),
            "pages[0].children[0].rows[0].cells[0].children[0].children[0].break",
        ),
    ] {
        assert_eq!(one(&d), (C::InvalidValue, path.into()), "{d}");
    }
    // The defaults are not pagination controls.
    let defaults = r#"{"type":"box","break":{"before":"auto","inside":"auto","fixed":false}}"#;
    assert_eq!(run(&page("", &cell(defaults))), vec![]);
}

#[test]
fn pagination_controls_do_not_apply_in_overlays() {
    // An overlay: fixed and absolutely positioned, directly in the page.
    let overlay = |brk: &str, children: &str| {
        doc(&format!(
            r#"{{"type":"box","break":{{"fixed":true{brk}}},"style":{{"position":"absolute"}},
                "children":[{children}]}}"#
        ))
    };
    assert_eq!(run(&overlay("", r#"{"type":"box"}"#)), vec![]);
    for brk in [
        r#","before":"page""#,
        r#","after":"page""#,
        r#","inside":"avoid""#,
        r#","minPresenceAhead":20"#,
    ] {
        let (code, path) = one(&overlay(brk, ""));
        assert_eq!(
            (code, path.as_str()),
            (C::InvalidValue, "pages[0].children[0].break"),
            "{brk}"
        );
    }
    // Inside it, like inside a header.
    let inside = "pages[0].children[0].children[0]";
    for (child, at) in [
        (
            r#"{"type":"box","break":{"before":"page"}}"#,
            inside.to_owned(),
        ),
        (
            r#"{"type":"box","break":{"inside":"avoid"}}"#,
            inside.to_owned(),
        ),
        (
            r#"{"type":"box","break":{"fixed":true}}"#,
            inside.to_owned(),
        ),
        (
            r#"{"type":"box","children":[{"type":"box","break":{"minPresenceAhead":20}}]}"#,
            format!("{inside}.children[0]"),
        ),
    ] {
        let (code, path) = one(&overlay("", child));
        assert_eq!(
            (code, path),
            (C::InvalidValue, format!("{at}.break")),
            "{child}"
        );
    }
    let (code, path) = one(&overlay("", r#"{"type":"pageBreak"}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidNode, "pages[0].children[0].children[0]")
    );
    // A fixed node in the flow, or an absolute one that is not fixed, keeps
    // its pagination controls.
    for flowing in [
        r#"{"type":"box","break":{"fixed":true,"before":"page"}}"#,
        r#"{"type":"box","break":{"before":"page"},"style":{"position":"absolute"},
            "children":[{"type":"pageBreak"}]}"#,
        r#"{"type":"box","children":[{"type":"box","break":{"fixed":true,"inside":"avoid"},
            "style":{"position":"absolute"}}]}"#,
    ] {
        assert_eq!(run(&doc(flowing)), vec![], "{flowing}");
    }
}

#[test]
fn render_refuses_invalid_documents() {
    let err = crate::render(
        doc(r#"{"type":"box","style":{"lineHeight":1}}"#).as_bytes(),
        InputFormat::Json,
        &crate::RenderOptions::default(),
        &fonts(),
        &ImageStore::new(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "STYLE_NOT_APPLICABLE");
}

#[test]
fn rejects_corrupt_and_unknown_images() {
    let doc = r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},
      "children":[{"type":"image","src":"nope"},{"type":"image","data":"!!"},
      {"type":"image","data":"R0lGODlh"},
      {"type":"image","data":"PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnIHdpZHRoPSc5JyBoZWlnaHQ9JzknPjx0ZXh0Png8L3RleHQ+PC9zdmc+"}]}]}"#;
    let e = validate(
        doc.as_bytes(),
        InputFormat::Json,
        &FontStore::new(),
        &ImageStore::new(),
    );
    let got: Vec<(&str, &str)> = e
        .iter()
        .map(|e| (e.code.as_str(), e.path.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("UNKNOWN_IMAGE", "pages[0].children[0].src"),
            ("INVALID_IMAGE", "pages[0].children[1].data"),
            ("INVALID_IMAGE", "pages[0].children[2].data"),
            ("INVALID_IMAGE", "pages[0].children[3].data"),
        ]
    );
    assert!(e[3].message.contains("<text>"), "{}", e[3].message);
}

#[test]
fn images_over_the_pixel_limit_are_too_large() {
    // A PNG header claiming 100000×100000 pixels: the probe reads no further.
    let mut huge = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    huge.extend_from_slice(&100_000u32.to_be_bytes());
    huge.extend_from_slice(&100_000u32.to_be_bytes());
    let mut images = ImageStore::new();
    images.insert("huge", huge);
    let doc = r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},
      "children":[{"type":"image","src":"huge"},{"type":"image","data":"iVBORw0KGgoAAAANSUhEUgABhqAAAYag"}]}]}"#;
    let e = validate(
        doc.as_bytes(),
        InputFormat::Json,
        &FontStore::new(),
        &images,
    );
    let got: Vec<(&str, &str)> = e
        .iter()
        .map(|e| (e.code.as_str(), e.path.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("IMAGE_TOO_LARGE", "pages[0].children[0].src"),
            ("IMAGE_TOO_LARGE", "pages[0].children[1].data"),
        ]
    );
    assert!(e[0].message.contains("megapixels"), "{}", e[0].message);
}

/// A PNG with a valid header and an `IDAT` that is not zlib data.
const CORRUPT_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVRnYXJiYWdlLW5vdC16bGlinTtYuwAAAABJRU5ErkJggg==";

#[test]
fn images_that_do_not_decode_are_invalid() {
    let fixture = crate::test_fixtures::fixture;
    let photo = fixture("images/photo.jpg");
    let sos = photo.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    // Cut 20 bytes into the image data and padded with junk.
    let mut cut_jpeg = photo[..sos + 20].to_vec();
    cut_jpeg.extend_from_slice(&[0x5A; 64]);
    // Lossless coding: the frame marker made SOF3.
    let mut lossless = photo.clone();
    let sof = lossless.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    lossless[sof + 1] = 0xC3;
    let mut images = ImageStore::new();
    images.insert(
        "corrupt",
        crate::images::base64::decode(CORRUPT_PNG).unwrap(),
    );
    images.insert("cut-png", fixture("images/logo.png")[..60].to_vec());
    images.insert("cut-jpeg", cut_jpeg);
    images.insert("lossless", lossless);
    images.insert("photo", photo);
    let doc = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":100,"height":100}},"children":[
          {{"type":"image","src":"corrupt"}},{{"type":"image","data":"{CORRUPT_PNG}"}},
          {{"type":"image","src":"cut-png"}},{{"type":"image","src":"cut-jpeg"}},
          {{"type":"image","src":"lossless"}},{{"type":"image","src":"photo"}}]}}]}}"#
    );
    let e = validate(
        doc.as_bytes(),
        InputFormat::Json,
        &FontStore::new(),
        &images,
    );
    let got: Vec<(&str, &str)> = e
        .iter()
        .map(|e| (e.code.as_str(), e.path.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("INVALID_IMAGE", "pages[0].children[0].src"),
            ("INVALID_IMAGE", "pages[0].children[1].data"),
            ("INVALID_IMAGE", "pages[0].children[2].src"),
            ("INVALID_IMAGE", "pages[0].children[3].src"),
            ("INVALID_IMAGE", "pages[0].children[4].src"),
        ]
    );
    assert!(
        e[0].message
            .starts_with("image `corrupt`: PNG could not be decoded"),
        "{}",
        e[0].message
    );
    assert!(e[3].message.contains("truncated"), "{}", e[3].message);
    assert!(e[4].message.contains("lossless"), "{}", e[4].message);

    // Render reports the same problem, at its path.
    let err = crate::render(
        doc.as_bytes(),
        InputFormat::Json,
        &crate::RenderOptions::default(),
        &FontStore::new(),
        &images,
    )
    .unwrap_err();
    assert_eq!(
        (
            err.code(),
            err.to_string().contains("pages[0].children[0].src")
        ),
        ("INVALID_IMAGE", true)
    );
}

#[test]
fn validate_without_images_checks_everything_else() {
    // More image references than the error cap, then an undeclared font.
    let images = vec![r#"{"type":"image","src":"logo"}"#; 100].join(",");
    let d = doc(&format!(
        r#"{images},{{"type":"image","data":"!!"}},
        {{"type":"text","runs":[{{"text":"x","font":"nope","size":9}}]}}"#
    ));
    let e: Vec<_> = crate::validate_without_images(d.as_bytes(), InputFormat::Json, &fonts())
        .into_iter()
        .map(|e| (e.code, e.path))
        .collect();
    assert_eq!(
        e,
        [
            (C::InvalidImage, "pages[0].children[100].data".into()),
            (C::UnknownFont, "pages[0].children[101].runs[0].font".into()),
        ]
    );
}

#[test]
fn links_and_bookmarks() {
    let ok = doc(
        r##"{"type":"text","id":"a","bookmark":{"title":"A"},"runs":[{"text":"x","font":"Inter-Regular","size":9}]},
            {"type":"box","link":"#a","bookmark":{"title":"B","level":3}},
            {"type":"text","runs":[{"text":"y","font":"Inter-Regular","size":9,"link":"mailto:x@example.com"}]},
            {"type":"image","src":"logo","link":"https://example.com/ä b"}"##,
    );
    assert_eq!(run(&ok), vec![]);

    let t = r#"{"type":"text","id":"t","runs":[{"text":"x","font":"Inter-Regular","size":9}]}"#;
    for (child, code, path) in [
        (
            r##"{"type":"box","link":"#nope"}"##,
            C::UnknownAnchor,
            "pages[0].children[1].link",
        ),
        (
            r#"{"type":"box","link":"example.com"}"#,
            C::InvalidValue,
            "pages[0].children[1].link",
        ),
        (
            r#"{"type":"box","link":"JavaScript:alert(1)"}"#,
            C::InvalidValue,
            "pages[0].children[1].link",
        ),
        (
            r#"{"type":"box","link":""}"#,
            C::OutOfRange,
            "pages[0].children[1].link",
        ),
        (
            r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":9,"link":"1http:x"}]}"#,
            C::InvalidValue,
            "pages[0].children[1].runs[0].link",
        ),
        (
            r#"{"type":"box","bookmark":{"title":""}}"#,
            C::OutOfRange,
            "pages[0].children[1].bookmark.title",
        ),
        (
            r#"{"type":"box","bookmark":{"title":"x","level":0}}"#,
            C::OutOfRange,
            "pages[0].children[1].bookmark.level",
        ),
        (
            r#"{"type":"box","bookmark":{"level":1}}"#,
            C::MissingField,
            "pages[0].children[1].bookmark.title",
        ),
        (
            r#"{"type":"pageBreak","link":"https://x"}"#,
            C::UnknownField,
            "pages[0].children[1].link",
        ),
        // An id under `display: none` is not a destination.
        (
            r##"{"type":"box","style":{"display":"none"},"children":[{"type":"box","id":"h"}]},{"type":"box","link":"#h"}"##,
            C::UnknownAnchor,
            "pages[0].children[2].link",
        ),
    ] {
        let d = doc(&format!("{t},{child}"));
        assert_eq!(one(&d), (code, path.to_string()), "{child}");
    }

    // Ids in headers are not destinations; bookmarks are not allowed there.
    let d = r##"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},
        "header":{"type":"box","id":"h","bookmark":{"title":"H"}},
        "children":[{"type":"box","link":"#h"}]}]}"##;
    let errs: Vec<_> = run(d).into_iter().map(|e| (e.code, e.path)).collect();
    assert_eq!(
        errs,
        vec![
            (C::InvalidValue, "pages[0].header.bookmark".to_string()),
            (C::UnknownAnchor, "pages[0].children[0].link".to_string()),
        ]
    );
}

#[test]
fn border_collapse() {
    let cell = r#"{"children":[]}"#;
    let ok = doc(&format!(
        r#"{{"type":"table","style":{{"borderCollapse":"separate","borderSpacing":3}},"columns":[{{"width":"1fr"}}],"rows":[{{"cells":[{cell}]}}]}}"#
    ));
    assert_eq!(run(&ok), vec![]);

    for (child, code, path) in [
        (
            r#"{"type":"box","style":{"borderCollapse":"separate"}}"#,
            C::StyleNotApplicable,
            "pages[0].children[0].style.borderCollapse",
        ),
        (
            r#"{"type":"box","style":{"borderSpacing":2}}"#,
            C::StyleNotApplicable,
            "pages[0].children[0].style.borderSpacing",
        ),
        (
            r#"{"type":"table","style":{"borderSpacing":2},"columns":[{"width":"1fr"}],"rows":[{"cells":[{"children":[]}]}]}"#,
            C::InvalidValue,
            "pages[0].children[0].style.borderSpacing",
        ),
        (
            r#"{"type":"table","style":{"borderCollapse":"separate","borderSpacing":-1},"columns":[{"width":"1fr"}],"rows":[{"cells":[{"children":[]}]}]}"#,
            C::OutOfRange,
            "pages[0].children[0].style.borderSpacing",
        ),
        (
            r#"{"type":"table","columns":[{"width":"1fr"}],"rows":[{"cells":[{"style":{"borderCollapse":"separate"},"children":[]}]}]}"#,
            C::StyleNotApplicable,
            "pages[0].children[0].rows[0].cells[0].style.borderCollapse",
        ),
    ] {
        assert_eq!(one(&doc(child)), (code, path.to_string()), "{child}");
    }
}

#[test]
fn grid_parity() {
    let ok = doc(r#"{"type":"box","style":{"display":"grid",
            "gridTemplateColumns":[{"repeat":"auto-fill","tracks":[{"min":60,"max":"1fr"}]}],
            "gridTemplateRows":[{"fitContent":40},{"repeat":2,"tracks":["auto",{"min":"min-content","max":"max-content"}]}],
            "gridAutoRows":[20,{"min":10,"max":"auto"}],
            "gridAutoColumns":["1fr"],
            "gridTemplateAreas":["head head","side main",". main"],
            "justifyItems":"center"},
          "children":[{"type":"box","style":{"gridArea":"main","justifySelf":"end"}},{"type":"spacer","style":{"gridArea":"head"}}]}"#);
    assert_eq!(run(&ok), vec![]);

    let p = "pages[0].children[0].style";
    for (style, code, path) in [
        (
            r#""gridTemplateColumns":[{"min":"1fr","max":10}]"#,
            C::InvalidValue,
            "gridTemplateColumns[0]",
        ),
        (
            r#""gridTemplateColumns":[{"min":10}]"#,
            C::InvalidValue,
            "gridTemplateColumns[0]",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":0,"tracks":[10]}]"#,
            C::OutOfRange,
            "gridTemplateColumns[0].repeat",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":2,"tracks":[]}]"#,
            C::InvalidValue,
            "gridTemplateColumns[0]",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":2,"tracks":[10],"min":5}]"#,
            C::InvalidValue,
            "gridTemplateColumns[0]",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":"auto-fit","tracks":[10]},{"repeat":"auto-fill","tracks":[10]}]"#,
            C::InvalidValue,
            "gridTemplateColumns[1].repeat",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":"auto-fill","tracks":[0,"0%"]}]"#,
            C::InvalidValue,
            "gridTemplateColumns[0].tracks",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":"auto-fill","tracks":["auto"]}]"#,
            C::InvalidValue,
            "gridTemplateColumns",
        ),
        (
            r#""gridTemplateColumns":[{"repeat":"auto-fill","tracks":[10]},"1fr"]"#,
            C::InvalidValue,
            "gridTemplateColumns",
        ),
        (
            r#""gridTemplateRows":[{"min":-1,"max":5}]"#,
            C::OutOfRange,
            "gridTemplateRows[0].min",
        ),
        (
            r#""gridAutoRows":[{"repeat":2,"tracks":[10]}]"#,
            C::UnknownField,
            "gridAutoRows[0].repeat",
        ),
        (
            r#""gridAutoRows":[{"fitContent":-3}]"#,
            C::OutOfRange,
            "gridAutoRows[0].fitContent",
        ),
        (
            r#""gridTemplateAreas":["a b","b b"]"#,
            C::InvalidValue,
            "gridTemplateAreas",
        ),
        (r#""gridArea":"9x""#, C::InvalidValue, "gridArea"),
        (
            r#""gridArea":"a","gridRow":{"start":1}"#,
            C::InvalidValue,
            "gridArea",
        ),
        // Not in the parent's areas (the page root has none).
        (r#""gridArea":"a""#, C::InvalidValue, "gridArea"),
    ] {
        let d = doc(&format!(r#"{{"type":"box","style":{{{style}}}}}"#));
        assert_eq!(one(&d), (code, format!("{p}.{path}")), "{style}");
    }
}

#[test]
fn transforms_radii_border_styles_and_z_index() {
    let ok = doc(
        r#"{"type":"box","style":{"borderRadius":{"topLeft":4,"bottomRight":8},
             "borderStyle":{"top":"dashed","left":"dotted"},"zIndex":-3,
             "transform":[{"translate":["50%",4]},{"scale":[2,1]},{"rotate":10},{"skew":[10,0]},{"matrix":[1,0,0,1,0,0]}],
             "transformOrigin":[0,"100%"]}}"#,
    );
    assert_eq!(run(&ok), vec![]);
    let cases = [
        (
            r#"{"type":"box","style":{"borderRadius":{"topLeft":-1}}}"#,
            C::OutOfRange,
            "pages[0].children[0].style.borderRadius.topLeft",
        ),
        (
            r#"{"type":"box","style":{"transform":[{"skew":[90,0]}]}}"#,
            C::OutOfRange,
            "pages[0].children[0].style.transform[0].skew[0]",
        ),
        (
            r#"{"type":"table","columns":[{"width":10}],"rows":[{"cells":[{"style":{"zIndex":1}}]}]}"#,
            C::StyleNotApplicable,
            "pages[0].children[0].rows[0].cells[0].style.zIndex",
        ),
    ];
    for (node, code, path) in cases {
        assert_eq!(one(&doc(node)), (code, path.to_string()), "{node}");
    }
    let (code, _) = one(&doc(r#"{"type":"box","style":{"borderStyle":"wavy"}}"#));
    assert_eq!(code, C::InvalidValue);
    let (code, _) = one(&doc(r#"{"type":"box","style":{"transform":[{"spin":1}]}}"#));
    assert!(
        matches!(code, C::InvalidValue | C::UnknownField),
        "{code:?}"
    );
}

#[test]
fn widows_orphans_and_page_filters() {
    let ok = doc(
        r#"{"type":"text","pages":"notFirst","style":{"widows":2,"orphans":3},"runs":[{"text":"x","font":"Inter-Regular","size":10}]},
           {"type":"box","pages":[1,-1]}"#,
    );
    assert_eq!(run(&ok), vec![]);
    let cases = [
        (
            r#"{"type":"text","style":{"widows":0},"runs":[{"text":"x","font":"Inter-Regular","size":10}]}"#,
            C::OutOfRange,
            "pages[0].children[0].style.widows",
        ),
        (
            r#"{"type":"box","style":{"orphans":2}}"#,
            C::StyleNotApplicable,
            "pages[0].children[0].style.orphans",
        ),
        (
            r#"{"type":"box","pages":[0]}"#,
            C::OutOfRange,
            "pages[0].children[0].pages",
        ),
    ];
    for (node, code, path) in cases {
        assert_eq!(one(&doc(node)), (code, path.to_string()), "{node}");
    }
    let (code, _) = one(&doc(r#"{"type":"box","pages":"sometimes"}"#));
    assert_eq!(code, C::InvalidValue);
}

#[test]
fn form_fields() {
    let ok = doc(
        r#"{"type":"field","field":{"name":"a","kind":"text","font":"Inter-Regular","value":"x","maxLength":3}},
           {"type":"field","field":{"name":"r","kind":"radio","value":"1","checked":true}},
           {"type":"field","field":{"name":"r","kind":"radio","value":"2"}},
           {"type":"field","field":{"name":"s","kind":"select","font":"Inter-Regular","options":["x","y"],"value":"y"}},
           {"type":"field","field":{"name":"c","kind":"checkbox","checked":true}}"#,
    );
    assert_eq!(run(&ok), vec![]);
    let f = "pages[0].children[0].field";
    let cases = [
        (
            r#"{"type":"field","field":{"name":"a.b","kind":"checkbox"}}"#,
            C::InvalidValue,
            "name",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"text"}}"#,
            C::MissingField,
            "font",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"text","font":"Nope"}}"#,
            C::UnknownFont,
            "font",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"radio"}}"#,
            C::MissingField,
            "value",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"select","font":"Inter-Regular","options":["x"],"value":"z"}}"#,
            C::InvalidValue,
            "value",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"checkbox","multiline":true}}"#,
            C::InvalidValue,
            "multiline",
        ),
        (
            r#"{"type":"field","field":{"name":"a","kind":"text","font":"Inter-Regular","value":"long","maxLength":2}}"#,
            C::OutOfRange,
            "value",
        ),
    ];
    for (node, code, field) in cases {
        assert_eq!(one(&doc(node)), (code, format!("{f}.{field}")), "{node}");
    }
    let dup = doc(
        r#"{"type":"field","field":{"name":"a","kind":"checkbox"}},{"type":"field","field":{"name":"a","kind":"checkbox"}}"#,
    );
    assert_eq!(
        one(&dup),
        (
            C::DuplicateId,
            "pages[0].children[1].field.name".to_string()
        )
    );
    let (code, path) = one(&doc(
        r#"{"type":"field","link":"https://x.ro","field":{"name":"a","kind":"checkbox"}}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownField, "pages[0].children[0].link")
    );
}

#[test]
fn decoration_thickness_and_colour_are_checked() {
    let (code, path) = one(&doc(
        r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":10,"decorationThickness":0}]}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (
            C::OutOfRange,
            "pages[0].children[0].runs[0].decorationThickness"
        )
    );
    let (code, path) = one(&doc(
        r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":10,"decorationColor":[2,0,0,1]}]}"#,
    ));
    assert_eq!(
        (code, path.as_str()),
        (
            C::OutOfRange,
            "pages[0].children[0].runs[0].decorationColor"
        )
    );
}

#[test]
fn image_object_position_is_checked() {
    let image = |position: &str| {
        doc(&format!(
            r#"{{"type":"image","src":"logo","objectPosition":{position}}}"#
        ))
    };
    for ok in [
        "{}",
        r#"{"x":"left","y":"top"}"#,
        r#"{"x":"center","y":"center"}"#,
        r#"{"x":"right","y":"bottom"}"#,
        r#"{"x":"25%","y":"-10.5%"}"#,
        r#"{"x":10,"y":-4.5}"#,
        r#"{"y":"bottom"}"#,
    ] {
        assert_eq!(run(&image(ok)), vec![], "{ok}");
    }
    for (bad, field) in [
        (r#"{"x":"top"}"#, "x"),
        (r#"{"y":"left"}"#, "y"),
        (r#"{"x":"middle"}"#, "x"),
        (r#"{"x":"10pt"}"#, "x"),
        (r#"{"y":"+5%"}"#, "y"),
    ] {
        let (code, path) = one(&image(bad));
        let want = format!("pages[0].children[0].objectPosition.{field}");
        assert_eq!(
            (code, path.as_str()),
            (C::InvalidValue, want.as_str()),
            "{bad}"
        );
    }
    let (code, path) = one(&image(r#"{"x":"left","z":0}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownField, "pages[0].children[0].objectPosition.z")
    );
    let (code, path) = one(&image(r#"{"x":true}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidType, "pages[0].children[0].objectPosition.x")
    );
    let (code, path) = one(&doc(r#"{"type":"box","objectPosition":{"x":"left"}}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownField, "pages[0].children[0].objectPosition")
    );
}

#[test]
fn image_object_position_is_bounded() {
    let image = |position: &str| {
        doc(&format!(
            r#"{{"type":"image","src":"logo","objectPosition":{position}}}"#
        ))
    };
    // Points within ±14400 (the largest page edge), percentages within
    // ±10000 %, bounds included.
    for ok in [
        r#"{"x":14400,"y":-14400}"#,
        r#"{"x":-14400,"y":14400}"#,
        r#"{"x":"10000%","y":"-10000%"}"#,
        r#"{"x":"-10000%","y":"10000%"}"#,
        r#"{"x":"9999.99%","y":0}"#,
    ] {
        assert_eq!(run(&image(ok)), vec![], "{ok}");
    }
    for (bad, field) in [
        (r#"{"x":14400.5}"#, "x"),
        (r#"{"y":-20000}"#, "y"),
        (r#"{"x":1e30}"#, "x"),
        (r#"{"x":"10000.5%"}"#, "x"),
        (r#"{"y":"-99999%"}"#, "y"),
        (r#"{"x":"9999999999999999999999999999999999999%"}"#, "x"),
        (r#"{"y":"9999999999999999999999999999999999999%"}"#, "y"),
    ] {
        let (code, path) = one(&image(bad));
        let want = format!("pages[0].children[0].objectPosition.{field}");
        assert_eq!(
            (code, path.as_str()),
            (C::OutOfRange, want.as_str()),
            "{bad}"
        );
    }
    // Both axes are reported.
    let errs = run(&image(r#"{"x":"-20000%","y":20000}"#));
    let paths: Vec<_> = errs.iter().map(|e| (e.code, e.path.as_str())).collect();
    assert_eq!(
        paths,
        [
            (C::OutOfRange, "pages[0].children[0].objectPosition.x"),
            (C::OutOfRange, "pages[0].children[0].objectPosition.y"),
        ]
    );
}

#[test]
fn debug_is_a_boolean_on_nodes_with_a_box() {
    let ok = doc(r#"{"type":"box","debug":true,"children":[
            {"type":"text","debug":true,"runs":[{"text":"x","font":"Inter-Regular","size":10}]},
            {"type":"image","debug":false,"src":"logo"},
            {"type":"spacer","debug":true},
            {"type":"field","debug":true,"field":{"name":"a","kind":"checkbox"}},
            {"type":"table","debug":true,"columns":[{"width":10}],
             "rows":[{"debug":true,"cells":[{"debug":true}]}]}]}"#);
    assert_eq!(run(&ok), []);
    let (code, path) = one(&doc(r#"{"type":"pageBreak","debug":true}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::UnknownField, "pages[0].children[0].debug")
    );
    let (code, path) = one(&doc(r#"{"type":"box","debug":"yes"}"#));
    assert_eq!(
        (code, path.as_str()),
        (C::InvalidType, "pages[0].children[0].debug")
    );
}

/// Far past the ±10000 % bound: resolved, it overflowed f32.
const HUGE_PERCENT: &str = r#""9999999999999999999999999999999999999%""#;

#[test]
fn lengths_and_numbers_at_their_bounds_are_valid() {
    let box_ = |style: &str| format!(r#"{{"type":"box","style":{{{style}}}}}"#);
    let nodes = [
        box_(r#""width":"10000%","height":1000000,"minWidth":"10000%","maxHeight":1000000,"flexBasis":"10000%""#),
        box_(r#""margin":{"top":-1000000,"left":"-10000%","right":"10000%"},"padding":{"top":1000000,"left":"10000%"}"#),
        box_(r#""position":"absolute","inset":{"top":"-10000%","left":-1000000,"bottom":1000000}"#),
        box_(r#""gap":"10000%","rowGap":1000000,"flexGrow":1000000,"flexShrink":1000000,"aspectRatio":0.000001"#),
        box_(r#""borderWidth":{"top":1000000},"borderRadius":{"topLeft":1000000},"aspectRatio":1000000"#),
        box_(r#""display":"grid","gridTemplateColumns":[1000000,"10000%","1000000fr",{"min":"10000%","max":1000000},{"fitContent":"10000%"}],"gridAutoRows":[{"fitContent":1000000}]"#),
        box_(r#""rotate":-1000000,"transformOrigin":["-10000%",1000000],"transform":[{"translate":["10000%",-1000000]},{"scale":[1000000,-1000000]},{"rotate":1000000},{"skew":[89.9,-89.9]},{"matrix":[1000000,-1000000,1000000,-1000000,1000000,-1000000]}]"#),
        r#"{"type":"table","style":{"borderCollapse":"separate","borderSpacing":1000000},"columns":[{"width":1000000},{"width":"1000000fr"}],
            "rows":[{"minHeight":1000000,"cells":[{},{}]}]}"#
            .into(),
        r#"{"type":"box","break":{"minPresenceAhead":1000000}}"#.into(),
    ];
    assert_eq!(run(&doc(&nodes.join(","))), vec![]);
}

#[test]
fn lengths_and_numbers_are_bounded() {
    for (node, path) in [
        // Percentages, past ±10000 % or far past it.
        (format!(r#"{{"type":"box","style":{{"width":{HUGE_PERCENT}}}}}"#), "style.width"),
        (r#"{"type":"box","style":{"height":"10000.5%"}}"#.into(), "style.height"),
        (r#"{"type":"box","style":{"maxWidth":"20000%"}}"#.into(), "style.maxWidth"),
        (format!(r#"{{"type":"box","style":{{"flexBasis":{HUGE_PERCENT}}}}}"#), "style.flexBasis"),
        (format!(r#"{{"type":"box","style":{{"margin":{{"left":{HUGE_PERCENT}}}}}}}"#), "style.margin.left"),
        (r#"{"type":"box","style":{"margin":{"top":"-10001%"}}}"#.into(), "style.margin.top"),
        (r#"{"type":"box","style":{"padding":{"right":"10001%"}}}"#.into(), "style.padding.right"),
        (r#"{"type":"box","style":{"position":"absolute","inset":{"bottom":"-10001%"}}}"#.into(), "style.inset.bottom"),
        (r#"{"type":"box","style":{"columnGap":"10001%"}}"#.into(), "style.columnGap"),
        (format!(r#"{{"type":"box","style":{{"transform":[{{"translate":[0,{HUGE_PERCENT}]}}]}}}}"#), "style.transform[0].translate[1]"),
        (r#"{"type":"box","style":{"transformOrigin":["-10001%",0]}}"#.into(), "style.transformOrigin[0]"),
        (r#"{"type":"box","style":{"display":"grid","gridTemplateColumns":[10,{"repeat":2,"tracks":[10,"10001%"]}]}}"#.into(), "style.gridTemplateColumns[1].tracks[1]"),
        (r#"{"type":"box","style":{"display":"grid","gridAutoColumns":[{"fitContent":"10001%"}]}}"#.into(), "style.gridAutoColumns[0].fitContent"),
        // Points past ±1000000, including numbers that are infinite as f32.
        (r#"{"type":"box","style":{"width":1000000.5}}"#.into(), "style.width"),
        (r#"{"type":"box","style":{"minHeight":1e30}}"#.into(), "style.minHeight"),
        (r#"{"type":"box","style":{"margin":{"bottom":-1e30}}}"#.into(), "style.margin.bottom"),
        (r#"{"type":"box","style":{"position":"absolute","inset":{"left":2000000}}}"#.into(), "style.inset.left"),
        (r#"{"type":"box","style":{"padding":{"top":3e38}}}"#.into(), "style.padding.top"),
        (r#"{"type":"box","style":{"gap":1e7}}"#.into(), "style.gap"),
        (r#"{"type":"box","style":{"display":"grid","gridTemplateRows":[{"min":0,"max":1e9}]}}"#.into(), "style.gridTemplateRows[0].max"),
        (r#"{"type":"box","style":{"display":"grid","gridTemplateColumns":["1000001fr"]}}"#.into(), "style.gridTemplateColumns[0]"),
        (r#"{"type":"box","style":{"transform":[{"translate":[-1e30,0]}]}}"#.into(), "style.transform[0].translate[0]"),
        (r#"{"type":"box","style":{"transformOrigin":[0,1e30]}}"#.into(), "style.transformOrigin[1]"),
        (r#"{"type":"box","style":{"borderWidth":{"left":1e39}}}"#.into(), "style.borderWidth.left"),
        (r#"{"type":"box","style":{"borderRadius":1e39}}"#.into(), "style.borderRadius"),
        (r#"{"type":"box","style":{"borderRadius":{"bottomLeft":2e6}}}"#.into(), "style.borderRadius.bottomLeft"),
        (r#"{"type":"box","break":{"minPresenceAhead":1e39}}"#.into(), "break.minPresenceAhead"),
        // Other numbers: factors and angles within ±1000000.
        (r#"{"type":"box","style":{"flexGrow":1e39}}"#.into(), "style.flexGrow"),
        (r#"{"type":"box","style":{"flexShrink":2e6}}"#.into(), "style.flexShrink"),
        (r#"{"type":"box","style":{"aspectRatio":1e-30}}"#.into(), "style.aspectRatio"),
        (r#"{"type":"box","style":{"aspectRatio":1e39}}"#.into(), "style.aspectRatio"),
        (r#"{"type":"box","style":{"rotate":1e39}}"#.into(), "style.rotate"),
        (r#"{"type":"box","style":{"transform":[{"rotate":-2e6}]}}"#.into(), "style.transform[0].rotate"),
        (r#"{"type":"box","style":{"transform":[{"scale":[1,3e38]}]}}"#.into(), "style.transform[0].scale[1]"),
        (r#"{"type":"box","style":{"transform":[{"matrix":[1,0,0,1,0,-1e30]}]}}"#.into(), "style.transform[0].matrix[5]"),
        // Tables.
        (r#"{"type":"table","columns":[{"width":1e39}],"rows":[]}"#.into(), "columns[0].width"),
        (r#"{"type":"table","columns":[{"width":10},{"width":"2000000fr"}],"rows":[]}"#.into(), "columns[1].width"),
        (r#"{"type":"table","columns":[{"width":10}],"rows":[{"minHeight":1e39,"cells":[{}]}]}"#.into(), "rows[0].minHeight"),
        (r#"{"type":"table","style":{"borderCollapse":"separate","borderSpacing":1e39},"columns":[{"width":10}],"rows":[]}"#.into(), "style.borderSpacing"),
        (r#"{"type":"table","columns":[{"width":10}],"rows":[{"cells":[{"style":{"padding":{"left":"10001%"}}}]}]}"#.into(), "rows[0].cells[0].style.padding.left"),
        // Text: the existing bounds also reject infinity.
        (r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":10,"letterSpacing":1e39}]}"#.into(), "runs[0].letterSpacing"),
        (r#"{"type":"text","style":{"textIndent":1e39},"runs":[{"text":"x","font":"Inter-Regular","size":10}]}"#.into(), "style.textIndent"),
    ] {
        let (code, got) = one(&doc(&node));
        let want = format!("pages[0].children[0].{path}");
        assert_eq!((code, got.as_str()), (C::OutOfRange, want.as_str()), "{node}");
    }
}

#[test]
fn out_of_range_messages_state_the_range() {
    let errs = run(&doc(&format!(
        r#"{{"type":"box","style":{{"width":{HUGE_PERCENT},"margin":{{"top":-2e6}},"padding":{{"left":-1}},"flexGrow":2e6,"gridTemplateColumns":["2000000fr"]}}}}"#
    )));
    let got: Vec<_> = errs
        .iter()
        .map(|e| (e.path.rsplit('.').next().unwrap(), e.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("flexGrow", "must be at most 1000000"),
            ("width", "percentages must be within [0%, 10000%]"),
            ("left", "must be non-negative"),
            ("top", "points must be within [-1000000, 1000000]"),
            (
                "gridTemplateColumns[0]",
                "flexible sizes must be within [0fr, 1000000fr]"
            ),
        ]
    );
}
