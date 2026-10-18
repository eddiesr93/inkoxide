//! Tagged output and PDF/UA-1: `fixtures/docs/accessible.json` passes
//! veraPDF's PDF/UA-1 and PDF/A-2a and 3a checks (with `VERAPDF` set), its
//! structure tree reads back in document order with qpdf, and a document
//! that lacks what PDF/UA requires is refused with `UA_CONFLICT`.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use common::{installed, qpdf_available, root, sorted_files, stores, verapdf, verapdf_report};
use inkoxide::{Encryption, InputFormat, PdfALevel, Permissions, RenderOptions};
use serde_json::Value;

fn fixture() -> Vec<u8> {
    fs::read(root().join("fixtures/docs/accessible.json")).unwrap()
}

fn render(doc: &[u8], options: &RenderOptions) -> Result<Vec<u8>, inkoxide::InkoxideError> {
    let (fonts, images) = stores();
    inkoxide::render(doc, InputFormat::Json, options, &fonts, &images)
}

fn write(name: &str, pdf: &[u8]) -> PathBuf {
    let dir = root().join("target/accessibility");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.pdf"));
    fs::write(&file, pdf).unwrap();
    file
}

#[test]
fn pdf_ua_and_the_accessible_pdf_a_levels_pass_verapdf() {
    let ua = RenderOptions::default().with_pdf_ua(true);
    let ua1 = write("ua1", &render(&fixture(), &ua).unwrap());
    verapdf("ua1", std::slice::from_ref(&ua1));
    // How far it is from PDF/UA-2 (on PDF 2.0, which inkoxide does not
    // write): reported, not checked.
    if let Some(report) = verapdf_report("ua2", &[ua1]) {
        eprintln!("veraPDF (ua2), for the record:\n{report}");
    }
    for level in [PdfALevel::A2a, PdfALevel::A3a] {
        let options = RenderOptions::default().with_pdf_a_level(level);
        let file = write(level.as_str(), &render(&fixture(), &options).unwrap());
        verapdf(level.as_str(), &[file]);
        let both = write(
            &format!("{}-ua1", level.as_str()),
            &render(&fixture(), &options.clone().with_pdf_ua(true)).unwrap(),
        );
        verapdf(level.as_str(), std::slice::from_ref(&both));
        verapdf("ua1", &[both]);
    }
}

/// qpdf's objects of `pdf`, written as `name`, by reference (`"12 0 R"`).
fn objects(name: &str, pdf: &[u8]) -> serde_json::Map<String, Value> {
    let file = write(name, pdf);
    let o = Command::new("qpdf")
        .args(["--json=2", "--json-key=qpdf"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let json: Value = serde_json::from_slice(&o.stdout).unwrap();
    json["qpdf"][1]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.trim_start_matches("obj:").to_string(), v["value"].clone()))
        .collect()
}

