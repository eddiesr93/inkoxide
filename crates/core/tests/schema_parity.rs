//! The JSON Schema is the contract; the serde structs must accept exactly
//! the same property names. For each object type we inject an unknown key,
//! read the list of expected fields from the error message and compare it
//! with the schema's `properties`.

use std::collections::BTreeSet;
use std::path::Path;

use inkoxide::{FontStore, ImageStore, InputFormat};

fn schema() -> serde_json::Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/document.v1.json");
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn schema_props(schema: &serde_json::Value, def: Option<&str>) -> BTreeSet<String> {
    let obj = match def {
        Some(d) => &schema["$defs"][d],
        None => schema,
    };
    obj["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

/// Substitutes `"zzUnknown":0,` for `@` in `template` and returns the field
/// names serde reports as expected at that position.
fn struct_fields(template: &str) -> BTreeSet<String> {
    let doc = template.replace('@', r#""zzUnknown":0,"#);
    let errs = inkoxide::validate(
        doc.as_bytes(),
        InputFormat::Json,
        &FontStore::new(),
        &ImageStore::new(),
    );
    assert_eq!(errs.len(), 1, "{doc}: {errs:#?}");
    assert_eq!(errs[0].code.as_str(), "UNKNOWN_FIELD", "{doc}: {errs:#?}");
    // The structured message is shortened; the raw serde one lists fields:
    // "unknown field `zzUnknown`, expected one of `a`, `b`".
    let msg = inkoxide::debug_raw_parse_error(doc.as_bytes()).expect("expected a parse error");
    let tail = msg
        .split("expected")
        .nth(1)
        .unwrap_or_else(|| panic!("no field list in: {msg}"));
    tail.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

const PAGE: &str = r#""size":{"width":10,"height":10}"#;

fn check(schema: &serde_json::Value, def: Option<&str>, template: &str) {
    let want = schema_props(schema, def);
    let got = struct_fields(template);
    assert_eq!(
        got,
        want,
        "property mismatch for {}",
        def.unwrap_or("Document")
    );
}

#[test]
fn structs_match_schema() {
    let s = schema();
    check(
        &s,
        None,
        &format!(r#"{{@"version":1,"pages":[{{{PAGE}}}]}}"#),
    );
    check(
        &s,
        Some("Meta"),
        &format!(r#"{{"version":1,"meta":{{@"title":"x"}},"pages":[{{{PAGE}}}]}}"#),
    );
    check(
        &s,
        Some("FontDecl"),
        &format!(r#"{{"version":1,"fonts":[{{@"id":"a","family":"b"}}],"pages":[{{{PAGE}}}]}}"#),
    );
    check(
        &s,
        Some("PageSpec"),
        &format!(r#"{{"version":1,"pages":[{{@{PAGE}}}]}}"#),
    );
    let node = |n: &str| format!(r#"{{"version":1,"pages":[{{{PAGE},"children":[{n}]}}]}}"#);
    check(
        &s,
        Some("Style"),
        &node(r#"{"type":"box","style":{@"width":1}}"#),
    );
    check(
        &s,
        Some("Break"),
        &node(r#"{"type":"box","break":{@"fixed":false}}"#),
    );
    check(
        &s,
        Some("TextRun"),
        &node(r#"{"type":"text","runs":[{@"text":"x","font":"f","size":1}]}"#),
    );
    check(
        &s,
        Some("GridPlacement"),
        &node(r#"{"type":"box","style":{"gridRow":{@"span":1}}}"#),
    );
    check(
        &s,
        Some("EdgesLengthPercent"),
        &node(r#"{"type":"box","style":{"padding":{@"top":1}}}"#),
    );
    check(
        &s,
        Some("ColumnSpec"),
        &node(r#"{"type":"table","columns":[{@"width":1}],"rows":[]}"#),
    );
    check(
        &s,
        Some("TableRow"),
        &node(r#"{"type":"table","columns":[{"width":1}],"rows":[{@"cells":[]}]}"#),
    );
    check(
        &s,
        Some("TableCell"),
        &node(r#"{"type":"table","columns":[{"width":1}],"rows":[{"cells":[{@"colSpan":1}]}]}"#),
    );

    // Node: the union of all node-type properties.
    let mut want = BTreeSet::new();
    for d in [
        "BoxNode",
        "TextNode",
        "ImageNode",
        "TableNode",
        "PageBreakNode",
        "SpacerNode",
    ] {
        want.extend(schema_props(&s, Some(d)));
    }
    let got = struct_fields(&node(r#"{@"type":"box"}"#));
    assert_eq!(got, want, "node property mismatch");
}
