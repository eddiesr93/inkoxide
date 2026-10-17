//! Tagged output and PDF/UA-1: `fixtures/docs/accessible.json` passes
//! veraPDF's PDF/UA-1 and PDF/A-2a and 3a checks (with `VERAPDF` set), its
//! structure tree reads back in document order with qpdf, and a document
//! that lacks what PDF/UA requires is refused with `UA_CONFLICT`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{installed, qpdf_available, root, sorted_files, stores};
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

/// veraPDF's verdict on `file` at `flavour`, when `VERAPDF` is set.
fn verapdf(file: &Path, flavour: &str) {
    let Ok(verapdf) = std::env::var("VERAPDF") else {
        return;
    };
    let o = Command::new(verapdf)
        .args(["--flavour", flavour, "--format", "text"])
        .arg(file)
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&o.stdout);
    assert!(report.starts_with("PASS"), "{flavour}: {report}");
}

#[test]
fn pdf_ua_and_the_accessible_pdf_a_levels_pass_verapdf() {
    let ua = RenderOptions::default().with_pdf_ua(true);
    verapdf(&write("ua1", &render(&fixture(), &ua).unwrap()), "ua1");
    for level in [PdfALevel::A2a, PdfALevel::A3a] {
        let options = RenderOptions::default().with_pdf_a_level(level);
        let file = write(level.as_str(), &render(&fixture(), &options).unwrap());
        verapdf(&file, level.as_str());
        let both = write(
            &format!("{}-ua1", level.as_str()),
            &render(&fixture(), &options.clone().with_pdf_ua(true)).unwrap(),
        );
        verapdf(&both, level.as_str());
        verapdf(&both, "ua1");
    }
}

/// qpdf's objects of `pdf`, by reference (`"12 0 R"`).
fn objects(pdf: &[u8]) -> serde_json::Map<String, Value> {
    let file = write("objects", pdf);
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
    let objects = objects(&pdf);
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
    // The list: items, each a label and a body.
    let items = elems(&r(3));
    assert_eq!(items.len(), 3);
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
        (
            doc(meta, &run_image("")),
            "pages[0].children[0].runs[1].image.alt",
        ),
        (doc(meta, &text("h2")), "pages[0].children[0].role"),
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
    verapdf(&write("decoration", &pdf), "ua1");
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