#[test]
fn the_structure_reads_in_document_order() {
    if !qpdf_available() {
        return;
    }
    let pdf = render(&fixture(), &RenderOptions::default().with_tagged(true)).unwrap();
    let objects = objects("structure", &pdf);
    let elem = |r: &str| &objects[r];
    let kind = |r: &str| elem(r)["/S"].as_str().unwrap().to_string();
    let kids = |r: &str| -> Vec<Value> { elem(r)["/K"].as_array().cloned().unwrap_or_default() };
    let elems = |r: &str| -> Vec<String> {
        kids(r)
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let catalog = objects.values().find(|v| v["/Type"] == "/Catalog").unwrap();
    assert_eq!(catalog["/MarkInfo"]["/Marked"], true);
    let root = catalog["/StructTreeRoot"].as_str().unwrap();
    let document = objects[root]["/K"].as_str().unwrap();
    assert_eq!(kind(document), "/Document");
    // The decorative image, the `artifact` text, the header and the fixed
    // footer are not in it.
    let top: Vec<String> = elems(document).iter().map(|r| kind(r)).collect();
    assert_eq!(
        top,
        [
            "/H1", "/P", "/H2", "/L", "/H2", "/Table", "/H3", "/Figure", "/Figure", "/P", "/Link",
            "/Form", "/Annot", "/P", "/P"
        ]
    );
    let r = |i: usize| elems(document)[i].clone();
    assert_eq!(elem(&r(7))["/Alt"], "u:The Inkoxide logo");
    assert_eq!(elem(&r(8))["/Alt"], "u:Sales rose 12% in the third quarter");
    for i in [7, 8] {
        assert_eq!(elem(&r(i))["/A"]["/BBox"].as_array().map(Vec::len), Some(4));
    }
    // The list: items, each a label and a body.
    let items = elems(&r(3));
    assert_eq!(items.len(), 3);
    assert_eq!(elem(&r(3))["/A"]["/ListNumbering"], "/Decimal");
    let parts: Vec<String> = elems(&items[0]).iter().map(|r| kind(r)).collect();
    assert_eq!(parts, ["/Lbl", "/LBody"]);
    // The table: a header row of column headers, then rows with a row
    // header, a span and an empty cell.
    let groups = elems(&r(5));
    assert_eq!(
        groups.iter().map(|g| kind(g)).collect::<Vec<_>>(),
        ["/THead", "/TBody"]
    );
    let head = &elems(&elems(&groups[0])[0])[0];
    assert_eq!(
        (kind(head), &elem(head)["/A"]["/Scope"]),
        ("/TH".into(), &Value::from("/Column"))
    );
    let rows = elems(&groups[1]);
    let support: Vec<String> = elems(&rows[1]).iter().map(|c| kind(c)).collect();
    assert_eq!(support, ["/TH", "/TD"]);
    assert_eq!(elem(&elems(&rows[1])[1])["/A"]["/ColSpan"], 2);
    assert_eq!(elems(&rows[2]).len(), 3, "the empty cell is kept");
    // The link in the paragraph: a Link element with its annotation.
    let link = elems(&r(9));
    assert_eq!(link.len(), 1);
    assert_eq!(kind(&link[0]), "/Link");
    let objr = |r: &str| kids(r).iter().filter(|k| k["/Type"] == "/OBJR").count();
    assert_eq!(objr(&link[0]), 1);
    for i in [10, 11, 12] {
        assert_eq!(objr(&r(i)), 1, "{}", kind(&r(i)));
    }
    // A link says what it is: the text it covers.
    let mut said: Vec<&str> = objects
        .values()
        .filter(|v| v["/Subtype"] == "/Link")
        .filter_map(|v| v["/Contents"].as_str())
        .collect();
    said.sort_unstable();
    assert_eq!(said, ["u:Jump to the end", "u:guide"]);
    // The long paragraph goes on to the second page: marked content there
    // is referenced with its page.
    let long = kids(&r(13));
    assert!(long.iter().any(|k| k["/Type"] == "/MCR"), "{long:?}");
    // Every page and annotation has its key in the parent tree.
    let tree = objects[root]["/ParentTree"].as_str().unwrap();
    let nums = objects[tree]["/Nums"].as_array().unwrap().len() / 2;
    let annots = objects
        .values()
        .filter(|v| v["/StructParent"].is_number())
        .count();
    assert_eq!(nums, 2 + annots);
    assert_eq!(objects[root]["/ParentTreeNextKey"], nums);
}

#[test]
fn tagged_output_is_the_same_on_any_number_of_threads() {
    let base = RenderOptions::default().with_pdf_ua(true);
    let one = render(&fixture(), &base.clone().with_threads(1)).unwrap();
    let four = render(&fixture(), &base.with_threads(4)).unwrap();
    assert!(one == four, "tagged output differs between thread counts");
}

#[test]
fn a_document_without_what_pdf_ua_requires_is_refused() {
    let doc = |meta: &str, children: &str| {
        format!(
            r#"{{"version":1,"meta":{{{meta}}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":200,"height":200}},"children":[{children}]}}]}}"#
        )
    };
    let meta = r#""title":"T","lang":"en""#;
    let text = |role: &str| {
        format!(
            r#"{{"type":"text","role":"{role}","runs":[{{"text":"x","font":"Inter-Regular","size":9}}]}}"#
        )
    };
    let ua = RenderOptions::default().with_pdf_ua(true);
    let refused = |d: String, options: &RenderOptions| match render(d.as_bytes(), options) {
        Err(inkoxide::InkoxideError::Validation(e)) => (e.code.as_str().to_string(), e.path),
        other => panic!("{d}: {:?}", other.map(|_| ())),
    };
    let image = r#"{"type":"image","data":"data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiLz4="}"#;
    let run_image = |alt: &str| run_image(alt, image);
    for (d, path) in [
        (doc(r#""lang":"en""#, ""), "meta.title"),
        (doc(r#""title":"T""#, ""), "meta.lang"),
        (doc(meta, image), "pages[0].children[0].alt"),
        (
            doc(meta, r#"{"type":"box","role":"figure","children":[]}"#),
            "pages[0].children[0].alt",
        ),
        (
            doc(
                meta,
                r#"{"type":"field","field":{"name":"a","kind":"checkbox"}}"#,
            ),
            "pages[0].children[0].field.label",
        ),
        // A field's widget is tagged even in decoration.
        (
            doc(
                meta,
                r#"{"type":"box","role":"artifact","children":[
                    {"type":"field","field":{"name":"a","kind":"checkbox"}}]}"#,
            ),
            "pages[0].children[0].children[0].field.label",
        ),
        (
            doc(
                meta,
                r#"{"type":"box","break":{"fixed":true},"children":[
                    {"type":"field","field":{"name":"a","kind":"checkbox"}}]}"#,
            ),
            "pages[0].children[0].children[0].field.label",
        ),
        (
            doc(meta, &run_image("")),
            "pages[0].children[0].runs[1].image.alt",
        ),
        (doc(meta, &text("h2")), "pages[0].children[0].role"),
        // Lists are made of items, each with a body.
        (
            doc(meta, r#"{"type":"box","role":"listItem","children":[]}"#),
            "pages[0].children[0].role",
        ),
        (
            doc(
                meta,
                r#"{"type":"box","role":"list","children":[{"type":"box","children":[
                    {"type":"box","role":"listBody","children":[]}]}]}"#,
            ),
            "pages[0].children[0].children[0].children[0].role",
        ),
        (
            doc(meta, &format!("{},{}", text("h1"), text("h3"))),
            "pages[0].children[1].role",
        ),
    ] {
        assert_eq!(
            refused(d, &ua),
            ("UA_CONFLICT".into(), path.into()),
            "{path}"
        );
    }
    // An item in a list, through boxes without a role, is in it.
    let list = r#"{"type":"box","role":"list","children":[{"type":"box","children":[
        {"type":"box","role":"listItem","children":[{"type":"box","role":"listBody","children":[]}]}]}]}"#;
    render(doc(meta, list).as_bytes(), &ua).unwrap();
    // Decoration needs none of it, and tagging alone needs no title.
    let decorative = doc(meta, &image.replace("\"data\"", r#""alt":"","data""#));
    render(decorative.as_bytes(), &ua).unwrap();
    render(doc(meta, &run_image(r#","alt":"""#)).as_bytes(), &ua).unwrap();
    render(
        doc("", image).as_bytes(),
        &RenderOptions::default().with_tagged(true),
    )
    .unwrap();
    // Assistive technology must be able to read an encrypted file.
    let locked = ua.with_encryption(
        Encryption::new()
            .with_owner_password("owner")
            .with_permissions(Permissions::new().with_content_accessibility(false)),
    );
    assert_eq!(
        refused(doc(meta, ""), &locked),
        (
            "INVALID_VALUE".into(),
            "options.encryption.permissions.contentAccessibility".into()
        )
    );
}

/// Tagging changes what the file says, not what it shows: every fixture,
/// tagged, extracts the same text and draws the same pixels (its first
/// two pages) as untagged, when poppler is installed.
#[test]
fn tagged_fixtures_read_and_look_as_untagged() {
    if !installed("pdftotext", "-v") || !installed("pdftoppm", "-v") {
        return;
    }
    let dir = root().join("target/accessibility/parity");
    fs::create_dir_all(&dir).unwrap();
    let run = |tool: &str, args: &[&str]| {
        let o = Command::new(tool).args(args).output().unwrap();
        assert!(
            o.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
        let name = doc.file_stem().unwrap().to_str().unwrap().to_string();
        let input = fs::read(&doc).unwrap();
        let mut seen: Vec<(String, Vec<u8>)> = Vec::new();
        for tagged in [false, true] {
            let options = RenderOptions::default()
                .with_creation_date("2026-10-08T00:00:00Z")
                .with_tagged(tagged);
            let pdf = render(&input, &options).unwrap();
            let base = dir.join(format!("{name}-{tagged}"));
            let file = base.with_extension("pdf");
            fs::write(&file, pdf).unwrap();
            let (file, base) = (file.to_str().unwrap(), base.to_str().unwrap());
            let text = format!("{base}.txt");
            run("pdftotext", &[file, &text]);
            let mut out = vec![("text".to_string(), fs::read(&text).unwrap())];
            run("pdftoppm", &["-r", "36", "-l", "2", file, base]);
            for page in sorted_files(&dir, &["ppm"]) {
                let stem = page.file_name().unwrap().to_str().unwrap().to_string();
                if let Some(n) = stem.strip_prefix(&format!("{name}-{tagged}-")) {
                    out.push((n.to_string(), fs::read(&page).unwrap()));
                    fs::remove_file(&page).unwrap();
                }
            }
            if seen.is_empty() {
                seen = out;
            } else {
                assert_eq!(seen.len(), out.len(), "{name}: pages");
                for ((what, a), (_, b)) in seen.iter().zip(&out) {
                    assert!(a == b, "{name}: {what} differs when tagged");
                }
            }
        }
    }
}

/// A paragraph with a run image from `image`'s `data`, with `alt` (`""`:
/// none; `,"alt":""`: an empty one) between two runs of text.
fn run_image(alt: &str, image: &str) -> String {
    let data = image
        .split(r#""data":"#)
        .nth(1)
        .unwrap()
        .trim_end_matches('}');
    format!(
        r#"{{"type":"text","runs":[{{"text":"Before ","font":"Inter-Regular","size":9}},
          {{"image":{{"data":{data},"width":9,"height":9{alt}}},"font":"Inter-Regular","size":9}},
          {{"text":" after","font":"Inter-Regular","size":9}}]}}"#
    )
}

/// An image in text with an empty `alt` is decoration: an artifact amid
/// the paragraph's content, which goes on after it in a sequence of its
/// own.
#[test]
fn an_image_in_text_with_an_empty_alt_is_an_artifact() {
    let image = r#""data":"data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiLz4="}"#;
    let doc = format!(
        r#"{{"version":1,"meta":{{"title":"T","lang":"en"}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
        "pages":[{{"size":{{"width":200,"height":200}},"children":[{}]}}]}}"#,
        run_image(r#","alt":"""#, image)
    );
    let options = RenderOptions::default()
        .with_pdf_ua(true)
        .with_compress(false);
    let pdf = render(doc.as_bytes(), &options).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let page = text
        .split("stream\n")
        .find(|s| s.contains(" BDC"))
        .and_then(|s| s.split("endstream").next())
        .unwrap();
    let artifact = page.find("/Artifact BMC").expect(page);
    let image = page.find(" Do").expect(page);
    let first = page.find("/P <<\n  /MCID 0").expect(page);
    let second = page.find("/P <<\n  /MCID 1").expect(page);
    assert!(
        first < artifact && artifact < image && image < second,
        "{page}"
    );
    assert!(!page.contains("/ActualText"), "{page}");
    verapdf("ua1", &[write("decoration", &pdf)]);
}

/// An image in text with alternative text is a `Figure` inside the
/// paragraph, where it is among the text, with its `/Alt` and where it is;
/// a linked run a `Link` with its text and its annotation.
#[test]
fn images_and_links_in_text_have_elements_in_place() {
    if !qpdf_available() {
        return;
    }
    let image = r#""data":"data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiLz4="}"#;
    let linked = run_image(r#","alt":"A dot""#, image).replace(
        r#"{"text":" after","#,
        r#"{"text":" a link","link":"https://example.com","font":"Inter-Regular","size":9},{"text":" after","#,
    );
    let doc = format!(
        r#"{{"version":1,"meta":{{"title":"T","lang":"en"}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
        "pages":[{{"size":{{"width":200,"height":200}},"children":[{linked}]}}]}}"#
    );
    let pdf = render(doc.as_bytes(), &RenderOptions::default().with_pdf_ua(true)).unwrap();
    let objects = objects("run-elements", &pdf);
    let p = objects.values().find(|v| v["/S"] == "/P").unwrap();
    // Text, the figure, the link, text.
    let kids = p["/K"].as_array().unwrap();
    let shape: Vec<&str> = kids
        .iter()
        .map(|k| match k.as_str() {
            Some(r) => objects[r]["/S"].as_str().unwrap(),
            None => "mcid",
        })
        .collect();
    assert_eq!(shape, ["mcid", "/Figure", "/Link", "mcid"], "{kids:?}");
    let figure = &objects[kids[1].as_str().unwrap()];
    assert_eq!(figure["/Alt"], "u:A dot");
    assert_eq!(figure["/A"]["/BBox"].as_array().map(Vec::len), Some(4));
    // The link holds its text and its annotation.
    let link = objects[kids[2].as_str().unwrap()]["/K"].as_array().unwrap();
    assert!(link[0].is_number(), "{link:?}");
    assert_eq!(link.last().unwrap()["/Type"], "/OBJR", "{link:?}");
    verapdf("ua1", &[write("run-elements", &pdf)]);
}

