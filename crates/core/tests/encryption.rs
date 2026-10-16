//! Encrypted output. With `qpdf` installed, every fixture, encrypted,
//! opens with its passwords, and qpdf's view of it, decrypted, is the
//! unencrypted render object for object: a string or stream left in plain
//! text would differ. Only font widths differ: encrypted, a font lists
//! those of all its glyphs.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{qpdf_available, root, sorted_files, stores};
use inkoxide::{
    Encryption, ImageStore, InkoxideError, InputFormat, PdfALevel, Permissions, Printing,
    RenderOptions,
};

fn qpdf(args: &[&str], file: &Path) -> Output {
    Command::new("qpdf").args(args).arg(file).output().unwrap()
}

/// qpdf's JSON view of `file` (decrypted with `password`): its objects,
/// streams decoded, without the `Encrypt` dictionary and the trailer, and
/// without the catalog's `/Extensions`, which encryption adds. Radio
/// buttons are seen by position ([`radios_by_position`]), as encryption
/// writes them.
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
    radios_by_position(&mut objects);
    for v in objects.values_mut() {
        for key in ["/BaseFont", "/FontName"] {
            if let Some(name) = v["value"].get_mut(key) {
                *name = untagged(name.as_str().unwrap()).into();
            }
        }
    }
    objects
}

/// Font name `name` without its subset tag (`/ABCDEF+Name`): an
/// encrypted file keys the tag, so it differs from the plain one.
fn untagged(name: &str) -> String {
    match name.as_bytes() {
        [b'/', tag @ ..]
            if tag.len() > 7 && tag[..6].iter().all(u8::is_ascii_uppercase) && tag[6] == b'+' =>
        {
            format!("/TAG+{}", &name[8..])
        }
        _ => name.to_string(),
    }
}

/// The subset tags of font names in `pdf`'s bytes.
fn subset_tags(pdf: &[u8]) -> Vec<Vec<u8>> {
    pdf.windows(8)
        .filter(|w| w[0] == b'/' && w[1..7].iter().all(u8::is_ascii_uppercase) && w[7] == b'+')
        .map(|w| w[1..7].to_vec())
        .collect()
}

