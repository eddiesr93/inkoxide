//! Every `fixtures/docs/*.json` must validate, render, render
//! deterministically, and (when `qpdf` is installed) pass `qpdf --check`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use inkoxide::{FontStore, HyphenationStore, ImageStore, InputFormat, RenderOptions};

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

/// Patterns registered for one fixture only:
/// `fixtures/hyphenation/<fixture>.<lang>.tex`.
fn hyphenation_for(name: &str) -> HyphenationStore {
    let mut store = HyphenationStore::new();
    for p in sorted_files(&root().join("fixtures/hyphenation"), &["tex"]) {
        let file = p.file_name().unwrap().to_str().unwrap();
        if let Some(lang) = file
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('.'))
            .and_then(|rest| rest.strip_suffix(".tex"))
        {
            store.register(lang, &fs::read(&p).unwrap()).unwrap();
        }
    }
    store
}

fn qpdf_available() -> bool {
    Command::new("qpdf").arg("--version").output().is_ok()
}

#[test]
fn all_fixtures_render() {
    let (fonts, images) = stores();
    let docs = sorted_files(&root().join("fixtures/docs"), &["json"]);
    assert!(!docs.is_empty(), "no fixtures found");
    let out_dir = root().join("target/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    let qpdf = qpdf_available();

    for doc in docs {
        let name = doc.file_stem().unwrap().to_str().unwrap();
        let input = fs::read(&doc).unwrap();
        let errors = inkoxide::validate(&input, InputFormat::Json, &fonts, &images);
        assert!(errors.is_empty(), "{name}: {errors:#?}");

        let opts = RenderOptions::default()
            .with_creation_date("2026-10-08T00:00:00Z")
            .with_hyphenation(hyphenation_for(name));
        let a = inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let single = opts.clone().with_threads(1);
        let b = inkoxide::render(&input, InputFormat::Json, &single, &fonts, &images).unwrap();
        assert!(a == b, "{name}: output is not deterministic");

        // The same document as MessagePack renders the same bytes.
        let value: serde_json::Value = serde_json::from_slice(&input).unwrap();
        let packed = rmp_serde::to_vec(&value).unwrap();
        let c = inkoxide::render(&packed, InputFormat::MessagePack, &opts, &fonts, &images)
            .unwrap_or_else(|e| panic!("{name} (msgpack): {e}"));
        assert!(a == c, "{name}: MessagePack output differs from JSON");

        let out = out_dir.join(format!("{name}.pdf"));
        fs::write(&out, &a).unwrap();
        if qpdf {
            let o = Command::new("qpdf")
                .arg("--check")
                .arg(&out)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "{name}: qpdf --check failed:\n{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
    }
}

/// Every fixture also renders as PDF/A-2b, deterministically. With
/// `VERAPDF` set to a veraPDF command, each output must pass its `2b`
/// profile.
#[test]
fn pdf_a_fixtures() {
    let (fonts, images) = stores();
    let out_dir = root().join("target/fixtures-pdfa");
    fs::create_dir_all(&out_dir).unwrap();
    let mut outputs = Vec::new();
    for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
        let name = doc.file_stem().unwrap().to_str().unwrap();
        let input = fs::read(&doc).unwrap();
        let opts = RenderOptions::default()
            .with_creation_date("2026-10-08T12:30:00+03:00")
            .with_pdf_a(true)
            .with_hyphenation(hyphenation_for(name));
        let a = inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let single = opts.clone().with_threads(1);
        let b = inkoxide::render(&input, InputFormat::Json, &single, &fonts, &images).unwrap();
        assert!(a == b, "{name}: PDF/A output is not deterministic");
        let has = |needle: &[u8]| a.windows(needle.len()).any(|w| w == needle);
        for needle in [
            &b"/GTS_PDFA1"[..],
            b"<pdfaid:part>2</pdfaid:part>",
            b"<xmp:CreateDate>2026-10-08T12:30:00+03:00</xmp:CreateDate>",
            b"/ID [",
        ] {
            assert!(
                has(needle),
                "{name}: no {}",
                String::from_utf8_lossy(needle)
            );
        }
        assert!(!has(b"/DeviceCMYK"), "{name}: DeviceCMYK in PDF/A output");
        let out = out_dir.join(format!("{name}.pdf"));
        fs::write(&out, &a).unwrap();
        outputs.push(out);
    }
    if let Ok(vera) = std::env::var("VERAPDF") {
        let o = Command::new(vera)
            .args(["--flavour", "2b", "--format", "text"])
            .args(&outputs)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&o.stdout);
        let passed = report.lines().filter(|l| l.starts_with("PASS ")).count();
        assert_eq!(passed, outputs.len(), "veraPDF:\n{report}");
    }
}

