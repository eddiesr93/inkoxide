//! `cargo bench -p inkoxide` — renders each fixture end to end.
//! Record results in docs/BENCHMARKS.md when they change meaningfully.

use std::fs;
use std::path::Path;

use criterion::{criterion_group, criterion_main, Criterion};
use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

fn stores() -> (FontStore, ImageStore) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut fonts = FontStore::new();
    let mut entries: Vec<_> = fs::read_dir(root.join("fonts"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.extension().is_some_and(|e| e == "ttf" || e == "otf") {
            let id = p.file_stem().unwrap().to_str().unwrap().to_string();
            fonts.register(id, fs::read(&p).unwrap()).unwrap();
        }
    }
    let mut images = ImageStore::new();
    if let Ok(rd) = fs::read_dir(root.join("images")) {
        for p in rd.flatten().map(|e| e.path()) {
            let id = p.file_stem().unwrap().to_str().unwrap().to_string();
            images.insert(id, fs::read(&p).unwrap());
        }
    }
    (fonts, images)
}

fn bench(c: &mut Criterion) {
    let (fonts, images) = stores();
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docs");
    let mut names: Vec<_> = fs::read_dir(&docs)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    names.sort();
    let opts = RenderOptions::default();
    for path in names
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let input = fs::read(&path).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        c.bench_function(&format!("render/{name}"), |b| {
            b.iter(|| inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images).unwrap())
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