/// Rewrites every radio button field of qpdf's JSON view `objects` the
/// way an encrypted file writes it: each button's on state is the
/// position among the kids of the first button with the same value, and
/// `/Opt` holds the values, which plain output may give as the state
/// names instead.
fn radios_by_position(objects: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::Value;
    const RADIO: u64 = 1 << 15;
    let radios: Vec<String> = objects
        .iter()
        .filter(|(_, o)| {
            o["value"]["/FT"] == "/Btn" && o["value"]["/Ff"].as_u64().unwrap_or(0) & RADIO != 0
        })
        .map(|(k, _)| k.clone())
        .collect();
    for radio in radios {
        let kids: Vec<String> = objects[&radio]["value"]["/Kids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| format!("obj:{}", k.as_str().unwrap()))
            .collect();
        let mut renamed = std::collections::HashMap::new();
        let mut values = Vec::new();
        for (i, kid) in kids.iter().enumerate() {
            let value = &mut objects.get_mut(kid).unwrap()["value"];
            let normal = value["/AP"]["/N"].as_object_mut().unwrap();
            let on = normal.keys().find(|k| *k != "/Off").unwrap().clone();
            let appearance = normal.remove(&on).unwrap();
            let position = renamed.get(&on).cloned().unwrap_or_else(|| format!("/{i}"));
            normal.insert(position.clone(), appearance);
            if value["/AS"] == on.as_str() {
                value["/AS"] = Value::from(position.clone());
            }
            values.push(Value::from(format!("u:{}", name_text(&on))));
            renamed.entry(on).or_insert(position);
        }
        let field = objects.get_mut(&radio).unwrap()["value"]
            .as_object_mut()
            .unwrap();
        if let Some(on) = field["/V"].as_str().and_then(|v| renamed.get(v)) {
            field["/V"] = Value::from(on.clone());
        }
        field.entry("/Opt").or_insert(Value::from(values));
    }
}

/// The text of PDF name `name` as qpdf's JSON writes it (`/a#20b`).
fn name_text(name: &str) -> String {
    let mut bytes = Vec::new();
    let mut rest = name.strip_prefix('/').unwrap().as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'#' && tail.len() >= 2 {
            let hex = std::str::from_utf8(&tail[..2]).unwrap();
            bytes.push(u8::from_str_radix(hex, 16).unwrap());
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    String::from_utf8(bytes).unwrap()
}

/// The widths of every `CIDFont` in qpdf's JSON view `objects`, by
/// object: its `/DW` and the width of each CID its `/W` lists. Both are
/// taken out of `objects`: an encrypted file lists the widths of every
/// glyph of a font, a plain one those of the glyphs it uses.
fn take_widths(
    objects: &mut serde_json::Map<String, serde_json::Value>,
) -> BTreeMap<String, (f64, BTreeMap<i64, f64>)> {
    let mut fonts = BTreeMap::new();
    for (k, v) in objects.iter_mut() {
        if !matches!(
            v["value"]["/Subtype"].as_str(),
            Some("/CIDFontType0" | "/CIDFontType2")
        ) {
            continue;
        }
        let dict = v["value"].as_object_mut().unwrap();
        let default = dict.remove("/DW").map_or(1000.0, |w| w.as_f64().unwrap());
        let list = dict.remove("/W").unwrap_or_default();
        let list = list.as_array().map(Vec::as_slice).unwrap_or_default();
        let mut widths = BTreeMap::new();
        let mut i = 0;
        while i < list.len() {
            let first = list[i].as_i64().unwrap();
            if let Some(each) = list[i + 1].as_array() {
                for (cid, w) in (first..).zip(each) {
                    widths.insert(cid, w.as_f64().unwrap());
                }
                i += 2;
            } else {
                let last = list[i + 1].as_i64().unwrap();
                let w = list[i + 2].as_f64().unwrap();
                widths.extend((first..=last).map(|cid| (cid, w)));
                i += 3;
            }
        }
        fonts.insert(k.clone(), (default, widths));
    }
    fonts
}

/// qpdf's view of `encrypted`, decrypted, is that of `plain` but for font
/// widths, and each glyph whose width `plain` lists has that width in
/// `encrypted` too.
fn assert_decrypts_to(encrypted: &Path, plain: &Path, name: &str) {
    let mut decrypted = objects(encrypted, Some(USER));
    let mut plain = objects(plain, None);
    let decrypted_widths = take_widths(&mut decrypted);
    let plain_widths = take_widths(&mut plain);
    assert_eq!(
        decrypted_widths.keys().collect::<Vec<_>>(),
        plain_widths.keys().collect::<Vec<_>>(),
        "{name}: not the same fonts"
    );
    for (font, (_, used)) in &plain_widths {
        let (default, all) = &decrypted_widths[font];
        for (cid, w) in used {
            let got = all.get(cid).unwrap_or(default);
            assert!(
                (got - w).abs() < 1e-3,
                "{name}: CID {cid} of {font} is {got} wide, not {w}"
            );
        }
    }
    assert_eq!(
        decrypted, plain,
        "{name}: decrypted objects differ from the plain render"
    );
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
        // Font names are not encrypted, so their subset tags must not be
        // the plain ones, made from the glyph set.
        for tag in subset_tags(&plain) {
            assert!(
                !subset_tags(&encrypted).contains(&tag),
                "{name}: an encrypted font name keeps its plain subset tag"
            );
        }
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
        assert_decrypts_to(&encrypted_path, &plain_path, name);
    }
}

/// Form scripts, field groups, list boxes, notes and radio buttons
/// repeated on every page, which the fixtures do not all have, decrypt to
/// the plain render too.
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
          {"type":"note","note":{"contents":"Check (this) — Ünïcode","author":"Ana"}},
          {"type":"field","break":{"fixed":true},"field":{"name":"copy","kind":"radio","value":"yes","checked":true}},
          {"type":"field","field":{"name":"copy","kind":"radio","value":"no"}},
          {"type":"box","style":{"height":400}}]}]}"#
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
    for needle in [&b"/JS"[..], b"/Opt", b"/Contents", b"/I [", b"/AS /yes"] {
        assert!(
            plain_text.windows(needle.len()).any(|w| w == needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
    assert_decrypts_to(&encrypted, &plain, "forms-notes");
    // The radio button repeated on both pages is one state, on in both,
    // as in the plain file.
    let o = qpdf(
        &["--json=2", "--json-key=qpdf", &format!("--password={USER}")],
        &encrypted,
    );
    let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let objects = json["qpdf"][1].as_object().unwrap();
    let field = objects
        .values()
        .map(|o| &o["value"])
        .find(|v| v["/T"] == "u:copy")
        .unwrap();
    let mut states: Vec<&str> = field["/Kids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kid| {
            objects[&format!("obj:{}", kid.as_str().unwrap())]["value"]["/AS"]
                .as_str()
                .unwrap()
        })
        .collect();
    // The repeated button's widgets, then the other button's.
    states.sort_unstable();
    let on = field["/V"].as_str().unwrap();
    assert!(states.len() > 2, "{states:?}");
    assert!(states[..states.len() - 1].iter().all(|s| *s == on));
    assert_eq!(states.last(), Some(&"/Off"));
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
          {"type":"field","field":{"name":"canary_field","kind":"text","font":"Inter-Regular","value":"CANARY-VALUE"}},
          {"type":"field","field":{"name":"r","kind":"radio","value":"CANARY-RADIO-A","checked":true}},
          {"type":"field","field":{"name":"r","kind":"radio","value":"CANARY-RADIO-B"}}]}]}"#;
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
        // Radio buttons' values, which name their states in plain output.
        b"CANARY-RADIO-A",
        b"CANARY-RADIO-B",
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
    #[cfg(feature = "msgpack")]
    {
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
}

