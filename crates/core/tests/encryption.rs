//! Encrypted output. With `qpdf` installed, every fixture, encrypted,
//! opens with its passwords, and qpdf's view of it, decrypted, is the
//! unencrypted render object for object: a string or stream left in plain
//! text would differ.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use inkoxide::{
    Encryption, FontStore, ImageStore, InkoxideError, InputFormat, PdfALevel, Permissions,
    Printing, RenderOptions,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sorted_files(dir: &Path, ext: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.retain(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ext.contains(&e.to_ascii_lowercase().as_str()))
    });
    v.sort();
    v
}

fn stores() -> (FontStore, ImageStore) {
    let mut fonts = FontStore::new();
    for p in sorted_files(&root().join("fixtures/fonts"), &["ttf", "otf"]) {
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        fonts.register(stem, fs::read(&p).unwrap()).unwrap();
    }
    let mut images = ImageStore::new();
    for p in sorted_files(
        &root().join("fixtures/images"),
        &["png", "jpg", "jpeg", "gif", "webp", "svg"],
    ) {
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        images.insert(stem, fs::read(&p).unwrap());
    }
    (fonts, images)
}

fn qpdf_available() -> bool {
    Command::new("qpdf").arg("--version").output().is_ok()
}

fn qpdf(args: &[&str], file: &Path) -> Output {
    Command::new("qpdf").args(args).arg(file).output().unwrap()
}

/// qpdf's JSON view of `file` (decrypted with `password`): its objects,
/// streams decoded, without the `Encrypt` dictionary and the trailer, and
/// without the catalog's `/Extensions`, which encryption adds.
fn objects(file: &Path, password: Option<&str>) -> serde_json::Map<String, serde_json::Value> {
    let mut args = vec!["--json=2", "--json-stream-data=inline"];
    let pw;
    if let Some(p) = password {
        pw = format!("--password={p}");
        args.push(&pw);
    }
    let o = qpdf(&args, file);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut objects = json["qpdf"][1].as_object().unwrap().clone();
    objects.remove("trailer");
    let encrypt: Vec<String> = objects
        .iter()
        .filter(|(_, v)| v["value"]["/Filter"] == "/Standard")
        .map(|(k, _)| k.clone())
        .collect();
    for k in encrypt {
        objects.remove(&k);
    }
    for v in objects.values_mut() {
        if v["value"]["/Type"] == "/Catalog" {
            v["value"].as_object_mut().unwrap().remove("/Extensions");
        }
    }
    objects
}

/// The options of these tests, encrypted with `encryption`.
fn options(encryption: Option<Encryption>) -> RenderOptions {
    let options = RenderOptions::default().with_creation_date("2026-10-08T00:00:00Z");
    match encryption {
        Some(e) => options.with_encryption(e),
        None => options,
    }
}

const USER: &str = "user (pw) \\ 1";
const OWNER: &str = "owner-pw";

fn both() -> Encryption {
    Encryption::new()
        .with_user_password(USER)
        .with_owner_password(OWNER)
        .with_permissions(Permissions::new().with_printing(Printing::HighResolution))
}

