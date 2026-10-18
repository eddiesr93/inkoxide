//! The JSON Schema is the contract; the serde structs must accept exactly
//! the same property names. For each object type we inject an unknown key,
//! read the list of expected fields from the error message and compare it
//! with the schema's `properties`.

use std::collections::BTreeSet;
use std::path::Path;

use super::de::NodeType;
use super::Document;
use crate::test_fixtures::fixture;
use crate::{validate, FontStore, ImageStore, InputFormat};

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
    let errs = validate(
        doc.as_bytes(),
        InputFormat::Json,
        &FontStore::new(),
        &ImageStore::new(),
    );
    assert_eq!(errs.len(), 1, "{doc}: {errs:#?}");
    assert_eq!(errs[0].code.as_str(), "UNKNOWN_FIELD", "{doc}: {errs:#?}");
    // The structured message is shortened; the raw serde one lists fields:
    // "unknown field `zzUnknown`, expected one of `a`, `b`".
    let msg = serde_json::from_slice::<Document>(doc.as_bytes())
        .expect_err("expected a parse error")
        .to_string();
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
    for (def, shape) in [
        ("ShapeCircle", "circle"),
        ("ShapeEllipse", "ellipse"),
        ("ShapeInset", "inset"),
    ] {
        check(
            &s,
            Some(def),
            &node(&format!(
                r#"{{"type":"box","style":{{"float":"left","shapeOutside":{{"{shape}":{{@"at":[0,0]}}}}}}}}"#
            )),
        );
    }
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
        Some("BackgroundImage"),
        &node(r#"{"type":"box","backgroundImage":{@"src":"i"}}"#),
    );
    check(
        &s,
        Some("InlineImage"),
        &node(r#"{"type":"text","runs":[{"font":"f","size":1,"image":{@"src":"i"}}]}"#),
    );
    check(
        &s,
        Some("Field"),
        &node(r#"{"type":"field","field":{@"name":"n","kind":"text"}}"#),
    );
    check(
        &s,
        Some("FieldFormat"),
        &node(r#"{"type":"field","field":{"name":"n","kind":"text","format":{@"type":"zip"}}}"#),
    );
    check(
        &s,
        Some("Note"),
        &node(r#"{"type":"note","note":{@"contents":"n"}}"#),
    );
    check(
        &s,
        Some("Attachment"),
        &node(r#"{"type":"attachment","attachment":{@"file":"a"}}"#),
    );
    check(
        &s,
        Some("AttachmentDecl"),
        &format!(
            r#"{{"version":1,"attachments":[{{@"name":"a","data":""}}],"pages":[{{{PAGE}}}]}}"#
        ),
    );
    check(
        &s,
        Some("XmpSchema"),
        &format!(
            r#"{{"version":1,"meta":{{"xmpExtensions":[{{@"schema":"s","namespace":"n","prefix":"p","properties":[]}}]}},"pages":[{{{PAGE}}}]}}"#
        ),
    );
    check(
        &s,
        Some("XmpProperty"),
        &format!(
            r#"{{"version":1,"meta":{{"xmpExtensions":[{{"schema":"s","namespace":"n","prefix":"p","properties":[{{@"name":"a","value":"v","description":"d"}}]}}]}},"pages":[{{{PAGE}}}]}}"#
        ),
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
        "NoteNode",
        "AttachmentNode",
    ] {
        want.extend(schema_props(&s, Some(d)));
    }
    let got = struct_fields(&node(r#"{@"type":"box"}"#));
    assert_eq!(got, want, "node property mismatch");
}

/// Each node type takes exactly the properties of its `*Node` definition.
#[test]
fn node_types_match_schema() {
    let s = schema();
    for ty in NodeType::ALL {
        let name = ty.name();
        let mut def = name.to_string();
        def[..1].make_ascii_uppercase();
        def.push_str("Node");
        assert_eq!(s["$defs"][&def]["properties"]["type"]["const"], name);
        let mut want = schema_props(&s, Some(&def));
        want.remove("type");
        want.remove("id");
        let got: BTreeSet<String> = ty.fields().map(str::to_string).collect();
        assert_eq!(got, want, "{def} property mismatch");
    }
}

/// `(code, path)` of each error the engine reports for `doc`. A font and an
/// image are registered under each of `ids`.
fn errors(doc: &str, ids: &[&str]) -> Vec<(String, String)> {
    let mut fonts = FontStore::new();
    let mut images = ImageStore::new();
    for id in ids {
        fonts
            .register(*id, fixture("fonts/Inter-Regular.ttf"))
            .unwrap();
        images.insert(*id, fixture("images/logo.png"));
    }
    validate(doc.as_bytes(), InputFormat::Json, &fonts, &images)
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

/// Builds a document with a given value where a bound applies.
type Make<'a> = Box<dyn Fn(f64) -> String + 'a>;

/// The bounds validation names as constants are the schema's: each equals
/// the schema's value, and the engine accepts a value at it and rejects one
/// past it, at the value's path.
#[test]
#[allow(clippy::cast_precision_loss)]
fn named_bounds_match_schema() {
    use crate::forms::format::{MAX_CURRENCY, MAX_DECIMALS, MAX_PATTERN};
    use crate::validate::rules::{
        MAX_ALT, MAX_BOOKMARK_LEVEL, MAX_BOOKMARK_TITLE, MAX_DECORATION_THICKNESS,
        MAX_FEATURE_VALUE, MAX_FIELD_NAME, MAX_FIELD_OPTIONS, MAX_FIELD_OPTION_LEN,
        MAX_FIELD_VALUE, MAX_FONT_FEATURES, MAX_FONT_SIZE, MAX_LETTER_SPACING, MAX_LINE_HEIGHT,
        MAX_LINK, MAX_NOTE, MAX_NOTE_AUTHOR, MAX_NUMBER, MAX_ORPHANS, MAX_PAGE_EDGE,
        MAX_TEXT_INDENT, MAX_VARIATIONS, MAX_VARIATION_VALUE, MAX_WEIGHT,
    };
    let s = schema();
    let doc = |fonts: &str, child: &str| {
        format!(r#"{{"version":1,"fonts":[{fonts}],"pages":[{{{PAGE},"children":[{child}]}}]}}"#)
    };
    let font = r#"{"id":"f","family":"F"}"#;
    let text = |style: &str, run: &str| {
        doc(
            font,
            &format!(
                r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"x","font":"f","size":9{run}}}]}}"#
            ),
        )
    };
    let field = |f: &str| {
        doc(
            "",
            &format!(r#"{{"type":"field","field":{{"name":"n","kind":"checkbox"{f}}}}}"#),
        )
    };
    let chars = |n: f64| "x".repeat(n as usize);
    let format = |f: &str| {
        doc(
            font,
            &format!(
                r#"{{"type":"field","field":{{"name":"n","kind":"text","font":"f","format":{{{f}}}}}}}"#
            ),
        )
    };
    let select = |options: &[String]| {
        doc(
            font,
            &format!(
                r#"{{"type":"field","field":{{"name":"n","kind":"select","font":"f","options":{}}}}}"#,
                serde_json::to_string(options).unwrap()
            ),
        )
    };
    // `n` features, each with a tag of its own.
    let features = |n: f64| {
        (0..n as usize)
            .map(|k| format!(r#""f{k:03}":1"#))
            .collect::<Vec<_>>()
            .join(",")
    };
    let run = "pages[0].children[0].runs[0]";
    let style = "pages[0].children[0].style";
    // (schema pointer, the engine's bound, the document with a value, the
    // first value past the bound, its code and path).
    let cases: Vec<(&str, f64, Make, f64, &str, String)> = vec![
        (
            "/$defs/FontDecl/properties/weight/maximum",
            f64::from(MAX_WEIGHT),
            Box::new(|v| doc(&format!(r#"{{"id":"f","family":"F","weight":{v}}}"#), "")),
            f64::from(MAX_WEIGHT) + 1.0,
            "OUT_OF_RANGE",
            "fonts[0].weight".into(),
        ),
        (
            "/$defs/Field/properties/name/maxLength",
            MAX_FIELD_NAME as f64,
            Box::new(|n| {
                // Parts short enough, with a dot every 128 characters.
                let name: String = (0..n as usize)
                    .map(|i| if i % 128 == 64 { '.' } else { 'x' })
                    .collect();
                doc(
                    "",
                    &format!(r#"{{"type":"field","field":{{"name":"{name}","kind":"checkbox"}}}}"#),
                )
            }),
            MAX_FIELD_NAME as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.name".into(),
        ),
        (
            "/$defs/Field/properties/options/maxItems",
            MAX_FIELD_OPTIONS as f64,
            Box::new(|n| select(&vec!["o".to_string(); n as usize])),
            MAX_FIELD_OPTIONS as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.options".into(),
        ),
        (
            "/$defs/Field/properties/options/items/maxLength",
            MAX_FIELD_OPTION_LEN as f64,
            Box::new(|n| select(&[chars(n)])),
            MAX_FIELD_OPTION_LEN as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.options[0]".into(),
        ),
        (
            "/$defs/Field/properties/value/oneOf/0/maxLength",
            MAX_FIELD_VALUE as f64,
            Box::new(|n| {
                doc(
                    font,
                    &format!(
                        r#"{{"type":"field","field":{{"name":"n","kind":"text","font":"f","value":"{}"}}}}"#,
                        chars(n)
                    ),
                )
            }),
            MAX_FIELD_VALUE as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.value".into(),
        ),
        (
            "/$defs/Field/properties/value/oneOf/1/maxItems",
            MAX_FIELD_OPTIONS as f64,
            Box::new(|n| {
                let options: Vec<String> = (0..(n as usize).min(MAX_FIELD_OPTIONS))
                    .map(|i| format!("o{i}"))
                    .collect();
                let values: Vec<String> = (0..n as usize)
                    .map(|i| format!("o{}", i % MAX_FIELD_OPTIONS))
                    .collect();
                doc(
                    font,
                    &format!(
                        r#"{{"type":"field","field":{{"name":"n","kind":"list","multiSelect":true,"font":"f","options":{},"value":{}}}}}"#,
                        serde_json::to_string(&options).unwrap(),
                        serde_json::to_string(&values).unwrap()
                    ),
                )
            }),
            MAX_FIELD_OPTIONS as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.value".into(),
        ),
        (
            "/$defs/FieldFormat/properties/decimals/maximum",
            f64::from(MAX_DECIMALS),
            Box::new(|v| format(&format!(r#""type":"number","decimals":{v}"#))),
            f64::from(MAX_DECIMALS) + 1.0,
            "OUT_OF_RANGE",
            "pages[0].children[0].field.format.decimals".into(),
        ),
        (
            "/$defs/FieldFormat/properties/currency/maxLength",
            MAX_CURRENCY as f64,
            Box::new(|n| format(&format!(r#""type":"number","currency":"{}""#, chars(n)))),
            MAX_CURRENCY as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.format.currency".into(),
        ),
        (
            "/$defs/FieldFormat/properties/pattern/maxLength",
            MAX_PATTERN as f64,
            Box::new(|n| {
                let pattern: String = (0..n as usize)
                    .map(|i| if i % 2 == 0 { 'd' } else { '/' })
                    .collect();
                format(&format!(r#""type":"date","pattern":"{pattern}""#))
            }),
            MAX_PATTERN as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.format.pattern".into(),
        ),
        (
            "/$defs/Note/properties/contents/maxLength",
            MAX_NOTE as f64,
            Box::new(|n| {
                doc(
                    "",
                    &format!(r#"{{"type":"note","note":{{"contents":"{}"}}}}"#, chars(n)),
                )
            }),
            MAX_NOTE as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].note.contents".into(),
        ),
        (
            "/$defs/Note/properties/author/maxLength",
            MAX_NOTE_AUTHOR as f64,
            Box::new(|n| {
                doc(
                    "",
                    &format!(
                        r#"{{"type":"note","note":{{"contents":"c","author":"{}"}}}}"#,
                        chars(n)
                    ),
                )
            }),
            MAX_NOTE_AUTHOR as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].note.author".into(),
        ),
        (
            "/$defs/TextRun/properties/size/maximum",
            f64::from(MAX_FONT_SIZE),
            Box::new(|v| {
                doc(
                    font,
                    &format!(r#"{{"type":"text","runs":[{{"text":"x","font":"f","size":{v}}}]}}"#),
                )
            }),
            f64::from(MAX_FONT_SIZE) + 0.5,
            "OUT_OF_RANGE",
            format!("{run}.size"),
        ),
        (
            "/$defs/Field/properties/size/maximum",
            f64::from(MAX_FONT_SIZE),
            Box::new(|v| field(&format!(r#","size":{v}"#))),
            f64::from(MAX_FONT_SIZE) + 0.5,
            "OUT_OF_RANGE",
            "pages[0].children[0].field.size".into(),
        ),
        (
            "/$defs/Bookmark/properties/title/maxLength",
            MAX_BOOKMARK_TITLE as f64,
            Box::new(|n| {
                doc(
                    "",
                    &format!(r#"{{"type":"box","bookmark":{{"title":"{}"}}}}"#, chars(n)),
                )
            }),
            MAX_BOOKMARK_TITLE as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].bookmark.title".into(),
        ),
        (
            "/$defs/Bookmark/properties/level/maximum",
            f64::from(MAX_BOOKMARK_LEVEL),
            Box::new(|v| {
                doc(
                    "",
                    &format!(r#"{{"type":"box","bookmark":{{"title":"t","level":{v}}}}}"#),
                )
            }),
            f64::from(MAX_BOOKMARK_LEVEL) + 1.0,
            "OUT_OF_RANGE",
            "pages[0].children[0].bookmark.level".into(),
        ),
        (
            "/$defs/TextRun/properties/decorationThickness/maximum",
            f64::from(MAX_DECORATION_THICKNESS),
            Box::new(|v| text("", &format!(r#","decorationThickness":{v}"#))),
            f64::from(MAX_DECORATION_THICKNESS) + 0.5,
            "OUT_OF_RANGE",
            format!("{run}.decorationThickness"),
        ),
        (
            "/$defs/TextRun/properties/letterSpacing/maximum",
            f64::from(MAX_LETTER_SPACING),
            Box::new(|v| text("", &format!(r#","letterSpacing":{v}"#))),
            f64::from(MAX_LETTER_SPACING) + 0.5,
            "OUT_OF_RANGE",
            format!("{run}.letterSpacing"),
        ),
        (
            "/$defs/TextRun/properties/letterSpacing/minimum",
            -f64::from(MAX_LETTER_SPACING),
            Box::new(|v| text("", &format!(r#","letterSpacing":{v}"#))),
            -f64::from(MAX_LETTER_SPACING) - 0.5,
            "OUT_OF_RANGE",
            format!("{run}.letterSpacing"),
        ),
        (
            "/$defs/TextRun/properties/fontFeatures/maxProperties",
            MAX_FONT_FEATURES as f64,
            Box::new(|n| text("", &format!(r#","fontFeatures":{{{}}}"#, features(n)))),
            MAX_FONT_FEATURES as f64 + 1.0,
            "OUT_OF_RANGE",
            format!("{run}.fontFeatures"),
        ),
        (
            "/$defs/FontDecl/properties/variations/maxProperties",
            MAX_VARIATIONS as f64,
            Box::new(|n| {
                let axes = (0..n as usize)
                    .map(|k| format!(r#""a{k:03}":1"#))
                    .collect::<Vec<_>>();
                doc(
                    &format!(
                        r#"{{"id":"f","family":"F","variations":{{{}}}}}"#,
                        axes.join(",")
                    ),
                    "",
                )
            }),
            MAX_VARIATIONS as f64 + 1.0,
            "OUT_OF_RANGE",
            "fonts[0].variations".into(),
        ),
        (
            "/$defs/FontDecl/properties/variations/additionalProperties/maximum",
            f64::from(MAX_VARIATION_VALUE),
            Box::new(|v| {
                doc(
                    &format!(r#"{{"id":"f","family":"F","variations":{{"wght":{v}}}}}"#),
                    "",
                )
            }),
            f64::from(MAX_VARIATION_VALUE) + 1.0,
            "OUT_OF_RANGE",
            "fonts[0].variations".into(),
        ),
        (
            "/$defs/FontDecl/properties/variations/additionalProperties/minimum",
            -f64::from(MAX_VARIATION_VALUE),
            Box::new(|v| {
                doc(
                    &format!(r#"{{"id":"f","family":"F","variations":{{"wght":{v}}}}}"#),
                    "",
                )
            }),
            -f64::from(MAX_VARIATION_VALUE) - 1.0,
            "OUT_OF_RANGE",
            "fonts[0].variations".into(),
        ),
        (
            "/$defs/TextRun/properties/fontFeatures/additionalProperties/maximum",
            f64::from(MAX_FEATURE_VALUE),
            Box::new(|v| text("", &format!(r#","fontFeatures":{{"salt":{v}}}"#))),
            f64::from(MAX_FEATURE_VALUE) + 1.0,
            "OUT_OF_RANGE",
            format!("{run}.fontFeatures"),
        ),
        (
            "/$defs/Style/properties/lineHeight/maximum",
            f64::from(MAX_LINE_HEIGHT),
            Box::new(|v| text(&format!(r#""lineHeight":{v}"#), "")),
            f64::from(MAX_LINE_HEIGHT) + 0.5,
            "OUT_OF_RANGE",
            format!("{style}.lineHeight"),
        ),
        (
            "/$defs/Style/properties/textIndent/maximum",
            f64::from(MAX_TEXT_INDENT),
            Box::new(|v| text(&format!(r#""textIndent":{v}"#), "")),
            f64::from(MAX_TEXT_INDENT) + 0.5,
            "OUT_OF_RANGE",
            format!("{style}.textIndent"),
        ),
        (
            "/$defs/Style/properties/textIndent/minimum",
            -f64::from(MAX_TEXT_INDENT),
            Box::new(|v| text(&format!(r#""textIndent":{v}"#), "")),
            -f64::from(MAX_TEXT_INDENT) - 0.5,
            "OUT_OF_RANGE",
            format!("{style}.textIndent"),
        ),
        (
            "/$defs/Style/properties/orphans/maximum",
            f64::from(MAX_ORPHANS),
            Box::new(|v| text(&format!(r#""orphans":{v}"#), "")),
            f64::from(MAX_ORPHANS) + 1.0,
            "OUT_OF_RANGE",
            format!("{style}.orphans"),
        ),
        (
            "/$defs/Style/properties/widows/maximum",
            f64::from(MAX_ORPHANS),
            Box::new(|v| text(&format!(r#""widows":{v}"#), "")),
            f64::from(MAX_ORPHANS) + 1.0,
            "OUT_OF_RANGE",
            format!("{style}.widows"),
        ),
    ];
    let mut cases = cases;
    let page = |width: f64, height: f64| {
        format!(
            r#"{{"version":1,"pages":[{{"size":{{"width":{width},"height":{height}}},"children":[]}}]}}"#
        )
    };
    let boxed = |b: &str| doc("", &format!(r#"{{"type":"box",{b}}}"#));
    let node = "pages[0].children[0]";
    cases.extend::<[(&str, f64, Make, f64, &str, String); 5]>([
        (
            "/$defs/Size/properties/width/maximum",
            f64::from(MAX_PAGE_EDGE),
            Box::new(move |v| page(v, 100.0)),
            f64::from(MAX_PAGE_EDGE) + 0.5,
            "OUT_OF_RANGE",
            "pages[0].size.width".into(),
        ),
        (
            "/$defs/Size/properties/height/maximum",
            f64::from(MAX_PAGE_EDGE),
            Box::new(move |v| page(100.0, v)),
            f64::from(MAX_PAGE_EDGE) + 0.5,
            "OUT_OF_RANGE",
            "pages[0].size.height".into(),
        ),
        (
            "/$defs/Length/oneOf/0/maximum",
            f64::from(MAX_NUMBER),
            Box::new(move |v| boxed(&format!(r#""style":{{"width":{v}}}"#))),
            f64::from(MAX_NUMBER) + 1.0,
            "OUT_OF_RANGE",
            format!("{style}.width"),
        ),
        (
            "/$defs/Length/oneOf/0/minimum",
            -f64::from(MAX_NUMBER),
            Box::new(move |v| boxed(&format!(r#""style":{{"margin":{{"left":{v}}}}}"#))),
            -f64::from(MAX_NUMBER) - 1.0,
            "OUT_OF_RANGE",
            format!("{style}.margin.left"),
        ),
        (
            "/$defs/Link/maxLength",
            MAX_LINK as f64,
            Box::new(move |n| {
                let target = format!("https://e.com/{}", chars(n - 14.0));
                boxed(&format!(r#""link":"{target}""#))
            }),
            MAX_LINK as f64 + 1.0,
            "INVALID_VALUE",
            format!("{node}.link"),
        ),
    ]);
    cases.push((
        "/$defs/InlineImage/properties/alt/maxLength",
        MAX_ALT as f64,
        Box::new(move |n| {
            doc(
                font,
                &format!(
                    r#"{{"type":"text","runs":[{{"font":"f","size":9,"image":{{"src":"f","alt":"{}"}}}}]}}"#,
                    chars(n)
                ),
            )
        }),
        MAX_ALT as f64 + 1.0,
        "INVALID_VALUE",
        format!("{run}.image.alt"),
    ));
    cases.push((
        "/$defs/ImageNode/properties/alt/maxLength",
        MAX_ALT as f64,
        Box::new(move |n| {
            doc(
                "",
                &format!(r#"{{"type":"image","src":"f","alt":"{}"}}"#, chars(n)),
            )
        }),
        MAX_ALT as f64 + 1.0,
        "INVALID_VALUE",
        format!("{node}.alt"),
    ));
    for (pointer, bound, make, past, code, path) in cases {
        let stated = s
            .pointer(pointer)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_else(|| panic!("the schema has no bound at {pointer}"));
        assert_eq!(stated, bound, "{pointer}");
        assert_eq!(errors(&make(bound), &["f"]), [], "{pointer}: {bound}");
        assert_eq!(
            errors(&make(past), &["f"]),
            one(code, &path),
            "{pointer}: {past}"
        );
    }
}

/// The schema's patterns for percentages state the engine's bound, and the
/// engine accepts a percentage at it and rejects one past it.
#[test]
fn percent_bounds_match_schema() {
    use crate::validate::rules::MAX_PERCENT;
    let s = schema();
    let most = (MAX_PERCENT * 100.0) as u32;
    let digits = most.to_string().len() - 1;
    for (def, pattern) in [
        (
            "Percent",
            format!(r"^-?0*([0-9]{{1,{digits}}}(\.[0-9]+)?|{most}(\.0+)?)%$"),
        ),
        (
            "NonNegativePercent",
            format!(r"^(0*([0-9]{{1,{digits}}}(\.[0-9]+)?|{most}(\.0+)?)|-0+(\.0+)?)%$"),
        ),
    ] {
        let stated = s
            .pointer(&format!("/$defs/{def}/pattern"))
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert_eq!(stated, pattern, "{def}");
    }
    // `width` is never negative, a margin may be.
    let doc = |style: &str| {
        format!(
            r#"{{"version":1,"pages":[{{{PAGE},"children":[{{"type":"box","style":{{{style}}}}}]}}]}}"#
        )
    };
    let path = "pages[0].children[0].style";
    for (ok, past, at) in [
        (
            format!(r#""width":"{most}%""#),
            format!(r#""width":"{most}.5%""#),
            "width",
        ),
        (
            format!(r#""margin":{{"left":"-{most}%"}}"#),
            format!(r#""margin":{{"left":"-{most}.5%"}}"#),
            "margin.left",
        ),
    ] {
        assert_eq!(errors(&doc(&ok), &[]), [], "{ok}");
        assert_eq!(
            errors(&doc(&past), &[]),
            one("OUT_OF_RANGE", &format!("{path}.{at}")),
            "{past}"
        );
    }
}

/// The schema's pattern for field names states the engine's bounds on
/// their parts, and the engine enforces them.
#[test]
fn field_name_parts_match_schema() {
    use crate::validate::rules::{MAX_FIELD_PARTS, MAX_FIELD_PART_LEN};
    let s = schema();
    let pattern = s
        .pointer("/$defs/Field/properties/name/pattern")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    assert_eq!(
        pattern,
        format!(
            r"^[^.]{{1,{MAX_FIELD_PART_LEN}}}(\.[^.]{{1,{MAX_FIELD_PART_LEN}}}){{0,{}}}$",
            MAX_FIELD_PARTS - 1
        )
    );
    let doc = |name: &str| {
        format!(
            r#"{{"version":1,"pages":[{{{PAGE},"children":[{{"type":"field","field":{{"name":"{name}","kind":"checkbox"}}}}]}}]}}"#
        )
    };
    let path = "pages[0].children[0].field.name";
    let part = "p".repeat(MAX_FIELD_PART_LEN);
    let parts = |n: usize| vec!["p"; n].join(".");
    for ok in [part.clone(), format!("a.{part}"), parts(MAX_FIELD_PARTS)] {
        assert_eq!(errors(&doc(&ok), &[]), [], "{ok}");
    }
    for bad in [
        format!("{part}p"),
        format!("a.{part}p"),
        parts(MAX_FIELD_PARTS + 1),
        "a..b".into(),
        ".a".into(),
        "a.".into(),
        String::new(),
    ] {
        assert_eq!(errors(&doc(&bad), &[]), one("INVALID_VALUE", path), "{bad}");
    }
}

/// Integers past their type's range are `OUT_OF_RANGE`, like those past a
/// bound the schema states, and the schema states the type's range where it
/// has no tighter bound.
#[test]
fn integer_bounds_match_schema() {
    let s = schema();
    let u32_max = i64::from(u32::MAX);
    for pointer in [
        "/$defs/Style/properties/maxLines/maximum",
        "/$defs/Field/properties/maxLength/maximum",
        "/$defs/TableCell/properties/colSpan/maximum",
        "/$defs/TableCell/properties/rowSpan/maximum",
        "/$defs/TableNode/properties/headerRows/maximum",
    ] {
        assert_eq!(bound(&s, pointer), u32_max, "{pointer}");
    }
    let pages = "/$defs/PageFilter/oneOf/1/items";
    assert_eq!(bound(&s, &format!("{pages}/minimum")), i64::from(i32::MIN));
    assert_eq!(bound(&s, &format!("{pages}/maximum")), i64::from(i32::MAX));
    assert_eq!(
        bound(&s, "/$defs/Style/properties/zIndex/maximum"),
        i64::from(i32::MAX)
    );

    let doc = |child: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"f","family":"F"}}],"pages":[{{{PAGE},"children":[{child}]}}]}}"#
        )
    };
    let past = u32_max + 1;
    let table = |cell: &str| {
        format!(
            r#"{{"type":"table","columns":[{{"width":1}}],"headerRows":0,"rows":[{{"cells":[{{{cell}}}]}}]}}"#
        )
    };
    let text = |style: &str| {
        format!(
            r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"x","font":"f","size":9}}]}}"#
        )
    };
    let p = "pages[0].children[0]";
    for (child, path) in [
        (
            text(&format!(r#""maxLines":{past}"#)),
            format!("{p}.style.maxLines"),
        ),
        (text(r#""orphans":-1"#), format!("{p}.style.orphans")),
        (text(r#""widows":70000"#), format!("{p}.style.widows")),
        (
            format!(
                r#"{{"type":"field","field":{{"name":"n","kind":"text","font":"f","maxLength":{past}}}}}"#
            ),
            format!("{p}.field.maxLength"),
        ),
        (
            table(&format!(r#""colSpan":{past}"#)),
            format!("{p}.rows[0].cells[0].colSpan"),
        ),
        (
            table(r#""rowSpan":-1"#),
            format!("{p}.rows[0].cells[0].rowSpan"),
        ),
        (
            table("").replace(r#""headerRows":0"#, r#""headerRows":-1"#),
            format!("{p}.headerRows"),
        ),
        (
            r#"{"type":"box","style":{"zIndex":2147483648}}"#.into(),
            format!("{p}.style.zIndex"),
        ),
        (
            r#"{"type":"box","pages":[1,-2147483649]}"#.into(),
            format!("{p}.pages[1]"),
        ),
        (
            r#"{"type":"box","bookmark":{"title":"t","level":300}}"#.into(),
            format!("{p}.bookmark.level"),
        ),
    ] {
        assert_eq!(
            errors(&doc(&child), &["f"]),
            one("OUT_OF_RANGE", &path),
            "{child}"
        );
    }
    let font = |w: i64| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"f","family":"F","weight":{w}}}],"pages":[{{{PAGE}}}]}}"#
        )
    };
    for w in [-1, 70_000] {
        assert_eq!(
            errors(&font(w), &["f"]),
            one("OUT_OF_RANGE", "fonts[0].weight")
        );
    }
}

/// The bound of `fallbackFonts` is the schema's.
#[test]
fn fallback_font_bound_matches_schema() {
    use crate::validate::rules::MAX_FALLBACK_FONTS;
    assert_eq!(
        bound(&schema(), "/properties/fallbackFonts/maxItems"),
        MAX_FALLBACK_FONTS as i64
    );
}

/// The bounds of `emojiImages` are the schema's.
#[test]
fn emoji_image_bounds_match_schema() {
    use crate::validate::rules::{MAX_EMOJI_IMAGES, MAX_EMOJI_SEQUENCE};
    let s = schema();
    assert_eq!(
        bound(&s, "/properties/emojiImages/maxProperties"),
        MAX_EMOJI_IMAGES as i64
    );
    assert_eq!(
        bound(&s, "/properties/emojiImages/propertyNames/maxLength"),
        MAX_EMOJI_SEQUENCE as i64
    );
}

/// The schema's bounds on attachments and XMP extensions are the engine's.
#[test]
#[allow(clippy::cast_precision_loss)]
fn attachment_bounds_match_schema() {
    use crate::attachments::{MAX_ATTACHMENTS, MAX_FILE_NAME, MAX_MIME_TYPE};
    use crate::validate::rules::{
        MAX_NOTE, MAX_XMP_NAME, MAX_XMP_PROPERTIES, MAX_XMP_SCHEMAS, MAX_XMP_VALUE,
    };
    type Make = Box<dyn Fn(usize) -> String>;
    let s = schema();
    let doc = move |attachments: &str, meta: &str| {
        format!(
            r#"{{"version":1,"meta":{{{meta}}},"attachments":[{attachments}],"pages":[{{{PAGE}}}]}}"#
        )
    };
    let file = move |name: &str, extra: &str| format!(r#"{{"name":"{name}","data":""{extra}}}"#);
    let files = move |n: usize| {
        (0..n)
            .map(|i| file(&format!("f{i}"), ""))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mime = move |n: usize| format!("a/{}", "b".repeat(n - 2));
    let schema_of = move |props: usize| {
        let props = (0..props)
            .map(|i| format!(r#"{{"name":"p{i}","value":"v","description":"d"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"schema":"s","namespace":"urn:x","prefix":"fx","properties":[{props}]}}"#)
    };
    let schemas = move |n: usize| {
        let all = (0..n)
            .map(|i| {
                format!(r#"{{"schema":"s","namespace":"urn:x","prefix":"x{i}","properties":[]}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#""xmpExtensions":[{all}]"#)
    };
    // One schema with property `property`, as the schema declares them.
    let xmp = move |schema: &str, namespace: &str, property: &str| {
        let s = format!(
            r#"{{"schema":"{schema}","namespace":"{namespace}","prefix":"fx","properties":[{property}]}}"#
        );
        doc("", &format!(r#""xmpExtensions":[{s}]"#))
    };
    let property = move |name: &str, value: &str, description: &str| {
        format!(r#"{{"name":"{name}","value":"{value}","description":"{description}"}}"#)
    };
    // A file and an icon on the page that opens it.
    let icon = move |name: &str, contents: &str| {
        format!(
            r#"{{"version":1,"attachments":[{}],"pages":[{{{PAGE},"children":[{{"type":"attachment","attachment":{{"file":"{name}"{contents}}}}}]}}]}}"#,
            file(name, "")
        )
    };
    let cases: Vec<(&str, usize, Make, &str, &str)> = vec![
        (
            "/$defs/AttachmentDecl/properties/description/maxLength",
            MAX_NOTE,
            Box::new(move |n| {
                doc(
                    &file("f", &format!(r#","description":"{}""#, "d".repeat(n))),
                    "",
                )
            }),
            "INVALID_VALUE",
            "attachments[0].description",
        ),
        (
            "/$defs/Attachment/properties/contents/maxLength",
            MAX_NOTE,
            Box::new(move |n| icon("f", &format!(r#","contents":"{}""#, "c".repeat(n)))),
            "INVALID_VALUE",
            "pages[0].children[0].attachment.contents",
        ),
        (
            "/$defs/Attachment/properties/file/maxLength",
            MAX_FILE_NAME,
            Box::new(move |n| icon(&"f".repeat(n), "")),
            "INVALID_VALUE",
            "attachments[0].name",
        ),
        (
            "/$defs/XmpSchema/properties/schema/maxLength",
            MAX_XMP_NAME,
            Box::new(move |n| xmp(&"s".repeat(n), "urn:x", "")),
            "INVALID_VALUE",
            "meta.xmpExtensions[0].schema",
        ),
        (
            "/$defs/XmpSchema/properties/namespace/maxLength",
            MAX_XMP_NAME,
            Box::new(move |n| xmp("s", &format!("urn:{}", "x".repeat(n - 4)), "")),
            "INVALID_VALUE",
            "meta.xmpExtensions[0].namespace",
        ),
        (
            "/$defs/XmpProperty/properties/name/maxLength",
            MAX_XMP_NAME,
            Box::new(move |n| xmp("s", "urn:x", &property(&"p".repeat(n), "v", "d"))),
            "INVALID_VALUE",
            "meta.xmpExtensions[0].properties[0].name",
        ),
        (
            "/$defs/XmpProperty/properties/value/maxLength",
            MAX_XMP_VALUE,
            Box::new(move |n| xmp("s", "urn:x", &property("p", &"v".repeat(n), "d"))),
            "INVALID_VALUE",
            "meta.xmpExtensions[0].properties[0].value",
        ),
        (
            "/$defs/XmpProperty/properties/description/maxLength",
            MAX_XMP_NAME,
            Box::new(move |n| xmp("s", "urn:x", &property("p", "v", &"d".repeat(n)))),
            "INVALID_VALUE",
            "meta.xmpExtensions[0].properties[0].description",
        ),
        (
            "/properties/attachments/maxItems",
            MAX_ATTACHMENTS,
            Box::new(move |n| doc(&files(n), "")),
            "OUT_OF_RANGE",
            "attachments",
        ),
        (
            "/$defs/AttachmentDecl/properties/name/maxLength",
            MAX_FILE_NAME,
            Box::new(move |n| doc(&file(&"n".repeat(n), ""), "")),
            "INVALID_VALUE",
            "attachments[0].name",
        ),
        (
            "/$defs/AttachmentDecl/properties/mimeType/maxLength",
            MAX_MIME_TYPE,
            Box::new(move |n| {
                // Each half of a MIME type is at most 63 bytes.
                let m = if n <= MAX_MIME_TYPE {
                    format!("{}/{}", "a".repeat(63), "b".repeat(n - 64))
                } else {
                    mime(n)
                };
                doc(&file("f", &format!(r#","mimeType":"{m}""#)), "")
            }),
            "INVALID_VALUE",
            "attachments[0].mimeType",
        ),
        (
            "/$defs/Meta/properties/xmpExtensions/maxItems",
            MAX_XMP_SCHEMAS,
            Box::new(move |n| doc("", &schemas(n))),
            "OUT_OF_RANGE",
            "meta.xmpExtensions",
        ),
        (
            "/$defs/XmpSchema/properties/properties/maxItems",
            MAX_XMP_PROPERTIES,
            Box::new(move |n| doc("", &format!(r#""xmpExtensions":[{}]"#, schema_of(n)))),
            "OUT_OF_RANGE",
            "meta.xmpExtensions[0].properties",
        ),
    ];
    for (pointer, bound, make, code, path) in cases {
        let stated = s
            .pointer(pointer)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_else(|| panic!("the schema has no bound at {pointer}"));
        assert_eq!(stated, bound as f64, "{pointer}");
        assert_eq!(errors(&make(bound), &[]), [], "{pointer}: {bound}");
        assert_eq!(errors(&make(bound + 1), &[]), one(code, path), "{pointer}");
    }
}