/// Marked content and save levels nest (ISO 32000-1, 14.6): no `q` … `Q`
/// pair holds the start or the end of a sequence without the other, on any
/// page of the fixture, tagged.
#[test]
fn marked_content_nests_with_save_levels() {
    let options = RenderOptions::default()
        .with_pdf_ua(true)
        .with_compress(false);
    // And text drawn inside save levels of its own: a colour with alpha,
    // a box with opacity, a link.
    let inside = br#"{"version":1,"meta":{"title":"T","lang":"en"},"fonts":[{"id":"Inter-Regular","family":"Inter"}],
        "pages":[{"size":{"width":300,"height":300},"children":[
          {"type":"text","runs":[{"text":"Half ","font":"Inter-Regular","size":9,"color":[0,0,0,0.5]},
            {"text":"linked","font":"Inter-Regular","size":9,"link":"https://example.com"}]},
          {"type":"box","style":{"opacity":0.5,"backgroundColor":[1,0,0,1]},"children":[
            {"type":"text","runs":[{"text":"Veiled","font":"Inter-Regular","size":9}]}]}]}]}"#;
    let mut pdf = render(&fixture(), &options).unwrap();
    pdf.extend(render(inside, &options).unwrap());
    let text = String::from_utf8_lossy(&pdf);
    let mut streams = 0;
    for stream in text.split("stream\n").skip(1) {
        let Some(body) = stream.split("endstream").next() else {
            continue;
        };
        if !body.contains(" BDC") {
            continue;
        }
        streams += 1;
        // Each frame: a save level (`q`) or a sequence (`BDC`, `BMC`).
        let mut stack: Vec<&str> = Vec::new();
        for token in body.split_ascii_whitespace() {
            match token {
                "q" | "BDC" | "BMC" => stack.push(if token == "q" { "q" } else { "mc" }),
                "Q" => assert_eq!(stack.pop(), Some("q"), "a Q inside a sequence"),
                "EMC" => assert_eq!(stack.pop(), Some("mc"), "an EMC inside a save level"),
                _ => {}
            }
        }
        assert!(stack.is_empty(), "{stack:?} left open");
    }
    assert!(streams >= 3, "{streams} tagged content streams");
}

