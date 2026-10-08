use crate::{validate, FontStore, ImageStore, InputFormat, ValidationCode as C, ValidationError};

fn fonts() -> FontStore {
    let mut f = FontStore::new();
    // Validation only checks presence; any non-trivial bytes will do.
    f.register("Inter-Regular", vec![0u8; 64]).unwrap();
    f
}

fn run(doc: &str) -> Vec<ValidationError> {
    let mut images = ImageStore::new();
    images.insert("logo", vec![0u8; 16]);
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
fn render_refuses_invalid_documents() {
    let err = crate::render(
        doc(r#"{"type":"box","style":{"lineHeight":1}}"#).as_bytes(),
        InputFormat::Json,
        &Default::default(),
        &fonts(),
        &ImageStore::new(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "STYLE_NOT_APPLICABLE");
}
