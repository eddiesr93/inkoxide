//! The output of every fixture, untagged and tagged, is pinned by its
//! SHA-256 in `output_hashes.sha256`: a change to what the engine writes
//! shows here, and is either a bug or blessed on purpose with
//! `INKOXIDE_BLESS=1 cargo test --test output_hashes`, which rewrites the
//! file. The fixtures whose numbers depend on the platform's math library
//! (`fixtures/wasm-parity.json`) are left out.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use common::{hyphenation_for, root, sorted_files, stores};
use inkoxide::{InputFormat, RenderOptions};
use sha2::Digest as _;

fn pinned() -> PathBuf {
    root().join("crates/core/tests/output_hashes.sha256")
}

/// `<hash>  <name>` lines; a tagged render's name ends in `.tagged`.
fn read_pinned() -> BTreeMap<String, String> {
    fs::read_to_string(pinned())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once("  "))
        .map(|(hash, name)| (name.to_string(), hash.to_string()))
        .collect()
}

#[test]
fn every_fixture_renders_as_pinned() {
    let (fonts, images) = stores();
    let parity: serde_json::Value =
        serde_json::from_slice(&fs::read(root().join("fixtures/wasm-parity.json")).unwrap())
            .unwrap();
    let platform: Vec<&str> = parity["docs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let mut hashes = BTreeMap::new();
    for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
        let name = doc.file_stem().unwrap().to_str().unwrap();
        if platform.contains(&name) {
            continue;
        }
        let input = fs::read(&doc).unwrap();
        let options = RenderOptions::default()
            .with_creation_date("2026-10-08T00:00:00Z")
            .with_hyphenation(hyphenation_for(name));
        for (tagged, key) in [(false, name.to_string()), (true, format!("{name}.tagged"))] {
            let options = options.clone().with_tagged(tagged);
            let pdf = inkoxide::render(&input, InputFormat::Json, &options, &fonts, &images)
                .unwrap_or_else(|e| panic!("{key}: {e}"));
            let hash = sha2::Sha256::digest(&pdf)
                .iter()
                .fold(String::new(), |mut s, b| {
                    let _ = write!(s, "{b:02x}");
                    s
                });
            hashes.insert(key, hash);
        }
    }
    if std::env::var_os("INKOXIDE_BLESS").is_some() {
        let lines = hashes.iter().fold(String::new(), |mut s, (name, hash)| {
            let _ = writeln!(s, "{hash}  {name}");
            s
        });
        fs::write(pinned(), lines).unwrap();
        return;
    }
    let pinned = read_pinned();
    let changed: Vec<&str> = hashes
        .keys()
        .chain(pinned.keys())
        .filter(|k| hashes.get(*k) != pinned.get(*k))
        .map(String::as_str)
        .collect();
    assert!(
        changed.is_empty(),
        "output changed for {changed:?}; if on purpose, bless it: \
         INKOXIDE_BLESS=1 cargo test -p inkoxide --test output_hashes"
    );
}