/// A figure that draws only its background and borders keeps its element
/// and its alternative text.
#[test]
fn a_figure_of_backgrounds_and_borders_keeps_its_element() {
    if !qpdf_available() {
        return;
    }
    let doc = r#"{"version":1,"meta":{"title":"T","lang":"en"},"pages":[{"size":{"width":200,"height":200},
        "children":[{"type":"box","role":"figure","alt":"A red square",
          "style":{"width":20,"height":20,"backgroundColor":[1,0,0,1],
                   "borderWidth":{"top":1,"right":1,"bottom":1,"left":1}},"children":[]}]}]}"#;
    let pdf = render(doc.as_bytes(), &RenderOptions::default().with_pdf_ua(true)).unwrap();
    let objects = objects("figure-objects", &pdf);
    let figure = objects
        .values()
        .find(|v| v["/S"] == "/Figure")
        .expect("a Figure element");
    assert_eq!(figure["/Alt"], "u:A red square");
    assert!(
        figure["/K"].is_number() || figure["/K"].is_array(),
        "{figure}"
    );
    // Where it is: the top left of a page 200 high.
    assert_eq!(figure["/A"]["/O"], "/Layout");
    assert_eq!(figure["/A"]["/BBox"], serde_json::json!([0, 180, 20, 200]));
    verapdf("ua1", &[write("figure-box", &pdf)]);
}

