//! Every `fixtures/docs/*.json` must validate, render, render
//! deterministically, and (when `qpdf` is installed) pass `qpdf --check`.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use common::{installed, qpdf_available, root, sorted_files, stores};
use inkoxide::{
    HyphenationStore, InkoxideError, InputFormat, PdfALevel, RenderOptions, ValidationCode,
};

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
        #[cfg(feature = "msgpack")]
        {
            let value: serde_json::Value = serde_json::from_slice(&input).unwrap();
            let packed = rmp_serde::to_vec(&value).unwrap();
            let c = inkoxide::render(&packed, InputFormat::MessagePack, &opts, &fonts, &images)
                .unwrap_or_else(|e| panic!("{name} (msgpack): {e}"));
            assert!(a == c, "{name}: MessagePack output differs from JSON");
        }

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

/// The fixtures PDF/A-1b rejects, because they need transparency, and the
/// path of the first thing that does.
const NOT_PDF_A_1B: &[(&str, &str)] = &[
    (
        "text-inline-images",
        "pages[0].children[0].runs[1].image.src",
    ),
    ("emoji-images", "emojiImages.❤\u{FE0F}"),
    ("emoji-colr", "pages[0].children[3].runs[1].color"),
    ("emoji-cbdt", "pages[0].children[1].runs[0]"),
    (
        "background-image",
        "pages[0].children[2].children[0].children[1].backgroundImage.src",
    ),
    ("debug-text", "pages[0].children[2].debug"),
    ("floats", "pages[0].children[1].children[1].debug"),
    (
        "boxes-nested",
        "pages[0].children[0].children[0].style.backgroundColor",
    ),
    (
        "header-footer-pagenumbers",
        "pages[0].children[0].children[0].runs[0].color",
    ),
    (
        "images-gif-webp",
        "pages[0].children[1].children[0].children[0].src",
    ),
    (
        "images-jpeg-png",
        "pages[0].children[3].children[1].children[1].src",
    ),
    (
        "paint-transforms",
        "pages[0].children[1].children[11].children[0].children[0].children[1].style.opacity",
    ),
    ("svg", "pages[0].children[1].src"),
    // Text in a clip path is a soft mask.
    ("svg-text", "pages[0].children[1].src"),
    (
        "text-super-sub-features",
        "pages[0].children[6].children[1].debug",
    ),
];