/// Registered patterns change line breaks only where a paragraph asks for
/// hyphenation in their language, and only that fixture registers them.
#[test]
fn registered_hyphenation_patterns() {
    let (fonts, images) = stores();
    let name = "text-hyphenate-en-patterns";
    let input = fs::read(root().join(format!("fixtures/docs/{name}.json"))).unwrap();
    let store = hyphenation_for(name);
    assert!(store.contains("en-US"));
    let render = |store: HyphenationStore| {
        let opts = RenderOptions::default()
            .with_compress(false)
            .with_hyphenation(store);
        inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images).unwrap()
    };
    let without = render(HyphenationStore::new());
    let with = render(store.clone());
    assert!(with != without, "patterns did not change the layout");
    // Patterns for another language change nothing.
    let mut other = HyphenationStore::new();
    other
        .register("hu", br"\patterns{ 1b 1c 1d 1n 1t }")
        .unwrap();
    assert!(
        render(other) == without,
        "patterns for another language changed the output"
    );
    // The document is `en-US`: patterns for `en` serve it, those for
    // `en-GB` do not.
    let tex = fs::read(root().join(format!("fixtures/hyphenation/{name}.en-us.tex"))).unwrap();
    let under = |lang: &str| {
        let mut s = HyphenationStore::new();
        s.register(lang, &tex).unwrap();
        render(s)
    };
    assert!(under("en") == with, "`en` patterns did not serve `en-US`");
    assert!(under("en-GB") == without, "`en-GB` patterns served `en-US`");
    assert!(render(store) == with, "not deterministic");
}

/// Enough paragraphs for shaping and painting to be split across threads:
/// the output must not depend on the thread count.
#[test]
fn output_is_independent_of_thread_count() {
    let (fonts, images) = stores();
    let words = [
        "Lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet,",
        "façade",
        "ﬁnancial",
        "AVATAR",
        "Wave",
        "Tokyo",
    ];
    let mut paras = Vec::new();
    for i in 0..600 {
        let text: Vec<&str> = (0..12 + i % 17)
            .map(|k| words[(i * 7 + k * 3) % words.len()])
            .collect();
        let font = [
            "Inter-Regular",
            "Inter-Bold",
            "NotoSans-Regular",
            "RobotoMono-Regular",
        ][i % 4];
        paras.push(format!(
            r#"{{"type":"text","style":{{"textAlign":"justify","hyphenate":true}},"runs":[{{"text":"{} {i}","font":"{font}","size":9}}]}}"#,
            text.join(" ")
        ));
    }
    let doc = format!(
        r#"{{"version":1,"meta":{{"lang":"en-US"}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"Inter-Bold","family":"Inter","weight":700}},{{"id":"NotoSans-Regular","family":"Noto Sans"}},{{"id":"RobotoMono-Regular","family":"Roboto Mono"}}],"pages":[{{"size":{{"width":420,"height":595}},"margin":{{"top":30,"right":30,"bottom":30,"left":30}},"children":[{}]}}]}}"#,
        paras.join(",")
    );
    let render = |threads| {
        let opts = RenderOptions::default().with_threads(threads);
        inkoxide::render(doc.as_bytes(), InputFormat::Json, &opts, &fonts, &images).unwrap()
    };
    let one = render(1);
    assert!(one == render(3), "3 threads differ from 1");
    assert!(one == render(8), "8 threads differ from 1");
}