/// The annotations of headers, each in an element of its own after the
/// document's content, come in page order.
#[test]
fn annotations_in_headers_come_in_page_order() {
    if !qpdf_available() {
        return;
    }
    let page = r#"{"size":{"width":200,"height":200},
        "header":{"type":"box","children":[
          {"type":"box","link":"https://example.com","style":{"width":20,"height":10},"children":[]},
          {"type":"note","note":{"contents":"A note."}}]},
        "children":[]}"#;
    let doc = format!(r#"{{"version":1,"pages":[{page},{page}]}}"#);
    let pdf = render(doc.as_bytes(), &RenderOptions::default().with_tagged(true)).unwrap();
    let objects = objects("header-annotations", &pdf);
    let catalog = objects.values().find(|v| v["/Type"] == "/Catalog").unwrap();
    let root = catalog["/StructTreeRoot"].as_str().unwrap();
    let document = objects[root]["/K"].as_str().unwrap();
    let page_of = |r: &Value| -> String {
        let e = &objects[r.as_str().unwrap()];
        let objr = e["/K"]
            .as_array()
            .and_then(|k| k.first().cloned())
            .unwrap_or_else(|| e["/K"].clone());
        objr["/Pg"].as_str().unwrap().to_string()
    };
    let kids = objects[document]["/K"].as_array().unwrap();
    let pages: Vec<String> = kids.iter().map(page_of).collect();
    assert_eq!(pages.len(), 4, "{kids:?}");
    assert!(
        pages[0] == pages[1] && pages[2] == pages[3] && pages[1] != pages[2],
        "{pages:?}"
    );
}