/// Every fixture also renders at every PDF/A level, deterministically,
/// except for those PDF/A-1b rejects ([`NOT_PDF_A_1B`]). With `VERAPDF`
/// set to a veraPDF command, each output must pass the profile of its
/// level.
#[test]
fn pdf_a_fixtures() {
    let (fonts, images) = stores();
    for (level, part, version) in [
        (PdfALevel::A1b, 1, "1.4"),
        (PdfALevel::A2b, 2, "1.7"),
        (PdfALevel::A3b, 3, "1.7"),
    ] {
        let name = level.as_str();
        let out_dir = root().join(format!("target/fixtures-pdfa-{name}"));
        fs::create_dir_all(&out_dir).unwrap();
        let mut outputs = Vec::new();
        for doc in sorted_files(&root().join("fixtures/docs"), &["json"]) {
            let fixture = doc.file_stem().unwrap().to_str().unwrap();
            let name = format!("{fixture} ({name})");
            let input = fs::read(&doc).unwrap();
            let opts = RenderOptions::default()
                .with_creation_date("2026-10-08T12:30:00+03:00")
                .with_pdf_a_level(level)
                .with_hyphenation(hyphenation_for(fixture));
            let rendered = inkoxide::render(&input, InputFormat::Json, &opts, &fonts, &images);
            let rejected = NOT_PDF_A_1B.iter().find(|(f, _)| *f == fixture);
            if let (PdfALevel::A1b, Some((_, path))) = (level, rejected) {
                match rendered {
                    Err(InkoxideError::Validation(e)) => {
                        assert_eq!(e.code, ValidationCode::PdfaConflict, "{name}: {e}");
                        assert_eq!(e.path, *path, "{name}: {e}");
                    }
                    other => panic!("{name}: not rejected: {:?}", other.map(|_| ())),
                }
                continue;
            }
            let a = rendered.unwrap_or_else(|e| panic!("{name}: {e}"));
            let single = opts.clone().with_threads(1);
            let b = inkoxide::render(&input, InputFormat::Json, &single, &fonts, &images).unwrap();
            assert!(a == b, "{name}: PDF/A output is not deterministic");
            let has = |needle: &[u8]| a.windows(needle.len()).any(|w| w == needle);
            let part = format!("<pdfaid:part>{part}</pdfaid:part>");
            let header = format!("%PDF-{version}");
            assert!(a.starts_with(header.as_bytes()), "{name}: not {header}");
            for needle in [
                &b"/GTS_PDFA1"[..],
                part.as_bytes(),
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
            // PDF/A-1 font subsets list their CIDs; the others need none.
            let subset = a
                .windows(18)
                .any(|w| w.starts_with(b"/FontName /") && w[17] == b'+');
            assert_eq!(
                has(b"/CIDSet"),
                subset && level == PdfALevel::A1b,
                "{name}: CIDSet"
            );
            let out = out_dir.join(format!("{fixture}.pdf"));
            fs::write(&out, &a).unwrap();
            outputs.push(out);
        }
        verapdf(name, &outputs);
    }
}

/// With `VERAPDF` set, every one of `outputs` passes veraPDF's profile
/// for `flavour` (`1b`, `2b`, `3b`). CI installs veraPDF and sets it.
fn verapdf(flavour: &str, outputs: &[PathBuf]) {
    let vera = std::env::var("VERAPDF");
    assert!(
        vera.is_ok() || std::env::var_os("CI").is_none(),
        "VERAPDF is not set"
    );
    if let Ok(vera) = vera {
        let o = Command::new(vera)
            .args(["--flavour", flavour, "--format", "text"])
            .args(outputs)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&o.stdout);
        let passed = report.lines().filter(|l| l.starts_with("PASS ")).count();
        assert_eq!(passed, outputs.len(), "veraPDF ({flavour}):\n{report}");
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

/// Without the `msgpack` feature, `MessagePack` input is a parse error.
#[cfg(not(feature = "msgpack"))]
#[test]
fn msgpack_is_a_parse_error_without_its_feature() {
    let (fonts, images) = stores();
    let ok: serde_json::Value = serde_json::from_str(
        r#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[]}]}"#,
    )
    .unwrap();
    let packed = rmp_serde::to_vec(&ok).unwrap();
    let e = inkoxide::validate(&packed, InputFormat::MessagePack, &fonts, &images);
    assert_eq!(e[0].code.as_str(), "PARSE_ERROR", "{e:?}");
    assert!(e[0].message.contains("JSON only"), "{e:?}");
}

/// `MessagePack` errors carry the same codes and paths as JSON ones.
#[cfg(feature = "msgpack")]
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

/// Colour emoji are images, but extract as their text (`ActualText`),
/// images, inline or not, as their alternative text, and form fields as the
/// text they show: checked with poppler's `pdftotext` when it is installed.
#[test]
fn colour_emoji_and_images_extract_as_text() {
    if !installed("pdftotext", "-v") {
        return;
    }
    let (fonts, images) = stores();
    let dir = root().join("target/fixtures-text");
    fs::create_dir_all(&dir).unwrap();
    for (name, want) in [
        (
            "emoji-colr",
            &["thumbs 👍 👍🏽 flag 🇫🇷 keycap 1\u{FE0F}\u{20E3} coder"][..],
        ),
        (
            "emoji-cbdt",
            &[
                "smile 😀 heart ❤\u{FE0F} thumbs 👍 flag 🇫🇷 end",
                // In a run whose own font has no outlines.
                "😀❤\u{FE0F}👍🇫🇷",
                "😀 ❤\u{FE0F} 👍 🇫🇷",
            ],
        ),
        ("emoji-images", &["smile 😀 flag 🇫🇷 heart ❤\u{FE0F}"]),
        ("text-inline-images", &["Logo"]),
        ("images-jpeg-png", &["Harbour photo"]),
        // The text form fields show, from their appearances.
        ("forms", &["Chloë Dubois-Müller", "No. 2026/0815"]),
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
        for want in want {
            assert!(text.contains(want), "{name}: {want} in {text}");
        }
    }
}

/// The text of an SVG image is PDF text: the fonts' `ToUnicode` maps hold
/// its characters, those only the SVG has too (its Hebrew, its digits, the
/// capitals of its clip path).
#[test]
fn svg_text_maps_to_unicode() {
    let input = fs::read(root().join("fixtures/docs/svg-text.json")).unwrap();
    let mapped = mapped_text(&input);
    for c in "שלוםCLIP42H".chars() {
        assert!(mapped.contains(c), "{c} is not mapped: {mapped}");
    }
}

/// The glyphs form fields show, and those embedded to edit them, map to
/// their characters too, though layout never shaped them.
#[test]
fn form_field_glyphs_map_to_unicode() {
    let field = |field: &str| format!(r#"{{"type":"field","field":{field}}}"#);
    let doc = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
           "pages":[{{"size":{{"width":300,"height":300}},"children":[{},{}]}}]}}"#,
        field(r#"{"name":"a","kind":"text","font":"Inter-Regular","value":"Zoë","readOnly":true}"#),
        field(r#"{"name":"b","kind":"select","font":"Inter-Regular","options":["Ωmega"]}"#),
    );
    let mapped = mapped_text(doc.as_bytes());
    // The value shown, an option, and the Latin range embedded for editing.
    for c in "ZoëΩmega€Șț~".chars() {
        assert!(mapped.contains(c), "{c} is not mapped: {mapped}");
    }
}

/// The text the `ToUnicode` maps of the fonts of document `input` map
/// glyphs to.
fn mapped_text(input: &[u8]) -> String {
    let (fonts, images) = stores();
    let options = RenderOptions::default().with_compress(false);
    let pdf = inkoxide::render(input, InputFormat::Json, &options, &fonts, &images).unwrap();
    let pdf = String::from_utf8_lossy(&pdf);
    let mut mapped = String::new();
    for block in pdf.split("beginbfchar").skip(1) {
        let block = block.split("endbfchar").next().unwrap_or("");
        // `<glyph> <UTF-16BE text>` per line.
        for target in block.lines().filter_map(|l| l.split_whitespace().nth(1)) {
            let hex = target.trim_matches(['<', '>']);
            let units: Vec<u16> = (0..hex.len())
                .step_by(4)
                .filter_map(|i| u16::from_str_radix(hex.get(i..i + 4)?, 16).ok())
                .collect();
            mapped.push_str(&String::from_utf16_lossy(&units));
        }
    }
    mapped
}

/// SVG text draws a colour font's glyphs as images, in spans with their
/// text, as page text does; a font without outlines is never embedded.
#[test]
fn svg_text_draws_colour_glyphs_as_images() {
    let (fonts, images) = stores();
    for fallback in ["EmojiTest-CBDT", "EmojiTest-COLR"] {
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"{fallback}","family":"E"}}],
                "fallbackFonts":["{fallback}"],
                "pages":[{{"size":{{"width":300,"height":100}},"children":[
                {{"type":"image","data":"PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnIHdpZHRoPScyMDAnIGhlaWdodD0nNDAnPjx0ZXh0IHg9JzAnIHk9JzMwJyBmb250LWZhbWlseT0nSW50ZXInIGZvbnQtc2l6ZT0nMjAnPkhpIPCfmIAg8J+Hq/Cfh7c8L3RleHQ+PC9zdmc+"}}]}}]}}"#
        );
        let opts = RenderOptions::default().with_compress(false);
        let pdf = inkoxide::render(doc.as_bytes(), InputFormat::Json, &opts, &fonts, &images)
            .unwrap_or_else(|e| panic!("{fallback}: {e}"));
        let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        assert!(!has(b"EmojiTestCBDT"), "{fallback}");
        // The colour glyphs' images, forms from COLR (the writer's unit
        // tests read what the SVG's form draws).
        let count = |needle: &[u8]| pdf.windows(needle.len()).filter(|w| w == &needle).count();
        assert!(
            count(b"/Subtype /Image") + count(b"/Subtype /Form") >= 3,
            "{fallback}"
        );
        assert_eq!(has(b"EmojiTestCOLR"), fallback == "EmojiTest-COLR");
    }
}