#[test]
fn encrypted_fixtures_decrypt_to_the_plain_render() {
    let (fonts, images) = stores();
    let out_dir = root().join("target/fixtures-encrypted");
    fs::create_dir_all(&out_dir).unwrap();
    let with_qpdf = qpdf_available();
    for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
        let name = doc.file_stem().unwrap().to_str().unwrap();
        let input = fs::read(&doc).unwrap();
        let render = |options: &RenderOptions| {
            inkoxide::render(&input, InputFormat::Json, options, &fonts, &images)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        let plain = render(&options(None));
        let encrypted = render(&options(Some(both())));
        assert!(
            encrypted.starts_with(b"%PDF-1.7"),
            "{name}: not a PDF 1.7 header"
        );
        // Deterministic, on one thread too.
        assert!(
            encrypted == render(&options(Some(both())).with_threads(1)),
            "{name}: encrypted output is not deterministic"
        );
        if !with_qpdf {
            continue;
        }
        let (plain_path, encrypted_path) = (
            out_dir.join(format!("{name}.plain.pdf")),
            out_dir.join(format!("{name}.pdf")),
        );
        fs::write(&plain_path, &plain).unwrap();
        fs::write(&encrypted_path, &encrypted).unwrap();
        for password in [USER, OWNER] {
            let o = qpdf(
                &["--check", &format!("--password={password}")],
                &encrypted_path,
            );
            assert!(
                o.status.success(),
                "{name}: qpdf --check failed:\n{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        assert_eq!(
            objects(&encrypted_path, Some(USER)),
            objects(&plain_path, None),
            "{name}: decrypted objects differ from the plain render"
        );
    }
}

/// Form scripts, field groups, list boxes and notes, which the fixtures
/// do not all have, decrypt to the plain render too.
#[test]
fn encrypted_scripts_groups_lists_and_notes_decrypt_to_the_plain_render() {
    if !qpdf_available() {
        return;
    }
    let (fonts, images) = stores();
    let doc = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
        "pages":[{"size":{"width":300,"height":300},"children":[
          {"type":"field","style":{"height":20},"field":{"name":"order.date","kind":"text","font":"Inter-Regular",
           "value":"2026-10-14","format":{"type":"date","pattern":"dd mmm yyyy"}}},
          {"type":"field","style":{"height":20},"field":{"name":"order.total","kind":"text","font":"Inter-Regular",
           "value":"12.5","format":{"type":"number","currency":"€"}}},
          {"type":"field","style":{"height":60},"field":{"name":"order.items","kind":"list","font":"Inter-Regular",
           "multiSelect":true,"options":["Café","Tea","Water"],"value":["Café","Water"]}},
          {"type":"note","note":{"contents":"Check (this) — Ünïcode","author":"Ana"}}]}]}"#
        .as_bytes();
    let out_dir = root().join("target/fixtures-encrypted");
    fs::create_dir_all(&out_dir).unwrap();
    let write = |name: &str, options: &RenderOptions| {
        let pdf = inkoxide::render(doc, InputFormat::Json, options, &fonts, &images)
            .unwrap_or_else(|e| panic!("{e}"));
        let path = out_dir.join(format!("{name}.pdf"));
        fs::write(&path, pdf).unwrap();
        path
    };
    let plain = write("forms-notes.plain", &options(None));
    let encrypted = write("forms-notes", &options(Some(both())));
    let plain_text = fs::read(&plain).unwrap();
    for needle in [&b"/JS"[..], b"/Opt", b"/Contents", b"/I ["] {
        assert!(
            plain_text.windows(needle.len()).any(|w| w == needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
    assert_eq!(objects(&encrypted, Some(USER)), objects(&plain, None));
}

#[test]
fn qpdf_reads_revision_6_permissions_and_passwords() {
    if !qpdf_available() {
        return;
    }
    let (fonts, images) = stores();
    let input = fs::read(root().join("fixtures/docs/invoice.json")).unwrap();
    let out_dir = root().join("target/fixtures-encrypted");
    fs::create_dir_all(&out_dir).unwrap();
    let write = |name: &str, e: Encryption| {
        let pdf = inkoxide::render(
            &input,
            InputFormat::Json,
            &options(Some(e)),
            &fonts,
            &images,
        )
        .unwrap();
        let path = out_dir.join(format!("{name}.pdf"));
        fs::write(&path, pdf).unwrap();
        path
    };
    let path = write("invoice-permissions", both());
    let o = qpdf(&["--show-encryption", &format!("--password={USER}")], &path);
    let report = String::from_utf8_lossy(&o.stdout);
    for line in [
        "R = 6",
        "stream encryption method: AESv3",
        "string encryption method: AESv3",
        "extract for accessibility: allowed",
        "extract for any purpose: not allowed",
        "print low resolution: allowed",
        "print high resolution: allowed",
        "modify document assembly: not allowed",
        "modify forms: not allowed",
        "modify annotations: not allowed",
        "modify other: not allowed",
    ] {
        assert!(report.contains(line), "no `{line}` in:\n{report}");
    }
    // A wrong password does not open it, and neither does none.
    assert!(!qpdf(&["--check", "--password=wrong"], &path)
        .status
        .success());
    assert!(!qpdf(&["--check"], &path).status.success());
    // With an owner password only, the file opens without one.
    let path = write(
        "invoice-owner-only",
        Encryption::new()
            .with_owner_password(OWNER)
            .with_permissions(Permissions::new()),
    );
    assert!(qpdf(&["--check"], &path).status.success());
    let o = qpdf(&["--show-encryption"], &path);
    assert!(String::from_utf8_lossy(&o.stdout).contains("print high resolution: not allowed"));
}

#[test]
fn encrypted_text_is_not_in_plain_sight() {
    let (fonts, images) = stores();
    let doc = br#"{"version":1,"meta":{"title":"CANARY-TITLE","lang":"en"},
        "fonts":[{"id":"Inter-Regular","family":"Inter"}],
        "pages":[{"size":{"width":200,"height":200},"children":[
          {"type":"text","link":"https://canary-link.example","bookmark":{"title":"CANARY-BOOKMARK"},
           "runs":[{"text":"CANARY","font":"Inter-Regular","size":10}]},
          {"type":"field","field":{"name":"canary_field","kind":"text","font":"Inter-Regular","value":"CANARY-VALUE"}}]}]}"#;
    let render = |options: &RenderOptions| {
        inkoxide::render(doc, InputFormat::Json, options, &fonts, &images).unwrap()
    };
    let plain = render(&options(None).with_compress(false));
    let encrypted = render(&options(Some(both())).with_compress(false));
    let has = |pdf: &[u8], needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    for needle in [
        &b"CANARY-TITLE"[..],
        b"canary-link",
        b"CANARY-BOOKMARK",
        b"canary_field",
        b"CANARY-VALUE",
        // Content streams, written uncompressed here.
        b"\nBT\n",
    ] {
        assert!(has(&plain, needle), "{}", String::from_utf8_lossy(needle));
        assert!(
            !has(&encrypted, needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
    // The passwords are nowhere in the file.
    assert!(!has(&encrypted, USER.as_bytes()) && !has(&encrypted, OWNER.as_bytes()));
    // MessagePack input gives the same bytes.
    let value: serde_json::Value = serde_json::from_slice(doc).unwrap();
    let packed = rmp_serde::to_vec(&value).unwrap();
    let from_msgpack = inkoxide::render(
        &packed,
        InputFormat::MessagePack,
        &options(Some(both())).with_compress(false),
        &fonts,
        &images,
    )
    .unwrap();
    assert!(
        from_msgpack == encrypted,
        "MessagePack output differs from JSON"
    );
}

#[test]
fn bad_encryption_options_name_the_option_and_never_the_password() {
    let (fonts, images) = stores();
    let doc = br#"{"version":1,"pages":[{"size":{"width":10,"height":10}}]}"#;
    let secret = "s3cret-marker";
    let render = |options: RenderOptions| match inkoxide::render(
        doc,
        InputFormat::Json,
        &options,
        &fonts,
        &images,
    ) {
        Err(InkoxideError::Validation(e)) => {
            assert!(!format!("{e} {e:?} {options:?}").contains(secret));
            (e.code.as_str(), e.path)
        }
        other => panic!("{:?}", other.map(|_| ())),
    };
    let pdf_a = RenderOptions::default()
        .with_pdf_a_level(PdfALevel::A2b)
        .with_encryption(Encryption::new().with_user_password(secret));
    assert_eq!(
        render(pdf_a),
        ("INVALID_VALUE", "options.encryption".into())
    );
    for (e, path) in [
        (Encryption::new(), "options.encryption"),
        (
            Encryption::new().with_user_password(format!("{secret}ü")),
            "options.encryption.userPassword",
        ),
        (
            Encryption::new()
                .with_user_password("u")
                .with_owner_password(format!("{secret}{}", "x".repeat(127))),
            "options.encryption.ownerPassword",
        ),
        (
            Encryption::new()
                .with_user_password(secret)
                .with_permissions(Permissions::new()),
            "options.encryption.permissions",
        ),
    ] {
        let options = RenderOptions::default().with_encryption(e);
        assert_eq!(render(options), ("INVALID_VALUE", path.into()));
    }
}