/// What headers and footers draw are pagination artifacts that say which
/// they are; the rest of the decoration is a plain artifact.
#[test]
fn headers_and_footers_are_pagination_artifacts() {
    let text = r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":9}]}"#;
    let doc = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":200}},
            "header":{text},"footer":{text},
            "children":[{{"type":"text","role":"artifact","runs":[{{"text":"y","font":"Inter-Regular","size":9}}]}}]}}]}}"#
    );
    let options = RenderOptions::default()
        .with_tagged(true)
        .with_compress(false);
    // Without whitespace, however the dictionaries are laid out.
    let pdf: String = String::from_utf8_lossy(&render(doc.as_bytes(), &options).unwrap())
        .split_whitespace()
        .collect();
    for band in ["Header", "Footer"] {
        let marked = format!("/Artifact<</Type/Pagination/Subtype/{band}>>BDC");
        assert_eq!(pdf.matches(&marked).count(), 1, "{band}");
    }
    assert_eq!(pdf.matches("/ArtifactBMC").count(), 1);
}

/// Under PDF/UA, every fixture is refused for the first thing it lacks
/// (`UA_CONFLICT`), or passes veraPDF's PDF/UA-1 check.
#[test]
fn every_fixture_under_pdf_ua_is_refused_or_passes() {
    let (fonts, images) = stores();
    let dir = root().join("target/accessibility/fixtures");
    fs::create_dir_all(&dir).unwrap();
    let ua = RenderOptions::default().with_pdf_ua(true);
    let (mut passed, mut refused) = (Vec::new(), 0);
    for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
        let name = doc.file_stem().unwrap().to_str().unwrap();
        let input = fs::read(&doc).unwrap();
        match inkoxide::render(&input, InputFormat::Json, &ua, &fonts, &images) {
            Ok(pdf) => {
                let file = dir.join(format!("{name}.pdf"));
                fs::write(&file, pdf).unwrap();
                passed.push(file);
            }
            Err(inkoxide::InkoxideError::Validation(e)) if e.code.as_str() == "UA_CONFLICT" => {
                refused += 1;
            }
            Err(e) => panic!("{name}: {e}"),
        }
    }
    eprintln!("{} pass, {refused} refused", passed.len());
    assert!(!passed.is_empty() && refused > 0);
    verapdf("ua1", &passed);
}