/// Field values and options are strings, so encrypted; the state PDF
/// writes as names and numbers stays readable, as SECURITY.md says:
/// checkbox states and the positions of selected list options.
#[test]
fn encrypted_field_values_are_strings_and_states_are_names() {
    let (fonts, images) = stores();
    let doc = br#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
        "pages":[{"size":{"width":300,"height":300},"children":[
          {"type":"field","field":{"name":"agree","kind":"checkbox","checked":true}},
          {"type":"field","style":{"height":20},"field":{"name":"pick","kind":"select","font":"Inter-Regular",
           "options":["CANARY-SELECT-A","CANARY-SELECT-B"],"value":"CANARY-SELECT-B"}},
          {"type":"field","style":{"height":60},"field":{"name":"list","kind":"list","font":"Inter-Regular",
           "options":["CANARY-LIST-A","CANARY-LIST-B"],"value":"CANARY-LIST-B"}}]}]}"#;
    let render = |options: &RenderOptions| {
        inkoxide::render(doc, InputFormat::Json, options, &fonts, &images).unwrap()
    };
    let plain = render(&options(None).with_compress(false));
    let encrypted = render(&options(Some(both())).with_compress(false));
    let has = |pdf: &[u8], needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    for needle in [
        &b"CANARY-SELECT-A"[..],
        b"CANARY-SELECT-B",
        b"CANARY-LIST-A",
        b"CANARY-LIST-B",
    ] {
        assert!(has(&plain, needle), "{}", String::from_utf8_lossy(needle));
        assert!(
            !has(&encrypted, needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
    for needle in [&b"/V /Yes"[..], b"/AS /Yes", b"/I [1]"] {
        assert!(has(&plain, needle), "{}", String::from_utf8_lossy(needle));
        assert!(
            has(&encrypted, needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
}

/// The `/DW` and `/W` entries of each `CIDFont` in `pdf`, as written.
fn width_entries(pdf: &[u8]) -> Vec<String> {
    let find = |from: usize, needle: &[u8]| {
        pdf[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| from + i)
    };
    let mut entries = Vec::new();
    let mut at = 0;
    while let Some(start) = find(at, b"/DW ") {
        let open = find(start, b"/W [").unwrap() + 3;
        let mut depth = 0;
        let end = (open..pdf.len())
            .find(|&i| {
                match pdf[i] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .unwrap();
        entries.push(String::from_utf8_lossy(&pdf[start..=end]).into_owned());
        at = end;
    }
    entries
}

/// Font dictionaries are not encrypted, so the widths an encrypted file
/// gives a font are those of all its glyphs, whatever the text: TrueType
/// and CFF fonts alike.
#[test]
fn encrypted_font_widths_do_not_depend_on_the_text() {
    let (fonts, images) = stores();
    let doc = |text: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},
            {{"id":"InterDisplay-Bold","family":"Inter Display","weight":700}}],
            "pages":[{{"size":{{"width":300,"height":300}},"children":[
              {{"type":"text","runs":[{{"text":"{text}","font":"Inter-Regular","size":10}},
               {{"text":" {text}","font":"InterDisplay-Bold","size":10}}]}}]}}]}}"#
        )
    };
    let render = |text: &str, options: &RenderOptions| {
        inkoxide::render(
            doc(text).as_bytes(),
            InputFormat::Json,
            options,
            &fonts,
            &images,
        )
        .unwrap()
    };
    let (a, b) = ("Pay 120 to Ana", "Owe 9 to Zoë");
    let plain = options(None);
    let encrypted = options(Some(both()));
    let plain_a = width_entries(&render(a, &plain));
    assert_eq!(plain_a.len(), 2);
    assert_ne!(plain_a, width_entries(&render(b, &plain)));
    let encrypted_a = width_entries(&render(a, &encrypted));
    assert_eq!(encrypted_a.len(), 2);
    assert_eq!(encrypted_a, width_entries(&render(b, &encrypted)));
    if !qpdf_available() {
        return;
    }
    let out_dir = root().join("target/fixtures-encrypted");
    fs::create_dir_all(&out_dir).unwrap();
    for (name, text) in [("widths-a", a), ("widths-b", b)] {
        let (plain_path, encrypted_path) = (
            out_dir.join(format!("{name}.plain.pdf")),
            out_dir.join(format!("{name}.pdf")),
        );
        fs::write(&plain_path, render(text, &plain)).unwrap();
        fs::write(&encrypted_path, render(text, &encrypted)).unwrap();
        assert_decrypts_to(&encrypted_path, &plain_path, name);
    }
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
        (
            Encryption::new()
                .with_user_password(secret)
                .with_owner_password(secret)
                .with_permissions(Permissions::new()),
            "options.encryption.permissions",
        ),
    ] {
        let options = RenderOptions::default().with_encryption(e);
        assert_eq!(render(options), ("INVALID_VALUE", path.into()));
    }
}

