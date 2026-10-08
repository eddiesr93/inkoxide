//! Every `fixtures/docs/*.json` must validate, render, render
//! deterministically, and (when `qpdf` is installed) pass `qpdf --check`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

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

pub fn stores() -> (FontStore, ImageStore) {
    let mut fonts = FontStore::new();
    for p in sorted_files(&root().join("fixtures/fonts"), &["ttf", "otf"]) {
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        fonts.register(stem, fs::read(&p).unwrap()).unwrap();
    }
    let mut images = ImageStore::new();
    for p in sorted_files(
        &root().join("fixtures/images"),
        &["png", "jpg", "jpeg", "svg"],
    ) {
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        images.insert(stem, fs::read(&p).unwrap());
    }
    (fonts, images)
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

        let opts = RenderOptions {
            creation_date: Some("2026-10-08T00:00:00Z".into()),
            ..Default::default()
        };
        let a = inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let single = RenderOptions {
            threads: Some(1),
            ..opts.clone()
        };
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
        "conșectetur",
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
        r#"{{"version":1,"meta":{{"lang":"ro"}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"Inter-Bold","family":"Inter","weight":700}},{{"id":"NotoSans-Regular","family":"Noto Sans"}},{{"id":"RobotoMono-Regular","family":"Roboto Mono"}}],"pages":[{{"size":{{"width":420,"height":595}},"margin":{{"top":30,"right":30,"bottom":30,"left":30}},"children":[{}]}}]}}"#,
        paras.join(",")
    );
    let render = |threads| {
        let opts = RenderOptions {
            threads: Some(threads),
            ..Default::default()
        };
        inkoxide::render(doc.as_bytes(), InputFormat::Json, &opts, &fonts, &images).unwrap()
    };
    let one = render(1);
    assert!(one == render(3), "3 threads differ from 1");
    assert!(one == render(8), "8 threads differ from 1");
}

/// MessagePack errors carry the same codes and paths as JSON ones.
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
