//! What the integration tests share.

#![allow(dead_code, unused_imports)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use inkoxide::{FontStore, ImageStore};

#[path = "../../src/test_timing.rs"]
mod timing;

pub(crate) use timing::{assert_as_fast, assert_bounded, assert_linear};

/// The repository root.
pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The files of `dir` with one of the extensions `ext`, sorted.
pub(crate) fn sorted_files(dir: &Path, ext: &[&str]) -> Vec<PathBuf> {
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

/// Every font and image of `fixtures/`, each under its file name
/// without the extension.
pub(crate) fn stores() -> (FontStore, ImageStore) {
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

/// Whether `tool` runs (with `arg`). CI installs every tool these tests
/// use, so there (`CI` set) a missing one fails the test instead of
/// skipping its checks.
pub(crate) fn installed(tool: &str, arg: &str) -> bool {
    let found = Command::new(tool).arg(arg).output().is_ok();
    assert!(
        found || std::env::var_os("CI").is_none(),
        "{tool} is not installed"
    );
    found
}

pub(crate) fn qpdf_available() -> bool {
    installed("qpdf", "--version")
}