/// The keys and the file identifier depend on everything the document
/// puts in the file: documents that differ only in an image's bytes, a
/// link, a form field, a note or a bookmark get different ones.
#[test]
fn every_part_of_the_document_changes_the_key() {
    let (fonts, _) = stores();
    let doc = |link: &str, value: &str, note: &str, bookmark: &str| {
        format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":300,"height":300}},"children":[
              {{"type":"text","link":"{link}","bookmark":{{"title":"{bookmark}"}},
               "runs":[{{"text":"Same text","font":"Inter-Regular","size":10}}]}},
              {{"type":"image","src":"pic","style":{{"width":50,"height":50}}}},
              {{"type":"field","field":{{"name":"f","kind":"text","font":"Inter-Regular","value":"{value}"}}}},
              {{"type":"note","note":{{"contents":"{note}"}}}}]}}]}}"#
        )
    };
    let photo = fs::read(root().join("fixtures/images/photo.jpg")).unwrap();
    // The same JPEG with one byte of its compressed data changed: it is
    // embedded as it is, so only the image stream differs.
    let mut changed = photo.clone();
    let mid = changed.len() / 2;
    changed[mid] ^= 0x01;
    let render = |doc: String, image: &[u8]| {
        let mut images = ImageStore::new();
        images.insert("pic", image.to_vec());
        let pdf = inkoxide::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options(Some(both())),
            &fonts,
            &images,
        )
        .unwrap();
        let find = |from: &[u8], to: &[u8]| {
            let start = pdf.windows(from.len()).rposition(|w| w == from).unwrap();
            let len = pdf[start..]
                .windows(to.len())
                .position(|w| w == to)
                .unwrap();
            pdf[start..start + len].to_vec()
        };
        // The file identifier, and the `Encrypt` dictionary (salts and
        // wrapped keys).
        (find(b"/ID [", b"]"), find(b"/Filter /Standard", b"endobj"))
    };
    let base = render(doc("https://a.example", "v", "n", "b"), &photo);
    assert_eq!(
        base,
        render(doc("https://a.example", "v", "n", "b"), &photo)
    );
    for (what, other) in [
        (
            "image bytes",
            render(doc("https://a.example", "v", "n", "b"), &changed),
        ),
        (
            "link",
            render(doc("https://b.example", "v", "n", "b"), &photo),
        ),
        (
            "field",
            render(doc("https://a.example", "w", "n", "b"), &photo),
        ),
        (
            "note",
            render(doc("https://a.example", "v", "m", "b"), &photo),
        ),
        (
            "bookmark",
            render(doc("https://a.example", "v", "n", "c"), &photo),
        ),
    ] {
        assert_ne!(other.0, base.0, "{what}: the same /ID");
        assert_ne!(other.1, base.1, "{what}: the same salts and keys");
    }
}

