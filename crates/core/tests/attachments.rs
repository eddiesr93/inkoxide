//! Files a PDF carries. With qpdf, poppler's `pdfdetach` and veraPDF
//! installed, the files come out of the PDF as they went in, encrypted or
//! not, and PDF/A-3b output with them passes veraPDF.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{installed, qpdf_available, root};
use inkoxide::{
    AttachmentStore, Encryption, FontStore, ImageStore, InputFormat, PdfALevel, RenderOptions,
};

/// The bytes of the XML file of the tests, and its base64.
const XML: &str = "<?xml version=\"1.0\"?><Invoice><Total>9.70</Total></Invoice>";

fn fonts() -> FontStore {
    let mut fonts = FontStore::new();
    let bytes = fs::read(root().join("fixtures/fonts/Inter-Regular.ttf")).unwrap();
    fonts.register("Inter-Regular", bytes).unwrap();
    fonts
}

fn store() -> AttachmentStore {
    let mut store = AttachmentStore::new();
    store.insert("invoice-xml", XML.as_bytes().to_vec());
    // Compresses well, so it is stored compressed.
    store.insert("notes", "note ".repeat(200).into_bytes());
    store
}

/// Two pages; a file from the store and one inline; an icon in the header
/// (on both pages), one in the flow and one in a line of text.
fn doc(meta: &str) -> String {
    let inline = "aGVsbG8gd29ybGQ="; // "hello world"
    format!(
        r#"{{"version":1,"meta":{{"title":"Attachments","lang":"en"{meta}}},
        "fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
        "attachments":[
          {{"name":"factur-x.xml","src":"invoice-xml","mimeType":"text/xml","relationship":"data",
            "description":"Invoice data","modified":"2026-10-17T09:00:00Z"}},
          {{"name":"notes.txt","src":"notes","mimeType":"text/plain"}},
          {{"name":"Grüße.txt","data":"{inline}","relationship":"supplement"}}
        ],
        "pages":[{{"size":{{"width":300,"height":300}},
          "header":{{"type":"attachment","attachment":{{"file":"notes.txt","icon":"paperclip"}}}},
          "children":[
            {{"type":"attachment","attachment":{{"file":"factur-x.xml","icon":"graph","contents":"The XML"}}}},
            {{"type":"text","runs":[{{"text":"See ","font":"Inter-Regular","size":12}},
              {{"attachment":{{"file":"Grüße.txt","icon":"tag","color":[1,0,0,1]}},"font":"Inter-Regular","size":12}}]}},
            {{"type":"pageBreak"}},
            {{"type":"text","runs":[{{"text":"Page two","font":"Inter-Regular","size":12}}]}}
          ]}}]}}"#
    )
}

fn render(doc: &str, options: &RenderOptions) -> Result<Vec<u8>, inkoxide::InkoxideError> {
    inkoxide::render(
        doc.as_bytes(),
        InputFormat::Json,
        options,
        &fonts(),
        &ImageStore::new(),
    )
}

