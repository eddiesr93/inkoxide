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
    use crate::validate::rules::{
        MAX_BOOKMARK_LEVEL, MAX_BOOKMARK_TITLE, MAX_DECORATION_THICKNESS, MAX_FIELD_NAME,
        MAX_FONT_SIZE, MAX_LETTER_SPACING, MAX_LINE_HEIGHT, MAX_ORPHANS, MAX_TEXT_INDENT,
        MAX_WEIGHT,
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
                doc(
                    "",
                    &format!(
                        r#"{{"type":"field","field":{{"name":"{}","kind":"checkbox"}}}}"#,
                        chars(n)
                    ),
                )
            }),
            MAX_FIELD_NAME as f64 + 1.0,
            "INVALID_VALUE",
            "pages[0].children[0].field.name".into(),
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