/// Encrypted, radio buttons name their states by position, and their
/// values are the strings of `/Opt`, which qpdf decrypts.
#[test]
fn encrypted_radio_values_are_strings() {
    if !qpdf_available() {
        return;
    }
    let (fonts, images) = stores();
    let doc = br#"{"version":1,"pages":[{"size":{"width":200,"height":200},"children":[
          {"type":"field","field":{"name":"size","kind":"radio","value":"small"}},
          {"type":"field","field":{"name":"size","kind":"radio","value":"Grande (L)","checked":true}}]}]}"#;
    let out_dir = root().join("target/fixtures-encrypted");
    fs::create_dir_all(&out_dir).unwrap();
    let pdf = inkoxide::render(
        doc,
        InputFormat::Json,
        &options(Some(both())),
        &fonts,
        &images,
    )
    .unwrap();
    let path = out_dir.join("radio.pdf");
    fs::write(&path, pdf).unwrap();
    assert!(qpdf(&["--check", &format!("--password={USER}")], &path)
        .status
        .success());
    let o = qpdf(
        &["--json=2", "--json-key=qpdf", &format!("--password={USER}")],
        &path,
    );
    let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let objects = json["qpdf"][1].as_object().unwrap();
    let field = objects
        .values()
        .map(|o| &o["value"])
        .find(|v| v["/T"] == "u:size")
        .unwrap();
    assert_eq!(field["/V"], "/1");
    assert_eq!(
        field["/Opt"],
        serde_json::json!(["u:small", "u:Grande (L)"])
    );
    for (i, kid) in field["/Kids"].as_array().unwrap().iter().enumerate() {
        let kid = &objects[&format!("obj:{}", kid.as_str().unwrap())]["value"];
        let states: Vec<&String> = kid["/AP"]["/N"].as_object().unwrap().keys().collect();
        assert_eq!(states, [&format!("/{i}"), &"/Off".to_string()]);
    }
}