fn out_dir() -> PathBuf {
    let dir = root().join("target/attachments");
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Each file `pdfdetach` saves from `pdf`, by name.
fn detached(pdf: &Path, password: Option<&str>) -> Vec<(String, Vec<u8>)> {
    let dir = out_dir().join(format!(
        "{}-files",
        pdf.file_stem().unwrap().to_string_lossy()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut cmd = Command::new("pdfdetach");
    if let Some(p) = password {
        cmd.args(["-upw", p]);
    }
    let o = cmd
        .args(["-saveall", "-o"])
        .arg(&dir)
        .arg(pdf)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&p).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

fn expected_files() -> Vec<(String, Vec<u8>)> {
    vec![
        ("Grüße.txt".into(), b"hello world".to_vec()),
        ("factur-x.xml".into(), XML.as_bytes().to_vec()),
        ("notes.txt".into(), "note ".repeat(200).into_bytes()),
    ]
}

#[test]
fn files_come_out_as_they_went_in() {
    let options = RenderOptions::default()
        .with_attachments(store())
        .with_compress(false);
    let pdf = render(&doc(""), &options).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    // Each file is written once, however many icons open it.
    assert_eq!(text.matches("/Type /Filespec").count(), 3);
    assert_eq!(text.matches("/Subtype /FileAttachment").count(), 4);
    assert!(text.contains("/AFRelationship /Data"), "{text}");
    assert!(text.contains("/Subtype /text#2Fxml"));
    // Each icon is associated with its file; not PDF/A-3, so the catalog
    // lists no associated files.
    assert_eq!(text.matches("/AF [").count(), 4);
    let file = out_dir().join("plain.pdf");
    fs::write(&file, &pdf).unwrap();
    if qpdf_available() {
        let o = Command::new("qpdf")
            .arg("--check")
            .arg(&file)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    }
    if installed("pdfdetach", "-v") {
        assert_eq!(detached(&file, None), expected_files());
    }
}

#[test]
fn encrypted_files_decrypt_to_the_same_bytes() {
    let options = RenderOptions::default()
        .with_attachments(store())
        .with_encryption(
            Encryption::new()
                .with_user_password("user")
                .with_owner_password("owner"),
        );
    let pdf = render(&doc(""), &options).unwrap();
    assert!(!pdf.windows(XML.len()).any(|w| w == XML.as_bytes()));
    let file = out_dir().join("encrypted.pdf");
    fs::write(&file, &pdf).unwrap();
    if installed("pdfdetach", "-v") {
        assert_eq!(detached(&file, Some("user")), expected_files());
    }
}

#[test]
fn documents_without_files_are_unchanged_by_the_option() {
    let plain = r#"{"version":1,"pages":[{"size":{"width":100,"height":100}}]}"#;
    let a = render(plain, &RenderOptions::default()).unwrap();
    let b = render(plain, &RenderOptions::default().with_attachments(store())).unwrap();
    assert_eq!(a, b);
}

/// Factur-X's metadata: the `fx` schema, declared for PDF/A.
const FACTUR_X: &str = r#","xmpExtensions":[{"schema":"Factur-X PDFA Extension Schema",
    "namespace":"urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#","prefix":"fx","properties":[
    {"name":"DocumentFileName","value":"factur-x.xml","description":"Name of the embedded XML invoice file"},
    {"name":"DocumentType","value":"INVOICE","description":"INVOICE"},
    {"name":"Version","value":"1.0","description":"The actual version of the Factur-X XML schema"},
    {"name":"ConformanceLevel","value":"EN 16931","description":"The conformance level of the embedded Factur-X data"}]}]"#;

#[test]
fn pdf_a_3b_carries_files_and_their_metadata() {
    let options = RenderOptions::default()
        .with_attachments(store())
        .with_pdf_a_level(PdfALevel::A3b)
        .with_compress(false);
    let pdf = render(&doc(FACTUR_X), &options).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    // The catalog's associated files, besides the four icons'.
    assert_eq!(text.matches("/AF [").count(), 5, "no associated files");
    assert!(text.contains("<fx:DocumentType>INVOICE</fx:DocumentType>"));
    assert!(text.contains("<pdfaSchema:prefix>fx</pdfaSchema:prefix>"));
    let file = out_dir().join("pdf-a-3b.pdf");
    fs::write(&file, &pdf).unwrap();
    if let Ok(verapdf) = std::env::var("VERAPDF") {
        let o = Command::new(verapdf)
            .args(["--flavour", "3b", "--format", "text"])
            .arg(&file)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&o.stdout);
        assert!(report.starts_with("PASS"), "{report}");
    }
    // The files are part of the identifier.
    let other = {
        let mut s = store();
        s.insert("invoice-xml", b"<Invoice/>".to_vec());
        render(&doc(FACTUR_X), &options.clone().with_attachments(s)).unwrap()
    };
    let id = |pdf: &[u8]| {
        let t = String::from_utf8_lossy(pdf).into_owned();
        t[t.rfind("/ID").unwrap()..]
            .lines()
            .next()
            .unwrap()
            .to_string()
    };
    assert_ne!(id(&pdf), id(&other));
}

