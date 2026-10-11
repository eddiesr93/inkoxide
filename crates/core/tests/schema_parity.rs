//! The JSON Schema is the contract; the serde structs must accept exactly
//! the same property names. For each object type we inject an unknown key,
//! read the list of expected fields from the error message and compare it
//! with the schema's `properties`.

use std::collections::BTreeSet;
use std::path::Path;

use inkoxide::{FontStore, ImageStore, InputFormat};

fn schema() -> serde_json::Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/schema/document.v1.json");
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
    check(
        &s,
        Some("ObjectPosition"),
        &node(r#"{"type":"image","src":"i","objectPosition":{@"x":1}}"#),
    );
    check(
        &s,
        Some("Field"),
        &node(r#"{"type":"field","field":{@"name":"n","kind":"text"}}"#),
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
        "FieldNode",
    ] {
        want.extend(schema_props(&s, Some(d)));
    }
    let got = struct_fields(&node(r#"{@"type":"box"}"#));
    assert_eq!(got, want, "node property mismatch");
}

/// `(code, path)` of each error the engine reports for `doc`. A font and an
/// image are registered under each of `ids`.
fn errors(doc: &str, ids: &[&str]) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut fonts = FontStore::new();
    let mut images = ImageStore::new();
    for id in ids {
        let font = std::fs::read(dir.join("fonts/Inter-Regular.ttf")).unwrap();
        fonts.register(*id, font).unwrap();
        images.insert(*id, std::fs::read(dir.join("images/logo.png")).unwrap());
    }
    inkoxide::validate(doc.as_bytes(), InputFormat::Json, &fonts, &images)
        .into_iter()
        .map(|e| (e.code.as_str().to_string(), e.path))
        .collect()
}

/// A bound the schema must state.
fn bound(schema: &serde_json::Value, pointer: &str) -> i64 {
    schema
        .pointer(pointer)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| panic!("the schema has no bound at {pointer}"))
}

fn one(code: &str, path: &str) -> [(String, String); 1] {
    [(code.to_string(), path.to_string())]
}

/// The engine accepts the values at each of these bounds of the schema and
/// rejects the first value past it, at the value's path.
#[test]
fn bounds_match_schema() {
    let s = schema();
    let doc = |fonts: &str, children: &str| {
        format!(r#"{{"version":1,"fonts":[{fonts}],"pages":[{{{PAGE},"children":[{children}]}}]}}"#)
    };

    // Id: node, row, cell, font and image ids, and a font reference.
    assert_eq!(bound(&s, "/$defs/Id/minLength"), 1);
    let max = usize::try_from(bound(&s, "/$defs/Id/maxLength")).unwrap();
    let table = |row: &str, cell: &str| {
        format!(
            r#"{{"type":"table","columns":[{{"width":1}}],"rows":[{{{row}"cells":[{{{cell}}}]}}]}}"#
        )
    };
    let id_uses = |id: &str| {
        [
            (
                doc("", &format!(r#"{{"type":"box","id":"{id}"}}"#)),
                "pages[0].children[0].id",
            ),
            (
                doc("", &table(&format!(r#""id":"{id}","#), "")),
                "pages[0].children[0].rows[0].id",
            ),
            (
                doc("", &table("", &format!(r#""id":"{id}""#))),
                "pages[0].children[0].rows[0].cells[0].id",
            ),
            (
                doc(&format!(r#"{{"id":"{id}","family":"F"}}"#), ""),
                "fonts[0].id",
            ),
            (
                doc("", &format!(r#"{{"type":"image","src":"{id}"}}"#)),
                "pages[0].children[0].src",
            ),
            (
                doc(
                    "",
                    &format!(r#"{{"type":"text","runs":[{{"text":"x","font":"{id}","size":9}}]}}"#),
                ),
                "pages[0].children[0].runs[0].font",
            ),
        ]
    };
    let longest = "i".repeat(max);
    for (d, _) in id_uses(&longest) {
        // The font reference needs its font declared.
        let d = d.replace(
            r#""fonts":[]"#,
            &format!(r#""fonts":[{{"id":"{longest}","family":"F"}}]"#),
        );
        assert_eq!(errors(&d, &[&longest]), [], "{d}");
    }
    for bad in [String::new(), format!("{longest}i")] {
        for (d, path) in id_uses(&bad) {
            assert_eq!(errors(&d, &[&bad]), one("INVALID_VALUE", path), "{d}");
        }
    }

    // FontDecl.family
    assert_eq!(bound(&s, "/$defs/FontDecl/properties/family/minLength"), 1);
    let font = |family: &str| doc(&format!(r#"{{"id":"f","family":"{family}"}}"#), "");
    assert_eq!(errors(&font("F"), &["f"]), []);
    assert_eq!(
        errors(&font(""), &["f"]),
        one("INVALID_VALUE", "fonts[0].family")
    );

    // pages
    assert_eq!(bound(&s, "/properties/pages/minItems"), 1);
    assert_eq!(
        errors(r#"{"version":1,"pages":[]}"#, &[]),
        one("INVALID_VALUE", "pages")
    );

    // GridPlacement: `start`, `end` and `span`, of rows and columns.
    let grid = "/$defs/GridPlacement/properties";
    let placed = |axis: &str, key: &str, v: i64| {
        doc(
            "",
            &format!(
                r#"{{"type":"box","style":{{"display":"grid"}},"children":[{{"type":"box","style":{{"{axis}":{{"{key}":{v}}}}}}}]}}"#
            ),
        )
    };
    for axis in ["gridRow", "gridColumn"] {
        for key in ["start", "end", "span"] {
            let (min, max) = (
                bound(&s, &format!("{grid}/{key}/minimum")),
                bound(&s, &format!("{grid}/{key}/maximum")),
            );
            let (mut ok, mut bad) = (vec![min, max], vec![min - 1, max + 1]);
            if key != "span" {
                // Lines: there is no line 0.
                assert_eq!(
                    s.pointer(&format!("{grid}/{key}/not/const")),
                    Some(&0.into())
                );
                ok.extend([-1, 1]);
                bad.push(0);
            }
            let path = format!("pages[0].children[0].children[0].style.{axis}.{key}");
            for v in ok {
                assert_eq!(errors(&placed(axis, key, v), &[]), [], "{axis}.{key}: {v}");
            }
            for v in bad {
                assert_eq!(
                    errors(&placed(axis, key, v), &[]),
                    one("OUT_OF_RANGE", &path),
                    "{axis}.{key}: {v}"
                );
            }
        }
    }
}