/// A table split across pages repeats its header row on each: the first is
/// read, the others are artifacts.
#[test]
fn a_split_table_reads_its_header_once() {
    if !qpdf_available() {
        return;
    }
    let row = |t: &str| {
        format!(
            r#"{{"cells":[{{"children":[{{"type":"text","runs":[{{"text":"{t}","font":"Inter-Regular","size":9}}]}}]}}]}}"#
        )
    };
    let rows: Vec<String> = std::iter::once(row("Head"))
        .chain((0..40).map(|i| row(&format!("Row {i}"))))
        .collect();
    let doc = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":200}},
        "children":[{{"type":"table","columns":[{{"width":"1fr"}}],"headerRows":1,"rows":[{}]}}]}}]}}"#,
        rows.join(",")
    );
    let pdf = render(doc.as_bytes(), &RenderOptions::default().with_tagged(true)).unwrap();
    let objects = objects("split-table", &pdf);
    let pages = objects.values().filter(|v| v["/Type"] == "/Page").count();
    assert!(pages >= 2, "{pages} pages");
    // The only child of an element.
    let only = |v: &Value| -> Value {
        match v["/K"].as_array() {
            Some(k) => {
                assert_eq!(k.len(), 1, "{v}");
                k[0].clone()
            }
            None => v["/K"].clone(),
        }
    };
    let elem = |k: Value| objects[k.as_str().unwrap()].clone();
    let head = objects.values().find(|v| v["/S"] == "/THead").unwrap();
    let th = elem(only(&elem(only(head))));
    assert_eq!(th["/S"], "/TH");
    // Its text: one sequence, on the first page.
    let p = elem(only(&th));
    assert!(only(&p).is_number(), "{p}");
    let first = objects
        .values()
        .find(|v| v["/Type"] == "/Pages")
        .and_then(|v| v["/Kids"][0].as_str().map(str::to_string))
        .unwrap();
    assert_eq!(p["/Pg"], first);
}

/// Reading order is document order, whatever `zIndex` and floats do to the
/// order things are drawn in.
#[test]
fn reading_order_is_document_order() {
    if !qpdf_available() {
        return;
    }
    let text = |t: &str, style: &str| {
        format!(
            r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"{t}","font":"Inter-Regular","size":9}}]}}"#
        )
    };
    let children = [
        text("First", r#""position":"relative","zIndex":2"#),
        text("Second", r#""float":"right","width":50"#),
        text("Third", r#""position":"relative","zIndex":1"#),
        text("Fourth", ""),
    ]
    .join(",");
    let doc = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":200}},
        "children":[{children}]}}]}}"#
    );
    let options = RenderOptions::default().with_tagged(true);
    let pdf = render(doc.as_bytes(), &options).unwrap();
    let objects = objects("reading-order", &pdf);
    let catalog = objects.values().find(|v| v["/Type"] == "/Catalog").unwrap();
    let root = catalog["/StructTreeRoot"].as_str().unwrap();
    let document = &objects[objects[root]["/K"].as_str().unwrap()];
    // Each paragraph's first MCID: the order they are drawn in.
    let mcids: Vec<i64> = document["/K"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let k = &objects[p.as_str().unwrap()]["/K"];
            k.as_i64()
                .or_else(|| k[0].as_i64())
                .unwrap_or_else(|| panic!("{k}"))
        })
        .collect();
    assert_eq!(mcids.len(), 4);
    assert!(
        mcids.windows(2).any(|w| w[0] > w[1]),
        "drawn in document order: {mcids:?}"
    );
}