#[test]
fn levels_and_documents_that_cannot_carry_them_are_refused() {
    let code = |doc: &str, options: &RenderOptions| {
        let e = render(doc, options).unwrap_err();
        match e {
            inkoxide::InkoxideError::Validation(v) => (v.code.as_str().to_string(), v.path),
            e => panic!("{e}"),
        }
    };
    let options = RenderOptions::default().with_attachments(store());
    for level in [PdfALevel::A1b, PdfALevel::A2b] {
        assert_eq!(
            code(&doc(""), &options.clone().with_pdf_a_level(level)),
            ("PDFA_CONFLICT".into(), "attachments[0]".into()),
            "{level:?}"
        );
    }
    // XMP extensions need the PDF/A metadata.
    assert_eq!(
        code(&doc(FACTUR_X), &options),
        ("INVALID_VALUE".into(), "meta.xmpExtensions".into())
    );
    // Only a render knows the store.
    assert_eq!(
        code(&doc(""), &RenderOptions::default()),
        (
            "ATTACHMENT_NOT_REGISTERED".into(),
            "attachments[0].src".into()
        )
    );
    let validated = inkoxide::validate(
        doc("").as_bytes(),
        InputFormat::Json,
        &fonts(),
        &ImageStore::new(),
    );
    assert_eq!(validated, []);
}

#[test]
fn bad_declarations_and_icons_are_reported_at_their_paths() {
    let codes = |attachments: &str, child: &str| -> Vec<(String, String)> {
        let doc = format!(
            r#"{{"version":1,"attachments":[{attachments}],
               "pages":[{{"size":{{"width":100,"height":100}},"children":[{child}]}}]}}"#
        );
        inkoxide::validate(
            doc.as_bytes(),
            InputFormat::Json,
            &fonts(),
            &ImageStore::new(),
        )
        .into_iter()
        .map(|e| (e.code.as_str().to_string(), e.path))
        .collect()
    };
    let one = |code: &str, path: &str| vec![(code.to_string(), path.to_string())];
    let icon = r#"{"type":"attachment","attachment":{"file":"missing.txt"}}"#;
    assert_eq!(
        codes("", icon),
        one("UNKNOWN_ATTACHMENT", "pages[0].children[0].attachment.file")
    );
    for (decl, code, path) in [
        (
            r#"{"name":"a/b","data":""}"#,
            "INVALID_VALUE",
            "attachments[0].name",
        ),
        (
            r#"{"name":"a","data":"","mimeType":"text"}"#,
            "INVALID_VALUE",
            "attachments[0].mimeType",
        ),
        (
            r#"{"name":"a","data":"","modified":"yesterday"}"#,
            "INVALID_VALUE",
            "attachments[0].modified",
        ),
        (
            r#"{"name":"a","data":"!!"}"#,
            "INVALID_VALUE",
            "attachments[0].data",
        ),
        (
            r#"{"name":"a","data":"","src":"b"}"#,
            "INVALID_VALUE",
            "attachments[0].data",
        ),
        (r#"{"name":"a"}"#, "MISSING_FIELD", "attachments[0]"),
        (
            r#"{"name":"a","data":""},{"name":"a","data":""}"#,
            "DUPLICATE_ID",
            "attachments[1].name",
        ),
    ] {
        assert_eq!(codes(decl, ""), one(code, path), "{decl}");
    }
    // Too large is told before the bytes are decoded.
    let big = format!(
        r#"{{"name":"big","data":"{}"}}"#,
        "A".repeat((64 << 20) / 3 * 4 + 8)
    );
    assert_eq!(
        codes(&big, ""),
        one("ATTACHMENT_TOO_LARGE", "attachments[0].data")
    );
}
