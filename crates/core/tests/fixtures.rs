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
    for p in sorted_files(&root().join("fixtures/images"), &["png", "jpg", "jpeg"]) {
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
        let b = inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images).unwrap();
        assert!(a == b, "{name}: output is not deterministic");

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