/// `MessagePack` errors carry the same codes and paths as JSON ones.
#[test]
fn msgpack_errors_match_json() {
    let (fonts, images) = stores();
    let cases = [
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","colour":1}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100},"children":[]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"flexDirection":"diagonal"}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"spacer","style":{"width":"wide"}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":-5,"height":100},"children":[]}]}"#,
        r#"{"version":2,"fonts":[],"pages":[]}"#,
        // Versions that are not the integer 1.
        r#"{"version":"1","fonts":[],"pages":[]}"#,
        r#"{"version":2.5,"fonts":[],"pages":[]}"#,
        r#"{"version":true,"fonts":[],"pages":[]}"#,
        r#"{"version":null,"fonts":[],"pages":[]}"#,
        r#"{"fonts":[],"pages":[]}"#,
        // A transform function with two keys or none, arrays too long.
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"transform":[{"translate":[1,2],"rotate":5}]}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"transform":[{}]}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"transform":[{"spin":5}]}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"backgroundColor":[0,0,0,1,1]}}]}]}"#,
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[{"type":"box","style":{"transformOrigin":[1,2,3]}}]}]}"#,
    ];
    for json in cases {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let packed = rmp_serde::to_vec(&value).unwrap();
        let a = inkoxide::validate(json.as_bytes(), InputFormat::Json, &fonts, &images);
        let b = inkoxide::validate(&packed, InputFormat::MessagePack, &fonts, &images);
        let key = |v: &[inkoxide::ValidationError]| {
            v.iter()
                .map(|e| (e.code.as_str(), e.path.clone()))
                .collect::<Vec<_>>()
        };
        assert!(!a.is_empty(), "{json}");
        assert_eq!(key(&a), key(&b), "{json}\n{b:#?}");
    }
    // Garbage and trailing bytes are parse errors.
    let ok: serde_json::Value = serde_json::from_str(
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[]}]}"#,
    )
    .unwrap();
    let mut trailing = rmp_serde::to_vec(&ok).unwrap();
    assert!(inkoxide::validate(&trailing, InputFormat::MessagePack, &fonts, &images).is_empty());
    trailing.push(0);
    for bad in [&b"\xc1"[..], &trailing[..]] {
        let e = inkoxide::validate(bad, InputFormat::MessagePack, &fonts, &images);
        assert_eq!(e[0].code.as_str(), "PARSE_ERROR", "{e:?}");
    }
}

/// Colour emoji are images, but extract as their text (`ActualText`):
/// checked with poppler's `pdftotext` when it is installed.
#[test]
fn colour_emoji_extract_as_text() {
    if Command::new("pdftotext").arg("-v").output().is_err() {
        return;
    }
    let (fonts, images) = stores();
    let dir = root().join("target/fixtures-text");
    fs::create_dir_all(&dir).unwrap();
    for (name, want) in [
        (
            "emoji-colr",
            "thumbs 👍 👍🏽 flag 🇫🇷 keycap 1\u{FE0F}\u{20E3} coder",
        ),
        (
            "emoji-cbdt",
            "smile 😀 heart ❤\u{FE0F} thumbs 👍 flag 🇫🇷 end",
        ),
    ] {
        let input = fs::read(root().join(format!("fixtures/docs/{name}.json"))).unwrap();
        let pdf = inkoxide::render(
            &input,
            InputFormat::Json,
            &RenderOptions::default(),
            &fonts,
            &images,
        )
        .unwrap();
        let file = dir.join(format!("{name}.pdf"));
        fs::write(&file, pdf).unwrap();
        let out = Command::new("pdftotext")
            .args(["-enc", "UTF-8"])
            .arg(&file)
            .arg("-")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains(want), "{name}: {text}");
    }
}
