//! Shared harness for the fuzz targets: fixture fonts and images, the
//! render oracle, and a structure-aware document generator.

use std::sync::LazyLock;

pub mod svg;

use inkoxide::{
    AttachmentStore, Encryption, FontStore, ImageStore, InkoxideError, InputFormat, PdfALevel,
    Permissions, RenderOptions, ValidationCode,
};
use serde_json::{json, Map, Value};

/// The fixture fonts: id, family, weight, style and bytes. Generated
/// documents declare them all, and their runs pick from them. The ids are
/// the file names, as `fixtures/docs` (the `render_json` seeds) expect.
const FIXTURE_FONTS: &[(&str, &str, u16, &str, &[u8])] = &[
    (
        "Inter-Regular",
        "Inter",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/Inter-Regular.ttf"),
    ),
    (
        "Inter-Bold",
        "Inter",
        700,
        "normal",
        include_bytes!("../../fixtures/fonts/Inter-Bold.ttf"),
    ),
    (
        "Inter-Italic",
        "Inter",
        400,
        "italic",
        include_bytes!("../../fixtures/fonts/Inter-Italic.ttf"),
    ),
    // CFF outlines.
    (
        "InterDisplay-Bold",
        "Inter Display",
        700,
        "normal",
        include_bytes!("../../fixtures/fonts/InterDisplay-Bold.otf"),
    ),
    (
        "NotoSans-Regular",
        "Noto Sans",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/NotoSans-Regular.ttf"),
    ),
    (
        "NotoSansArabic-Regular",
        "Noto Sans Arabic",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/NotoSansArabic-Regular.ttf"),
    ),
    (
        "NotoSansHebrew-Regular",
        "Noto Sans Hebrew",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/NotoSansHebrew-Regular.ttf"),
    ),
    (
        "RobotoMono-Regular",
        "Roboto Mono",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/RobotoMono-Regular.ttf"),
    ),
    // Variable fonts: glyf with gvar, HVAR, MVAR and avar, and CFF2.
    (
        "VarTest-glyf",
        "VarTest",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/VarTest-glyf.ttf"),
    ),
    (
        "VarTest-CFF2",
        "VarTest CFF2",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/VarTest-CFF2.otf"),
    ),
    // An `opsz` axis, which text takes from its size.
    (
        "VarTest-opsz",
        "VarTest Opsz",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/VarTest-opsz.ttf"),
    ),
    // Vertical metrics (vhea, vmtx, VORG) and vertical forms (`vert`).
    (
        "VertTest",
        "Vert Test",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/VertTest.ttf"),
    ),
    // Colour fonts: COLR layers and SVG documents with outlines, PNG
    // bitmaps without (CBDT and sbix).
    (
        "EmojiTest-COLR",
        "Emoji Test",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/EmojiTest-COLR.ttf"),
    ),
    (
        "EmojiTest-CBDT",
        "Emoji Test Bitmap",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/EmojiTest-CBDT.ttf"),
    ),
    (
        "EmojiTest-sbix",
        "Emoji Test sbix",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/EmojiTest-sbix.ttf"),
    ),
    (
        "EmojiTest-SVG",
        "Emoji Test SVG",
        400,
        "normal",
        include_bytes!("../../fixtures/fonts/EmojiTest-SVG.ttf"),
    ),
];

/// The colour fonts among [`FIXTURE_FONTS`]: the last ones.
#[cfg(test)]
const COLOUR_FONTS: usize = 4;

pub static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let mut f = FontStore::new();
    for &(id, _, _, _, bytes) in FIXTURE_FONTS {
        f.register(id, bytes.to_vec()).unwrap();
    }
    f
});

/// The files `attachments` name by `src`: an XML invoice and a few binary
/// bytes.
pub static ATTACHMENTS: LazyLock<AttachmentStore> = LazyLock::new(|| {
    let mut s = AttachmentStore::new();
    s.insert("att-xml", b"<?xml version=\"1.0\"?><Invoice/>".to_vec());
    s.insert(
        "att-bin",
        vec![0u8, 1, 2, 0xFF, 0xFE, b'%', b'%', b'E', b'O', b'F'],
    );
    s
});

/// The names the generator gives its attachments, which icons name.
const FILE_NAMES: &[&str] = &["factur-x.xml", "data.bin", "Grüße.txt"];

/// The fixture images, by file name as `fixtures/docs` expects; `svgtext`
/// is `text` under the name the generator has used.
const FIXTURE_IMAGES: &[(&str, &[u8])] = &[
    ("anim", include_bytes!("../../fixtures/images/anim.gif")),
    (
        "badge-lossless",
        include_bytes!("../../fixtures/images/badge-lossless.webp"),
    ),
    ("chart", include_bytes!("../../fixtures/images/chart.svg")),
    (
        "checker-la",
        include_bytes!("../../fixtures/images/checker-la.png"),
    ),
    (
        "emoji-flag",
        include_bytes!("../../fixtures/images/emoji-flag.svg"),
    ),
    (
        "emoji-smile",
        include_bytes!("../../fixtures/images/emoji-smile.svg"),
    ),
    ("logo", include_bytes!("../../fixtures/images/logo.png")),
    (
        "markers",
        include_bytes!("../../fixtures/images/markers.svg"),
    ),
    (
        "palette",
        include_bytes!("../../fixtures/images/palette.png"),
    ),
    ("photo", include_bytes!("../../fixtures/images/photo.jpg")),
    (
        "photo-cmyk",
        include_bytes!("../../fixtures/images/photo-cmyk.jpg"),
    ),
    (
        "photo-gray",
        include_bytes!("../../fixtures/images/photo-gray.jpg"),
    ),
    (
        "photo-lossy",
        include_bytes!("../../fixtures/images/photo-lossy.webp"),
    ),
    (
        "photo-rot6",
        include_bytes!("../../fixtures/images/photo-rot6.jpg"),
    ),
    ("text", include_bytes!("../../fixtures/images/text.svg")),
    ("svgtext", include_bytes!("../../fixtures/images/text.svg")),
    (
        "text-path",
        include_bytes!("../../fixtures/images/text-path.svg"),
    ),
    (
        "text-vertical",
        include_bytes!("../../fixtures/images/text-vertical.svg"),
    ),
    (
        "text-vertical-path",
        include_bytes!("../../fixtures/images/text-vertical-path.svg"),
    ),
];

pub static IMAGES: LazyLock<ImageStore> = LazyLock::new(|| {
    let mut s = ImageStore::new();
    for &(id, bytes) in FIXTURE_IMAGES {
        s.insert(id, bytes.to_vec());
    }
    s.insert("bad", b"\x89PNG\r\n\x1a\nnot really".to_vec());
    s.insert(
        "badsvg",
        b"<svg xmlns='http://www.w3.org/2000/svg'><text><textPath>hi</textPath></text></svg>"
            .to_vec(),
    );
    s
});

/// Validates and renders `doc`; panics (a finding) when the engine panics
/// or when `validate` and `render` disagree. A JSON document that renders
/// must render to the same bytes as `MessagePack`.
///
/// # Panics
///
/// On a finding.
pub fn check(doc: &[u8]) {
    // Most inputs also go through the PDF/A writer.
    let _ = check_with(doc, &FONTS, &IMAGES, pdf_a_level(doc.len()));
}

/// Above this size the JSON render is not rendered again, encrypted and
/// as `MessagePack`: a valid document can ask for tens of thousands of pages
/// (a row 1 000 000 points high on a page 32 points high), and three
/// renders of those take longer than the fuzzer's timeout.
const RERENDER_LIMIT: usize = 8 << 20;

/// The oracle of [`check`], with the given stores and PDF/A level:
/// validates and renders the JSON `doc`, encrypted too when `pdf_a` is
/// `None`, and as `MessagePack` (both only up to [`RERENDER_LIMIT`]).
/// Returns the JSON render.
fn check_with(
    doc: &[u8],
    fonts: &FontStore,
    images: &ImageStore,
    pdf_a: Option<PdfALevel>,
) -> Result<Vec<u8>, InkoxideError> {
    let json = render_with(doc, InputFormat::Json, fonts, images, pdf_a);
    if json.as_ref().is_ok_and(|pdf| pdf.len() > RERENDER_LIMIT) {
        return json;
    }
    let value = serde_json::from_slice::<serde_json::Value>(doc).ok();
    // The plain renders are also rendered encrypted.
    if pdf_a.is_none() {
        check_encrypted(doc, &json, value.as_ref(), fonts, images);
    }
    let Some(value) = value else {
        return json;
    };
    let Ok(packed) = rmp_serde::to_vec(&value) else {
        return json;
    };
    // Only a document JSON accepts has to round-trip: the `Value` detour
    // drops duplicate keys, which JSON parsing rejects.
    let msgpack = render_with(&packed, InputFormat::MessagePack, fonts, images, pdf_a);
    if let Ok(a) = &json {
        match &msgpack {
            Ok(b) => assert!(a == b, "MessagePack output differs from JSON"),
            Err(e) => panic!("MessagePack rejected a document JSON renders: {e}"),
        }
    }
    json
}

/// Instances of the variable fixture fonts: id, source, weight and axis
/// values (one past its axis, one an axis the font lacks).
const INSTANCES: &[(&str, &str, u16, &str)] = &[
    (
        "VarTest-650",
        "VarTest-glyf",
        650,
        r#"{"wght":650,"wdth":75}"#,
    ),
    (
        "VarTest-max",
        "VarTest-glyf",
        900,
        r#"{"wght":5000,"opsz":12}"#,
    ),
    (
        "VarCFF2-100",
        "VarTest-CFF2",
        100,
        r#"{"wght":100,"wdth":50}"#,
    ),
];

/// The `fonts` of a document that declares every fixture font, the
/// [`INSTANCES`], and a variable font's weights and widths, which SVG text
/// draws at its own and runs at their size's optical size.
fn fixture_fonts() -> Vec<Value> {
    let fonts = FIXTURE_FONTS
        .iter()
        .map(|(id, family, weight, style, _)| {
            json!({"id": id, "family": family, "weight": weight, "style": style})
        });
    let instances = INSTANCES.iter().map(|(id, source, weight, variations)| {
        let variations: Value = serde_json::from_str(variations).unwrap();
        json!({"id": id, "family": "VarTest", "weight": weight, "source": source, "variations": variations})
    });
    let ranged = json!({"id": "VarTest-range", "family": "VarRange", "source": "VarTest-glyf",
        "weightRange": [100, 900], "stretchRange": [50, 100]});
    fonts.chain(instances).chain([ranged]).collect()
}

/// Renders `doc` encrypted: it must render exactly when `plain`, its
/// unencrypted render, did, fail with the same code otherwise, and keep
/// the title out of plain sight.
fn check_encrypted(
    doc: &[u8],
    plain: &Result<Vec<u8>, InkoxideError>,
    value: Option<&Value>,
    fonts: &FontStore,
    images: &ImageStore,
) {
    let options = RenderOptions::default()
        .with_compress(false)
        .with_threads(1)
        .with_attachments(ATTACHMENTS.clone())
        .with_encryption(
            Encryption::new()
                .with_user_password("fuzz user")
                .with_owner_password("fuzz owner")
                .with_permissions(Permissions::new().with_copying(true)),
        );
    let encrypted = inkoxide::render(doc, InputFormat::Json, &options, fonts, images);
    let (plain, encrypted) = match (plain, &encrypted) {
        (Ok(p), Ok(e)) => (p, e),
        (Err(a), Err(b)) => {
            assert_eq!(a.code(), b.code(), "encrypted: {b}; plain: {a}");
            return;
        }
        (p, e) => panic!(
            "plain {:?}, encrypted {:?}",
            p.as_ref().map(|_| ()),
            e.as_ref().map(|_| ())
        ),
    };
    assert!(encrypted.starts_with(b"%PDF-") && encrypted.trim_ascii_end().ends_with(b"%%EOF"));
    // Encrypted streams are binary: only the object syntax is scanned.
    assert_finite_objects(encrypted);
    // A title the plain file shows as a literal string is encrypted.
    let title = value
        .and_then(|v| v.pointer("/meta/title"))
        .and_then(Value::as_str)
        .filter(|t| {
            t.len() >= 4
                && t.bytes()
                    .all(|b| b.is_ascii_graphic() && !b"()\\".contains(&b))
        });
    if let Some(t) = title {
        let literal = format!("({t})");
        let has = |pdf: &[u8]| pdf.windows(literal.len()).any(|w| w == literal.as_bytes());
        if has(plain) {
            assert!(!has(encrypted), "the title {literal} is in plain text");
        }
    }
}

/// Renders arbitrary bytes registered as an image (SVG or raster), drawn
/// twice at different sizes, in a document that declares the fixture
/// fonts (SVG text is laid out with them); same oracle as [`check`].
///
/// # Errors
///
/// Why the engine did not render it.
pub fn check_image(bytes: &[u8]) -> Result<(), InkoxideError> {
    let mut images = ImageStore::new();
    images.insert("fz", bytes.to_vec());
    let doc = json!({
        "version": 1,
        "meta": {"title": "Fuzz-Image"},
        "fonts": fixture_fonts(),
        "pages": [{"size": {"width": 300, "height": 400}, "children": [
            {"type": "image", "src": "fz"},
            {"type": "image", "src": "fz", "fit": "cover", "style": {"width": 40, "height": 90}}
        ]}]
    })
    .to_string();
    check_with(doc.as_bytes(), &FONTS, &images, pdf_a_level(bytes.len())).map(|_| ())
}

/// Registers arbitrary bytes as a font, as they are and with the WOFF and
/// WOFF2 signatures in place of their first four bytes, so the decoders
/// are reached from any input. Each font that registers sets a line of
/// text, and is the fallback of a line in Inter with emoji sequences (a
/// colour font's glyphs are drawn from its colour tables); same oracle as
/// [`check`], and rendered again without subsetting, which embeds each
/// font and instance whole. Returns how many registered.
///
/// # Panics
///
/// On a finding.
pub fn check_font(bytes: &[u8]) -> usize {
    let mut variants = vec![bytes.to_vec()];
    if let Some(rest) = bytes.get(4..) {
        for signature in [b"wOFF", b"wOF2"] {
            variants.push([&signature[..], rest].concat());
        }
    }
    // A variable font is also drawn at an instance: past its axes, and at
    // an axis it may lack.
    let doc = r#"{"version":1,"meta":{"title":"Fuzz-Font"},"fonts":[{"id":"fz","family":"Fz"},{"id":"Inter-Regular","family":"Inter"},
        {"id":"fzv","family":"Fz","weight":900,"source":"fz","variations":{"wght":900,"wdth":50,"opsz":72,"slnt":-90}},
        {"id":"fzr","family":"Fzr","source":"fz","weightRange":[1,1000],"stretchRange":[1,1000]}],
        "fallbackFonts":["fz"],"pages":[{"size":{"width":300,"height":200},"children":[
        {"type":"text","runs":[{"text":"Hello, world! 0123 Äß של fi\u00ad😀","font":"fz","size":12}]},
        {"type":"text","runs":[{"text":"Variable 0123 Äß","font":"fzv","size":12}]},
        {"type":"text","runs":[{"text":"Optical 0123","font":"fzr","size":7},{"text":"Optical","font":"fzr","size":48}]},
        {"type":"text","runs":[{"text":"😀 👩‍💻 🇫🇷 1️⃣ #⃣ 👍🏽 ❤️ ☺︎ 🏴󠁧󠁢󠁥󠁮󠁧󠁿","font":"Inter-Regular","size":12}]}]}]}"#;
    let pdf_a = pdf_a_level(bytes.len());
    let mut registered = 0;
    for v in variants {
        let mut fonts = FontStore::new();
        let (_, _, _, _, inter) = FIXTURE_FONTS[0];
        fonts.register("Inter-Regular", inter.to_vec()).unwrap();
        if fonts.register("fz", v).is_ok() {
            registered += 1;
            let rendered = check_with(doc.as_bytes(), &fonts, &IMAGES, pdf_a);
            // Without subsetting, the PDF embeds each font, and each
            // instance, whole: no panic, and a render as the subset one.
            let options = RenderOptions::default()
                .with_compress(false)
                .with_threads(1)
                .with_subset_fonts(false);
            let whole =
                inkoxide::render(doc.as_bytes(), InputFormat::Json, &options, &fonts, &IMAGES);
            if pdf_a.is_none() {
                assert_eq!(
                    rendered.is_ok(),
                    whole.is_ok(),
                    "subsetting changed whether a font renders"
                );
            }
            if let Ok(pdf) = whole {
                assert_finite_numbers(&pdf, true);
            }
        }
    }
    registered
}

/// Every script of `pdf` (uncompressed) is a call of a standard form
/// function with plain arguments: integers, booleans and string literals
/// that cannot end early. PDF/A output has no scripts, no `/AA` and no
/// JavaScript actions. Only keys and action types count: a font may be
/// named `AA` or `JavaScript` (`/BaseFont /AA`).
fn check_scripts(pdf: &[u8], pdf_a: bool) {
    let objects = pdf_objects(pdf);
    if pdf_a {
        assert!(
            objects.scripts.is_empty()
                && !objects
                    .keys
                    .iter()
                    .any(|n| n == b"AA" || n == b"JavaScript")
                && !objects.action_types.iter().any(|n| n == b"JavaScript"),
            "PDF/A output with a script"
        );
    }
    for script in &objects.scripts {
        let script = script.as_deref().expect("a /JS that is not a string");
        assert!(
            form_call(script),
            "a script that is not a form function call: {script:?}"
        );
    }
}

/// What [`check_scripts`] reads from the object syntax of a PDF.
#[derive(Debug, Default)]
struct PdfObjects {
    /// Every name (without its `/`) followed by a value that is not a
    /// name: a key, as `/AA <<`, and never a name value, as the `/AA` of
    /// `/BaseFont /AA`. A key with a name value is not in it.
    keys: Vec<Vec<u8>>,
    /// The value of every `/S` key that is a name: the action types (and
    /// other subtypes, as `/S /D` of page labels).
    action_types: Vec<Vec<u8>>,
    /// The value of every `/JS` key, decoded as a text string; `None` when
    /// it is not a string.
    scripts: Vec<Option<String>>,
}

/// Tokenizes the object syntax of an uncompressed, unencrypted `pdf`:
/// strings (literal and hex) are read as values, never searched, and
/// stream data is skipped, so document text cannot pose as a key.
fn pdf_objects(pdf: &[u8]) -> PdfObjects {
    let mut out = PdfObjects::default();
    let mut i = 0;
    let (mut length, mut after_length) = (None, false);
    let mut after_s = false;
    while i < pdf.len() {
        match pdf[i] {
            b'%' => {
                while i < pdf.len() && pdf[i] != b'\n' {
                    i += 1;
                }
            }
            // A dictionary.
            b'<' if pdf.get(i + 1) == Some(&b'<') => {
                after_s = false;
                i += 2;
            }
            b'(' | b'<' => {
                after_s = false;
                i = pdf_string(pdf, i).map_or(pdf.len(), |(_, end)| end);
            }
            b'/' => {
                let start = i + 1;
                i = start;
                while i < pdf.len() && !delimiter(pdf[i]) {
                    i += 1;
                }
                let name = pdf[start..i].to_vec();
                after_length = name == b"Length";
                if after_s {
                    out.action_types.push(name.clone());
                }
                after_s = name == b"S";
                if name == b"JS" {
                    while i < pdf.len() && pdf[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let value = pdf_string(pdf, i);
                    out.scripts
                        .push(value.as_ref().map(|(bytes, _)| text_string(bytes)));
                    if let Some((_, end)) = value {
                        i = end;
                    }
                }
                let next = pdf[i..].trim_ascii_start();
                if !(next.is_empty()
                    || [&b"/"[..], b">>", b"]"].iter().any(|d| next.starts_with(d)))
                {
                    out.keys.push(name);
                }
            }
            b if delimiter(b) => i += 1,
            _ => {
                after_s = false;
                let start = i;
                while i < pdf.len() && !delimiter(pdf[i]) {
                    i += 1;
                }
                let token = &pdf[start..i];
                if after_length {
                    length = std::str::from_utf8(token).ok().and_then(|t| t.parse().ok());
                }
                after_length = false;
                if token == b"obj" {
                    length = None;
                } else if token == b"stream" {
                    let body = i + if pdf[i..].starts_with(b"\r\n") { 2 } else { 1 };
                    // The data ends at `/Length`, or else (an indirect
                    // length) at the first `endstream`.
                    let end = length
                        .map(|n: usize| (body + n).min(pdf.len()))
                        .filter(|&end| pdf[end..].trim_ascii_start().starts_with(b"endstream"))
                        .or_else(|| {
                            pdf.get(body..)?
                                .windows(9)
                                .position(|w| w == b"endstream")
                                .map(|p| body + p)
                        });
                    i = end.unwrap_or(pdf.len());
                }
            }
        }
    }
    out
}

/// The bytes of the string (literal or hex) that starts at `pdf[i]`, and
/// where it ends.
fn pdf_string(pdf: &[u8], i: usize) -> Option<(Vec<u8>, usize)> {
    match pdf.get(i)? {
        b'(' => {
            let mut out = Vec::new();
            let mut depth = 1;
            let mut j = i + 1;
            while j < pdf.len() {
                let c = pdf[j];
                j += 1;
                match c {
                    b'\\' => {
                        let e = *pdf.get(j)?;
                        j += 1;
                        match e {
                            b'n' => out.push(b'\n'),
                            b'r' => out.push(b'\r'),
                            b't' => out.push(b'\t'),
                            b'b' => out.push(8),
                            b'f' => out.push(12),
                            b'0'..=b'7' => {
                                let mut v = u32::from(e - b'0');
                                for _ in 0..2 {
                                    match pdf.get(j) {
                                        Some(&d @ b'0'..=b'7') => {
                                            v = v * 8 + u32::from(d - b'0');
                                            j += 1;
                                        }
                                        _ => break,
                                    }
                                }
                                out.push(v as u8);
                            }
                            // A line continuation.
                            b'\r' => {
                                if pdf.get(j) == Some(&b'\n') {
                                    j += 1;
                                }
                            }
                            b'\n' => {}
                            e => out.push(e),
                        }
                    }
                    b'(' => {
                        depth += 1;
                        out.push(c);
                    }
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((out, j));
                        }
                        out.push(c);
                    }
                    _ => out.push(c),
                }
            }
            None
        }
        b'<' => {
            let end = i + 1 + pdf.get(i + 1..)?.iter().position(|&b| b == b'>')?;
            let mut hex: Vec<u8> = pdf[i + 1..end]
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect();
            if hex.len() % 2 == 1 {
                hex.push(b'0');
            }
            let bytes = hex
                .chunks(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                .collect::<Option<Vec<u8>>>()?;
            Some((bytes, end + 1))
        }
        _ => None,
    }
}

/// A PDF text string: UTF-16BE after a byte order mark, else one
/// character per byte (`PDFDocEncoding`, which is ASCII where scripts
/// are). Unpaired surrogates become U+FFFD, which no script accepts.
fn text_string(bytes: &[u8]) -> String {
    match bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some(utf16) => {
            let units = utf16
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]));
            char::decode_utf16(units)
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect()
        }
        None => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Whether `script` is one call of a standard form function, ending with
/// `;`, whose arguments are integers, booleans or string literals.
fn form_call(script: &str) -> bool {
    const FUNCTIONS: &[&str] = &[
        "AFNumber_Keystroke",
        "AFNumber_Format",
        "AFPercent_Keystroke",
        "AFPercent_Format",
        "AFDate_KeystrokeEx",
        "AFDate_FormatEx",
        "AFTime_KeystrokeEx",
        "AFTime_FormatEx",
        "AFSpecial_Keystroke",
        "AFSpecial_Format",
    ];
    let Some((name, mut rest)) = script.split_once('(') else {
        return false;
    };
    if !FUNCTIONS.contains(&name) {
        return false;
    }
    loop {
        let arg_end = if let Some(string) = rest.strip_prefix('"') {
            // A string literal: printable ASCII, with quotes and
            // backslashes escaped.
            let mut chars = string.char_indices();
            let mut end = None;
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => {
                        end = Some(i + 2);
                        break;
                    }
                    '\\' => match chars.next() {
                        Some((_, '\\' | '"' | '\'')) => {}
                        Some((_, 'u')) => {
                            let hex = (0..4)
                                .filter_map(|_| chars.next())
                                .filter(|(_, c)| c.is_ascii_hexdigit());
                            if hex.count() != 4 {
                                return false;
                            }
                        }
                        _ => return false,
                    },
                    '\'' => return false,
                    ' '..='~' => {}
                    _ => return false,
                }
            }
            let Some(end) = end else { return false };
            end
        } else {
            let end = rest.find([',', ')']).unwrap_or(rest.len());
            let arg = &rest[..end];
            if !(arg == "true"
                || arg == "false"
                || (!arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit())))
            {
                return false;
            }
            end
        };
        rest = &rest[arg_end..];
        if rest == ");" {
            return true;
        }
        let Some(next) = rest.strip_prefix(", ") else {
            return false;
        };
        rest = next;
    }
}

/// The PDF/A level an input of `len` bytes is rendered at: none, or one
/// of the three, a quarter of the inputs each.
fn pdf_a_level(len: usize) -> Option<PdfALevel> {
    match len % 4 {
        1 => Some(PdfALevel::A1b),
        2 => Some(PdfALevel::A2b),
        3 => Some(PdfALevel::A3b),
        _ => None,
    }
}

#[cfg(test)]
fn render(
    doc: &[u8],
    format: InputFormat,
    pdf_a: Option<PdfALevel>,
) -> Result<Vec<u8>, InkoxideError> {
    render_with(doc, format, &FONTS, &IMAGES, pdf_a)
}

fn render_with(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
    images: &ImageStore,
    pdf_a: Option<PdfALevel>,
) -> Result<Vec<u8>, InkoxideError> {
    let errors = inkoxide::validate(doc, format, fonts, images);
    let mut options = RenderOptions::default()
        .with_compress(false)
        .with_threads(1)
        .with_attachments(ATTACHMENTS.clone());
    if let Some(level) = pdf_a {
        options = options.with_pdf_a_level(level);
    }
    let result = inkoxide::render(doc, format, &options, fonts, images);
    // Tagging changes what is written, not what renders: a quarter of the
    // documents render tagged too, and both render or fail alike; as
    // PDF/UA, only `UA_CONFLICT` may refuse more.
    if doc.len() % 4 == 1 {
        let ua = doc.len() % 8 == 1;
        let tagged = options.clone().with_tagged(true).with_pdf_ua(ua);
        let other = inkoxide::render(doc, format, &tagged, fonts, images);
        match (&result, &other) {
            (Ok(_), Ok(pdf)) => assert!(pdf.starts_with(b"%PDF-")),
            (_, Err(InkoxideError::Validation(e)))
                if ua && e.code == ValidationCode::UaConflict => {}
            (Err(a), Err(b)) => assert_eq!(a.code(), b.code(), "tagged: {b}; untagged: {a}"),
            (a, b) => panic!(
                "untagged {:?}, tagged {:?}",
                a.as_ref().map(|_| ()),
                b.as_ref().map(|_| ())
            ),
        }
    }
    match &result {
        Ok(pdf) => {
            assert!(
                errors.is_empty(),
                "rendered a document validate rejects: {errors:?}"
            );
            assert!(pdf.starts_with(b"%PDF-") && pdf.trim_ascii_end().ends_with(b"%%EOF"));
            assert_finite_numbers(pdf, true);
            check_scripts(pdf, pdf_a.is_some());
        }
        // `validate` does not know the PDF/A level: every level rejects
        // valid documents with a page side under 3 points, PDF/A-1b those
        // that need transparency.
        Err(InkoxideError::Validation(e))
            if e.code == ValidationCode::PdfaConflict && errors.is_empty() =>
        {
            assert!(
                pdf_a == Some(PdfALevel::A1b)
                    || (pdf_a.is_some() && e.message.contains("page side"))
                    || (pdf_a == Some(PdfALevel::A2b) && e.path == "attachments[0]"),
                "PDFA_CONFLICT at {pdf_a:?}: {e:?}"
            );
        }
        // What only a render knows: the attachment store, and that XMP
        // extensions have no metadata to go in without PDF/A.
        Err(InkoxideError::Validation(e))
            if errors.is_empty()
                && (e.code == ValidationCode::AttachmentNotRegistered
                    || (pdf_a.is_none() && e.path == "meta.xmpExtensions")) => {}
        Err(InkoxideError::Validation(e)) => {
            assert!(
                !errors.is_empty(),
                "render rejected a valid document: {e:?}"
            );
        }
        // The writer's backstop: PDF/A-1b validation must have rejected
        // the document first.
        Err(InkoxideError::Pdf(m)) => assert!(
            !m.contains("does not allow transparency"),
            "validation missed transparency: {m}"
        ),
        Err(_) => {}
    }
    result
}

/// PDF whitespace and delimiters.
fn delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || b"()<>[]{}/%".contains(&b)
}

/// Panics on an `inf` or `NaN` operand (what pdf-writer writes for a
/// non-finite f32; PDF has no such numbers and viewers reject the page).
/// Scans the object syntax (`objects`) and, from there, the uncompressed
/// streams: content streams (the oracle renders without compression) and
/// widget appearances. Compressed streams (fonts, images, SVG forms) and
/// XMP metadata are skipped, and so are strings and names, which hold
/// document text (`(inf)`, `/NaN`) rather than numbers.
fn assert_finite_numbers(data: &[u8], objects: bool) {
    scan_numbers(data, objects, true);
}

/// [`assert_finite_numbers`] over the object syntax of `pdf` only, not
/// its streams.
fn assert_finite_objects(pdf: &[u8]) {
    scan_numbers(pdf, true, false);
}

/// The scan of [`assert_finite_numbers`]; streams are followed only with
/// `streams`.
fn scan_numbers(data: &[u8], objects: bool, streams: bool) {
    let mut i = 0;
    // Since the last `obj`: the `/Length` and whether the stream is
    // compressed or metadata.
    let (mut length, mut skip, mut after_length) = (None, false, false);
    while i < data.len() {
        match data[i] {
            b'(' => {
                // A literal string: balanced parentheses, `\` escapes.
                let mut depth = 0;
                while i < data.len() {
                    match data[i] {
                        b'\\' => i += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
            }
            b'%' => {
                while i < data.len() && data[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' => {
                let start = i + 1;
                i = start;
                while i < data.len() && !delimiter(data[i]) {
                    i += 1;
                }
                let name = &data[start..i];
                after_length = name == b"Length";
                skip |= objects && (name == b"Filter" || name == b"Metadata");
            }
            b if delimiter(b) => i += 1,
            _ => {
                let start = i;
                while i < data.len() && !delimiter(data[i]) {
                    i += 1;
                }
                let token = &data[start..i];
                let number = token.strip_prefix(b"-").unwrap_or(token);
                assert!(
                    number != b"inf" && number != b"NaN",
                    "non-finite number `{}` in the PDF",
                    String::from_utf8_lossy(token)
                );
                if !objects {
                    continue;
                }
                if after_length {
                    length = std::str::from_utf8(token).ok().and_then(|t| t.parse().ok());
                }
                after_length = false;
                if token == b"obj" {
                    (length, skip) = (None, false);
                } else if token == b"stream" {
                    let body = i + if data[i..].starts_with(b"\r\n") { 2 } else { 1 };
                    let end = length.map_or(data.len(), |n: usize| (body + n).min(data.len()));
                    if streams && !skip {
                        scan_numbers(&data[body.min(end)..end], false, true);
                    }
                    i = end;
                }
            }
        }
    }
}

/// Entropy source over the fuzzer's bytes; reads zeros once exhausted, so
/// generation always terminates.
#[derive(Debug)]
pub struct Src<'a> {
    data: &'a [u8],
    pos: usize,
    /// One document in eight may break the schema on purpose.
    chaos: bool,
    /// Generating flowing content (not a header, footer or table cell).
    flow: bool,
}

impl<'a> Src<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut s = Self {
            data,
            pos: 0,
            chaos: false,
            flow: true,
        };
        s.chaos = s.byte() >= 224;
        s
    }
    fn byte(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
    fn below(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        let v = if n <= 256 {
            self.byte() as usize
        } else {
            u16::from_le_bytes([self.byte(), self.byte()]) as usize
        };
        v % n
    }
    fn chance(&mut self, of256: u8) -> bool {
        self.byte() < of256
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
    fn exhausted(&self) -> bool {
        self.pos >= self.data.len()
    }
}

fn num(s: &mut Src) -> Value {
    match s.below(12) {
        0 => json!(0),
        1 => json!(-(s.below(100) as f64)),
        2 => edge(s),
        3 => json!(1e-6),
        4 => json!(14_400),
        _ => json!(s.below(600) as f64 + s.below(4) as f64 * 0.25),
    }
}

/// Numbers at the bounds of validation (±1000000 for lengths and factors),
/// and now and then just past them or far past them: values that
/// overflowed f32 in layout before validation bounded them. 1e39 is
/// infinite as f32, which fails the whole parse where a length is
/// expected, so it is rarer still.
fn edge(s: &mut Src) -> Value {
    match s.below(8) {
        0 | 1 => json!(1_000_000),
        2 | 3 => json!(-1_000_000),
        4 => json!(999_999.75),
        5 => json!(1_000_000.5),
        6 => json!(s.pick(&[-1e9, f64::from(f32::MAX), -1e30])),
        _ if s.chance(32) => json!(1e39),
        _ => json!(-999_999.75),
    }
}

/// Percentages at and past the ±10000 % bound; a huge one resolved to
/// `inf` before it was bounded.
fn edge_pct(s: &mut Src) -> Value {
    json!(s.pick(&[
        "10000%",
        "-10000%",
        "9999.5%",
        "10000.01%",
        "-20000%",
        "9999999999999999999999999999999999999%",
        "-9999999999999999999999999999999999999%",
    ]))
}

fn nonneg(s: &mut Src) -> Value {
    match s.below(8) {
        0 => json!(0),
        1 => json!(1_000_000),
        2 if s.chance(64) => json!(s.pick(&[1_000_000.5, 1e7, 1e39])),
        _ => json!(s.below(50)),
    }
}

fn length(s: &mut Src) -> Value {
    match s.below(6) {
        0 => json!("auto"),
        1 => json!(format!("{}%", s.below(250))),
        2 if s.chance(64) => edge_pct(s),
        _ => num(s),
    }
}

fn length_pct(s: &mut Src) -> Value {
    if s.chance(60) {
        if s.chance(16) {
            edge_pct(s)
        } else {
            json!(format!("{}%", s.below(120)))
        }
    } else {
        num(s)
    }
}

fn color(s: &mut Src) -> Value {
    let mut c: Vec<Value> = (0..4).map(|_| json!(s.below(5) as f64 / 4.0)).collect();
    if s.chaos && s.chance(8) {
        c.pop();
    }
    Value::Array(c)
}

fn edges(s: &mut Src, f: fn(&mut Src) -> Value) -> Value {
    let mut m = Map::new();
    for k in ["top", "right", "bottom", "left"] {
        if s.chance(150) {
            m.insert(k.into(), f(s));
        }
    }
    Value::Object(m)
}

fn corners(s: &mut Src, f: fn(&mut Src) -> Value) -> Value {
    let mut m = Map::new();
    for k in ["topLeft", "topRight", "bottomRight", "bottomLeft"] {
        if s.chance(150) {
            m.insert(k.into(), f(s));
        }
    }
    Value::Object(m)
}

fn border_style(s: &mut Src) -> Value {
    json!(s.pick(&["solid", "dashed", "dotted"]))
}

fn breadth(s: &mut Src) -> Value {
    match s.below(7) {
        0 => json!(format!("{}fr", s.below(4))),
        1 => json!("auto"),
        2 => json!("min-content"),
        3 => json!("max-content"),
        4 => json!(format!("{}%", s.below(100))),
        // The bounds (1000000 points or fr, 10000 %) and past them.
        5 if s.chance(64) => match s.below(4) {
            0 => json!(s.pick(&["1000000fr", "1000001fr"])),
            1 => edge_pct(s),
            _ => edge(s),
        },
        _ => json!(s.below(300)),
    }
}

/// A fixed breadth, as auto-repeat requires.
fn fixed(s: &mut Src) -> Value {
    if s.chance(60) {
        json!(format!("{}%", 1 + s.below(60)))
    } else {
        json!(4 + s.below(200))
    }
}

fn track(s: &mut Src) -> Value {
    match s.below(10) {
        0 | 1 => {
            let min = if s.chaos { breadth(s) } else { fixed(s) };
            json!({"min": min, "max": breadth(s)})
        }
        2 => json!({"fitContent": length_pct(s)}),
        _ => breadth(s),
    }
}

/// `gridTemplateColumns` / `gridTemplateRows`: tracks and `repeat()`s.
fn template(s: &mut Src) -> Value {
    if s.chance(50) {
        // An auto repeat: everything must have a fixed size.
        let n = s.pick(&["auto-fill", "auto-fit"]);
        let mut v: Vec<Value> = (0..s.below(2)).map(|_| fixed(s)).collect();
        let tracks: Vec<Value> = (0..=s.below(2)).map(|_| fixed(s)).collect();
        v.insert(s.below(v.len() + 1), json!({"repeat": n, "tracks": tracks}));
        return Value::Array(v);
    }
    let v = (0..s.below(6))
        .map(|_| {
            if s.chance(40) {
                let count = if s.chaos {
                    json!(s.below(1200))
                } else {
                    json!(1 + s.below(6))
                };
                let tracks: Vec<Value> = (0..=s.below(3)).map(|_| track(s)).collect();
                json!({"repeat": count, "tracks": tracks})
            } else {
                track(s)
            }
        })
        .collect();
    Value::Array(v)
}

/// Area templates; every one names `a` and `b`.
const AREAS: &[&[&str]] = &[
    &["a b"],
    &["a a", "b c"],
    &["head head head", "a b b", "a b b", ". . c"],
    &["a", "b", "a"],
];

fn placement(s: &mut Src) -> Value {
    let mut m = Map::new();
    if s.chance(128) {
        m.insert("start".into(), json!(s.below(10) as i64 - 3));
    }
    if s.chance(100) {
        m.insert("end".into(), json!(s.below(10) as i64 - 3));
    }
    if s.chance(100) {
        m.insert("span".into(), json!(s.below(6)));
    }
    Value::Object(m)
}

const TEXT_ONLY: &[&str] = &[
    "textAlign",
    "lineHeight",
    "maxLines",
    "ellipsis",
    "hyphenate",
    "lang",
    "whiteSpace",
    "textWrap",
    "hyphenationPenalty",
    "textIndent",
    "orphans",
    "widows",
    "direction",
];

const BOX_PROPS: &[&str] = &[
    "display",
    "flexDirection",
    "flexWrap",
    "justifyContent",
    "alignItems",
    "alignSelf",
    "alignContent",
    "flexGrow",
    "flexShrink",
    "flexBasis",
    "gap",
    "rowGap",
    "columnGap",
    "width",
    "height",
    "minWidth",
    "minHeight",
    "maxWidth",
    "maxHeight",
    "aspectRatio",
    "padding",
    "margin",
    "position",
    "inset",
    "float",
    "clear",
    "gridTemplateColumns",
    "gridTemplateRows",
    "gridAutoFlow",
    "gridColumn",
    "gridRow",
    "gridAutoColumns",
    "gridAutoRows",
    "gridTemplateAreas",
    "gridArea",
    "justifyItems",
    "justifySelf",
    "overflow",
    "backgroundColor",
    "opacity",
    "borderWidth",
    "borderColor",
    "borderRadius",
    "borderStyle",
    "rotate",
    "transform",
    "transformOrigin",
    "verticalAlign",
    "zIndex",
];

fn prop(s: &mut Src, name: &str) -> Value {
    let e = |s: &mut Src, xs: &[&str]| json!(s.pick(xs));
    match name {
        "display" => e(s, &["flex", "grid", "none"]),
        "flexDirection" => e(s, &["row", "column", "row-reverse", "column-reverse"]),
        "flexWrap" => e(s, &["nowrap", "wrap", "wrap-reverse"]),
        "justifyContent" | "alignContent" => e(
            s,
            &[
                "flex-start",
                "flex-end",
                "center",
                "space-between",
                "space-around",
                "space-evenly",
                "stretch",
                "start",
                "end",
            ],
        ),
        "alignSelf" | "justifySelf" => e(
            s,
            &[
                "auto",
                "flex-start",
                "flex-end",
                "center",
                "baseline",
                "stretch",
                "start",
                "end",
            ],
        ),
        "alignItems" | "justifyItems" => e(
            s,
            &[
                "flex-start",
                "flex-end",
                "center",
                "baseline",
                "stretch",
                "start",
                "end",
            ],
        ),
        "borderRadius" if s.chance(80) => corners(s, nonneg),
        "flexGrow" | "flexShrink" | "borderRadius" => nonneg(s),
        "gap" | "rowGap" | "columnGap" => length_pct(s),
        "flexBasis" | "width" | "height" | "minWidth" | "minHeight" | "maxWidth" | "maxHeight" => {
            length(s)
        }
        "aspectRatio" | "lineHeight" | "rotate" => num(s),
        "transform" => transform(s),
        "transformOrigin" => json!([length_pct(s), length_pct(s)]),
        "textIndent" => match s.below(4) {
            0 => json!(s.pick(&[10_000.0, -10_000.0, 10_000.5])),
            _ => json!(s.below(40) as f64 - 20.0),
        },
        "padding" => edges(s, length_pct),
        "margin" | "inset" => edges(s, length),
        "position" => e(s, &["relative", "absolute"]),
        "gridTemplateColumns" | "gridTemplateRows" => template(s),
        "gridAutoColumns" | "gridAutoRows" => {
            Value::Array((0..s.below(3)).map(|_| track(s)).collect())
        }
        "gridTemplateAreas" if s.chaos => e(s, &["a b", "a\tb", "1 2", "a . a"])
            .as_str()
            .map_or(Value::Null, |r| json!([r, r])),
        "gridTemplateAreas" => json!(s.pick(AREAS)),
        // Random names in chaos; otherwise placed by `node` under a parent
        // with areas (`Null` here, as for any unknown property).
        "gridArea" if s.chaos => e(s, &["a", "b", "zz", "9"]),
        "gridAutoFlow" => e(s, &["row", "column", "row dense", "column dense"]),
        "gridColumn" | "gridRow" => placement(s),
        "overflow" => e(s, &["visible", "hidden"]),
        "float" => e(s, &["left", "right", "none"]),
        "clear" => e(s, &["left", "right", "both", "none"]),
        "backgroundColor" => color(s),
        "opacity" => json!(s.below(5) as f64 / 4.0),
        "borderWidth" => edges(s, nonneg),
        "borderColor" => edges(s, color),
        "borderStyle" if s.chance(80) => edges(s, border_style),
        "borderStyle" => border_style(s),
        "textAlign" => e(s, &["left", "center", "right", "justify"]),
        "maxLines" => json!(s.below(5)),
        "ellipsis" | "hyphenate" => json!(s.chance(128)),
        "lang" => e(s, &["fr", "en", "en-US", "de", "xx", ""]),
        "whiteSpace" => e(s, &["normal", "nowrap", "pre", "pre-wrap", "pre-line"]),
        "textWrap" => e(s, &["wrap", "pretty", "pretty", "balance"]),
        "hyphenationPenalty" if s.chaos && s.chance(10) => json!(20_000),
        "hyphenationPenalty" => json!([0, 50, 500, 10_000][s.below(4)]),
        "orphans" | "widows" => match s.below(8) {
            0 => json!(100),
            1 if s.chaos => json!(s.pick(&[0, 101, -1])),
            _ => json!(1 + s.below(5)),
        },
        "direction" => e(s, &["ltr", "rtl", "auto"]),
        "verticalAlign" => e(s, &["top", "middle", "bottom"]),
        "zIndex" => match s.below(8) {
            0 => json!(s.pick(&[i64::from(i32::MIN), i64::from(i32::MAX)])),
            1 if s.chaos => json!(s.pick(&[i64::from(i32::MAX) + 1, -1i64 << 40])),
            _ => json!(s.below(5) as i64 - 2),
        },
        _ => Value::Null,
    }
}

/// A scale factor or matrix entry: ordinary, mirrored, or at the edges.
fn factor(s: &mut Src) -> Value {
    match s.below(4) {
        0 => edge(s),
        1 => json!(-1),
        _ => json!(0.5 + s.below(4) as f64),
    }
}

/// `transform`: one to three functions.
fn transform(s: &mut Src) -> Value {
    let ops = (0..=s.below(3))
        .map(|_| match s.below(5) {
            0 => json!({"translate": [length_pct(s), length_pct(s)]}),
            1 => json!({"scale": [factor(s), factor(s)]}),
            2 => json!({"rotate": num(s)}),
            3 => json!({"skew": [s.pick(&[0.0, 30.0, -89.9, 90.0]), 0]}),
            _ => json!({"matrix": [factor(s), 0, 0, factor(s), num(s), num(s)]}),
        })
        .collect();
    Value::Array(ops)
}

/// Any style property: one of [`BOX_PROPS`] or [`TEXT_ONLY`].
fn any_prop(s: &mut Src) -> &'static str {
    let i = s.below(BOX_PROPS.len() + TEXT_ONLY.len());
    BOX_PROPS
        .get(i)
        .copied()
        .unwrap_or_else(|| TEXT_ONLY[i - BOX_PROPS.len()])
}

fn style(s: &mut Src, text: bool) -> Value {
    let mut m = Map::new();
    for _ in 0..s.below(7) {
        let mut p = any_prop(s);
        if !text && !s.chaos && TEXT_ONLY.contains(&p) {
            continue;
        }
        if text && s.chance(128) {
            p = s.pick(TEXT_ONLY);
        }
        m.insert(p.into(), prop(s, p));
    }
    Value::Object(m)
}

fn brk(s: &mut Src) -> Value {
    let mut m = Map::new();
    if s.chance(60) {
        m.insert("before".into(), json!(s.pick(&["auto", "page"])));
    }
    if s.chance(60) {
        m.insert("after".into(), json!(s.pick(&["auto", "page"])));
    }
    if s.chance(80) {
        m.insert("inside".into(), json!(s.pick(&["auto", "avoid"])));
    }
    if s.chance(40) {
        m.insert("minPresenceAhead".into(), nonneg(s));
    }
    if s.chance(40) {
        m.insert("fixed".into(), json!(true));
    }
    Value::Object(m)
}

const WORDS: &[&str] = &[
    "Lorem",
    "ipsum",
    "dolor",
    "éèçàœ",
    "Größenordnung",
    "Ünïcödé",
    "VAT",
    "1 234,56",
    "—",
    "a",
    "supercalifragilisticexpialidocious",
    "\u{200B}",
    "\u{00AD}",
    "\n",
    "\t",
    "  ",
    "مرحبا",
    "日本語",
    "😀",
    "👩\u{200D}💻",
    "🇫🇷",
    "1\u{FE0F}\u{20E3}",
    "👍🏽",
    "❤\u{FE0F}",
    "❤\u{FE0E}",
    "\u{200D}",
    "fi",
    "AV",
    "-",
    "\u{feff}",
    "",
];

/// A sentence of plain words, repeated into long paragraphs: one chunk
/// with a break opportunity at every space, for `textWrap: pretty`.
const PROSE: &str = "In olden times when wishing still helped one, there lived a king \
    whose daughters were all beautiful. ";

fn text(s: &mut Src) -> String {
    let mut t = String::new();
    for _ in 0..s.below(24) {
        t.push_str(s.pick(WORDS));
        if s.chance(200) {
            t.push(' ');
        }
    }
    if s.chance(6) {
        // One long word, or a long paragraph.
        t = if s.chance(128) {
            "x".repeat(s.below(5000))
        } else {
            PROSE.repeat(2 + s.below(200))
        };
    }
    t
}

fn link(s: &mut Src) -> Value {
    if s.chaos && s.chance(60) {
        return json!(s.pick(&["", "javascript:x", "nope", "#", "#missing"]));
    }
    match s.below(4) {
        0 => json!("https://example.com/ä b?q=1#f"),
        1 => json!("mailto:x@example.com"),
        _ => json!(format!("#n{}", s.below(4))),
    }
}

/// A fixture font, or one the document does not declare.
fn font(s: &mut Src) -> Value {
    let declared = FIXTURE_FONTS.len() + INSTANCES.len();
    let i = s.below(declared + 2);
    let font = FIXTURE_FONTS
        .get(i)
        .map(|f| f.0)
        .or_else(|| INSTANCES.get(i - FIXTURE_FONTS.len()).map(|f| f.0))
        .or_else(|| (i == declared).then_some("VarTest-range"))
        .unwrap_or("Missing");
    json!(font)
}

fn font_size(s: &mut Src) -> Value {
    match s.below(8) {
        0 => json!(1000),
        1 => json!(0.01),
        2 => json!(0),
        _ => json!(4 + s.below(30)),
    }
}

fn run(s: &mut Src) -> Value {
    let mut m = Map::new();
    // One byte picks a placeholder, a note or text.
    let kind = s.byte();
    if kind < 20 {
        m.insert(
            "placeholder".into(),
            json!(s.pick(&["pageNumber", "totalPages"])),
        );
    } else {
        m.insert("text".into(), json!(text(s)));
    }
    m.insert("font".into(), font(s));
    m.insert("size".into(), font_size(s));
    if s.chance(50) {
        m.insert("color".into(), color(s));
    }
    if s.chance(40) {
        m.insert("letterSpacing".into(), num(s));
    }
    if s.chance(40) {
        m.insert(
            "decoration".into(),
            json!(s.pick(&["none", "underline", "lineThrough"])),
        );
        if s.chance(80) {
            m.insert(
                "decorationStyle".into(),
                json!(s.pick(&["solid", "double", "dotted", "dashed", "wavy"])),
            );
        }
        if s.chance(60) {
            m.insert("decorationColor".into(), color(s));
        }
        if s.chance(60) {
            let thickness = match s.below(8) {
                0 => json!(100),
                1 => json!(0.01),
                2 if s.chaos => json!(s.pick(&[0.0, -1.0, 100.5])),
                _ => json!(0.25 * (1 + s.below(16)) as f64),
            };
            m.insert("decorationThickness".into(), thickness);
        }
    }
    if s.chance(20) {
        m.insert("link".into(), link(s));
    }
    if s.chance(10) {
        m.insert("hyphenate".into(), json!(s.chance(50)));
    }
    let image = s.chance(20);
    if image {
        // An image in the line, in place of the text.
        m.remove("text");
        m.remove("placeholder");
        if s.chaos && s.chance(20) {
            m.insert("text".into(), json!("x"));
        }
        let mut im = Map::new();
        image_props(s, &mut im);
        for side in ["width", "height"] {
            if s.chance(90) {
                let v = match s.below(6) {
                    0 => json!(14_400),
                    1 => json!(0.01),
                    2 if s.chaos => json!(s.pick(&[0.0, -1.0, 14_400.5])),
                    _ => json!(1 + s.below(60)),
                };
                im.insert(side.into(), v);
            }
        }
        if s.chance(60) {
            im.insert("alt".into(), json!(text(s)));
        }
        m.insert("image".into(), Value::Object(im));
    }
    if !image && (20..30).contains(&kind) {
        // A note in the line, in place of the text.
        m.remove("text");
        m.remove("placeholder");
        if s.chaos && s.chance(20) {
            m.insert("text".into(), json!("x"));
        }
        if s.chance(64) {
            m.insert("attachment".into(), attachment(s));
        } else {
            let mut n = note(s);
            if s.chance(128) {
                n["contents"] = json!("In the line");
            }
            m.insert("note".into(), n);
        }
    }
    // Images in the line are aligned more often: they have the most
    // values.
    if (image && s.chance(80)) || s.chance(25) {
        let which = s.below(3);
        if which != 1 {
            let align = if image && s.chance(160) {
                match s.below(8) {
                    0 => json!(-14_400),
                    1 => json!(num(s)),
                    _ => json!(s.pick(&["middle", "textTop", "textBottom", "top", "bottom"])),
                }
            } else if s.chaos && s.chance(4) {
                json!(s.pick(&["middle", "top", ""]))
            } else {
                json!(s.pick(&["baseline", "super", "sub"]))
            };
            m.insert("verticalAlign".into(), align);
        }
        if which != 0 {
            m.insert("fontFeatures".into(), font_features(s));
        }
    }
    Value::Object(m)
}

/// OpenType features: tags the fixture fonts have or not, values on and
/// off, alternates and the bounds; with chaos, now and then one past them.
fn font_features(s: &mut Src) -> Value {
    const TAGS: &[&str] = &[
        "tnum", "pnum", "onum", "lnum", "zero", "frac", "sups", "subs", "smcp", "c2sc", "case",
        "liga", "dlig", "calt", "kern", "salt", "ss01", "cv05", "aalt", "rlig", "abcd",
    ];
    let mut m = Map::new();
    let broken = s.chaos && s.chance(4);
    if broken && s.chance(25) {
        // One feature too many.
        for k in 0..33 {
            m.insert(format!("x{k:03}"), json!(1));
        }
        return Value::Object(m);
    }
    for _ in 0..s.below(5) {
        let value = match s.below(8) {
            0 => json!(0),
            1 => json!(65535),
            2 => json!(s.below(4)),
            _ => json!(1),
        };
        m.insert(s.pick(TAGS).to_string(), value);
    }
    if broken {
        let (tag, value) = match s.below(4) {
            0 => ("tnu", json!(1)),
            1 => ("t\u{e9}m", json!(1)),
            2 => ("tnum", json!(65536)),
            _ => ("tnum", json!(-1)),
        };
        m.insert(tag.into(), value);
    }
    Value::Object(m)
}

/// A float's `shapeOutside`: each basic shape, in or past the margin
/// box; chaos makes polygons of too few or too many corners.
fn shape(s: &mut Src) -> Value {
    // Points or a percentage, in the margin box or past it; chaos takes
    // any length.
    fn len(s: &mut Src) -> Value {
        if s.chaos {
            length_pct(s)
        } else if s.chance(128) {
            json!(format!("{}%", s.below(130)))
        } else {
            json!(s.below(300) as f64 - 20.0)
        }
    }
    fn point(s: &mut Src) -> Value {
        json!([len(s), len(s)])
    }
    let mut m = Map::new();
    match s.below(4) {
        0 => {
            if s.chance(128) {
                m.insert("radius".into(), json!(format!("{}%", s.below(80))));
            }
            if s.chance(128) {
                m.insert("at".into(), point(s));
            }
            json!({ "circle": m })
        }
        1 => {
            for k in ["rx", "ry"] {
                if s.chance(128) {
                    m.insert(k.into(), json!(s.below(200)));
                }
            }
            if s.chance(128) {
                m.insert("at".into(), point(s));
            }
            json!({ "ellipse": m })
        }
        2 => {
            for k in ["top", "right", "bottom", "left"] {
                if s.chance(128) {
                    m.insert(k.into(), len(s));
                }
            }
            if s.chance(128) {
                m.insert("radius".into(), json!(s.below(60)));
            }
            json!({ "inset": m })
        }
        _ => {
            let n = if s.chaos && s.chance(32) {
                s.pick(&[1, 2, 1001])
            } else {
                3 + s.below(8)
            };
            json!({ "polygon": (0..n).map(|_| point(s)).collect::<Vec<_>>() })
        }
    }
}

/// Makes box `m` a float container: a column, with floats among its
/// `kids`, some with a shape, and text after them to wrap, directly and
/// in a box, and no `alignContent` on it or `flexBasis` on its children
/// (chaos keeps some, and some row parents).
fn floats(
    s: &mut Src,
    m: &mut Map<String, Value>,
    kids: &mut Vec<Value>,
    depth: usize,
    budget: &mut usize,
) {
    if !s.chaos {
        if let Some(Value::Object(st)) = m.get_mut("style") {
            for k in ["display", "flexDirection", "flexWrap", "alignContent"] {
                st.remove(k);
            }
        }
        for k in kids.iter_mut() {
            if let Some(Value::Object(st)) = k.get_mut("style") {
                st.remove("flexBasis");
            }
        }
    }
    let n = if s.chaos && s.chance(4) {
        40
    } else {
        1 + s.below(3)
    };
    for _ in 0..n {
        let mut f = node(s, depth + 1, budget);
        if let Value::Object(fm) = &mut f {
            if fm.get("type").and_then(Value::as_str) == Some("pageBreak") {
                continue;
            }
            if !s.chaos {
                fm.remove("break");
            }
            let st = fm
                .entry("style")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(st) = st {
                if !s.chaos {
                    st.remove("position");
                    st.remove("flexGrow");
                    st.remove("flexBasis");
                }
                st.insert("float".into(), json!(s.pick(&["left", "right"])));
                if s.chance(128) {
                    st.insert("width".into(), json!(10 + s.below(200)));
                }
                if s.chance(128) {
                    st.insert("height".into(), json!(5 + s.below(150)));
                }
                if s.chance(96) {
                    st.insert("shapeOutside".into(), shape(s));
                    if s.chance(128) {
                        let margin = if s.chaos { num(s) } else { json!(s.below(20)) };
                        st.insert("shapeMargin".into(), margin);
                    }
                }
            }
        }
        let at = s.below(kids.len() + 1);
        kids.insert(at, f);
    }
    let words = (0..=s.below(60))
        .map(|_| text(s))
        .collect::<Vec<_>>()
        .join(" ");
    let text = json!({"type": "text", "runs": [{"text": words, "font": "Inter-Regular", "size": 6 + s.below(14)}]});
    if s.chance(96) {
        // Its lines wrap beside the floats too, inside its padding.
        kids.push(
            json!({"type": "box", "style": {"padding": edges(s, nonneg)}, "children": [text]}),
        );
    } else {
        kids.push(text);
    }
    if s.chance(64) {
        kids.push(json!({"type": "box", "style": {"clear": s.pick(&["left", "right", "both"]), "height": 10}}));
    }
}

/// A form field: names mostly unique, radio buttons in two groups, some
/// fields in groups of fields (dotted names).
fn field(s: &mut Src) -> Value {
    let mut m = Map::new();
    let kind = s.pick(&["text", "checkbox", "radio", "select", "list"]);
    let choice = matches!(kind, "select" | "list");
    m.insert("kind".into(), json!(kind));
    // A group of fields, nested at most two deep.
    let set = match s.below(4) {
        0 => format!("set{}.", s.below(2)),
        1 => format!("set{}.sub{}.", s.below(2), s.below(2)),
        _ => String::new(),
    };
    let name = if s.chaos && s.chance(40) {
        // Malformed names, and a field that is also a group (`a`, `a.b`).
        json!(s.pick(&[
            "",
            "a.b",
            "a",
            "a..b",
            ".a",
            "a.",
            "f0",
            "n".repeat(129).as_str(),
            vec!["n"; 33].join(".").as_str(),
        ]))
    } else if kind == "radio" {
        json!(format!("{set}group{}", s.below(2)))
    } else {
        json!(format!("{set}f{}", s.below(1000)))
    };
    m.insert("name".into(), name);
    let mut options: Vec<Value> = (0..=s.below(4)).map(|_| json!(text(s))).collect();
    // The limits on options, and just past them.
    if s.chance(4) {
        let past = usize::from(s.chaos);
        if s.chance(128) {
            options = vec![json!("o"); 1000 + past];
        } else {
            options.push(json!("o".repeat(1000 + past)));
        }
    }
    if choice || (s.chaos && s.chance(20)) {
        m.insert("options".into(), Value::Array(options.clone()));
    }
    let multi = (kind == "list" || s.chaos) && s.chance(100);
    if multi {
        m.insert("multiSelect".into(), json!(true));
    }
    if s.chance(150) {
        let value = match kind {
            // Some of the options, each once.
            "list" if multi && !s.chaos && s.chance(160) => {
                let mut seen = Vec::new();
                for o in &options {
                    if !seen.contains(o) && s.chance(100) {
                        seen.push(o.clone());
                    }
                }
                Value::Array(seen)
            }
            "select" | "list" if !s.chaos => options[s.below(options.len())].clone(),
            "radio" => json!(s.pick(&["yes", "no", "maybe"])),
            // Values not among the options, twice, or on a field that
            // takes one value.
            _ if s.chaos && s.chance(20) => {
                let v = text(s);
                json!([v.clone(), v, options[0].clone()])
            }
            _ if s.chance(4) => json!("v".repeat(8000 + usize::from(s.chaos))),
            _ => json!(text(s)),
        };
        m.insert("value".into(), value);
    }
    if matches!(kind, "text" | "select" | "list") || s.chance(30) {
        m.insert("font".into(), font(s));
    }
    for (key, of256) in [
        ("checked", 60),
        ("multiline", 60),
        ("password", 30),
        ("readOnly", 30),
        ("required", 30),
    ] {
        let applies = matches!((key, kind), ("checked", "checkbox" | "radio"))
            || matches!((key, kind), ("multiline" | "password", "text"))
            || matches!(key, "readOnly" | "required");
        if (applies || s.chaos) && s.chance(of256) {
            m.insert(key.into(), json!(s.chance(128)));
        }
    }
    // Sorted and typed choices, unchecked spelling, and where the value
    // goes; with chaos, on fields that do not take them.
    let typed = (kind == "select" || s.chaos) && s.chance(60);
    if typed {
        m.insert("edit".into(), json!(true));
        if kind == "select" && !s.chaos && s.chance(100) {
            m.insert("value".into(), json!(text(s)));
        }
    }
    if (choice || s.chaos) && s.chance(60) {
        m.insert("sort".into(), json!(s.chance(200)));
    }
    if (kind == "text" || typed || s.chaos) && s.chance(40) {
        m.insert("noSpell".into(), json!(true));
    }
    if (matches!(kind, "text" | "select" | "list") || s.chaos) && s.chance(60) {
        m.insert("align".into(), json!(s.pick(&["left", "center", "right"])));
    }
    if (kind == "text" || s.chaos) && s.chance(40) {
        let max = if s.chaos {
            s.below(3)
        } else {
            1 + s.below(200)
        };
        m.insert("maxLength".into(), json!(max));
    }
    if s.chance(100) {
        m.insert("size".into(), font_size(s));
    }
    if s.chance(50) {
        m.insert("color".into(), color(s));
    }
    if (kind == "text" || s.chaos) && s.chance(50) {
        let (format, value) = field_format(s);
        m.insert("format".into(), format);
        if !s.chaos {
            // Formats take a single line, shown, and a value that fits.
            for key in ["multiline", "password", "maxLength"] {
                m.remove(key);
            }
        }
        match value {
            Some(v) => m.insert("value".into(), v),
            None => m.remove("value"),
        };
    }
    if s.chance(60) {
        m.insert("label".into(), json!(format!("L{}", text(s))));
    }
    Value::Object(m)
}

/// A text field format and a value that fits it, or none. In chaos mode,
/// values that do not fit, and patterns and currencies that would end a
/// script string or add a call to it.
fn field_format(s: &mut Src) -> (Value, Option<Value>) {
    const BREAKOUTS: &[&str] = &[
        "\");app.alert(1);//",
        "'",
        "\\",
        "\n",
        "\u{2028}",
        "dd\"",
        "</script>",
        "123456789",
    ];
    let mut m = Map::new();
    let kind = s.pick(&[
        "number", "percent", "date", "time", "zip", "zipPlus4", "phone", "ssn",
    ]);
    m.insert("type".into(), json!(kind));
    let value = match kind {
        "number" | "percent" => {
            if s.chance(128) {
                let decimals = if s.chaos {
                    json!(s.pick(&[11, -1, 255]))
                } else {
                    json!(s.below(11))
                };
                m.insert("decimals".into(), decimals);
            }
            if s.chance(100) {
                let sep = s.pick(&["1,234.56", "1234.56", "1.234,56", "1234,56"]);
                m.insert("separators".into(), json!(sep));
            }
            if kind == "number" || s.chaos {
                if s.chance(100) {
                    let neg = s.pick(&["minus", "red", "parens", "parensRed"]);
                    m.insert("negative".into(), json!(neg));
                }
                if s.chance(128) {
                    let currency = if s.chaos {
                        s.pick(BREAKOUTS)
                    } else {
                        s.pick(&["$", "€ ", " EUR", "CHF", ""])
                    };
                    m.insert("currency".into(), json!(currency));
                }
                if s.chance(60) {
                    m.insert("currencyBefore".into(), json!(s.chance(128)));
                }
            }
            let mut values = vec![
                "0",
                "-1234.5",
                "999.995",
                "0.125",
                "-0.004",
                "123456789012345.123456789012345",
            ];
            if s.chaos {
                values.extend(["1e5", "1,000", "", "1234567890123456"]);
            }
            json!(s.pick(&values))
        }
        "date" | "time" => {
            let pattern = if s.chaos && s.chance(100) {
                s.pick(BREAKOUTS)
            } else if kind == "date" {
                s.pick(&[
                    "dd/mm/yyyy",
                    "d mmm yy",
                    "dddd, mmmm d, yyyy",
                    "yyyy-mm-dd HH:MM tt",
                ])
            } else {
                s.pick(&["HH:MM", "h:MM tt", "HH:MM:ss", "h t"])
            };
            if !s.chaos || s.chance(200) {
                m.insert("pattern".into(), json!(pattern));
            }
            let values: &[&str] = match (kind, s.chaos) {
                ("date", false) => &["2026-10-14", "2024-02-29T23:59:59", "0001-01-01"],
                ("time", false) => &["18:30", "00:00:00", "2026-10-14T07:05"],
                _ => &["2026-02-30", "24:00", "2026-10-14T10:00Z", ""],
            };
            json!(s.pick(values))
        }
        _ => {
            let n = match kind {
                "zip" => 5,
                "phone" => 10,
                _ => 9,
            };
            let n = if s.chaos { n + 1 } else { n };
            json!((0..n)
                .map(|i| char::from(b'0' + (i % 10) as u8))
                .collect::<String>())
        }
    };
    let value = s.chance(200).then_some(value);
    (Value::Object(m), value)
}

/// A text note: contents at their limit and past it, control characters,
/// an author, a colour, open or not.
fn note(s: &mut Src) -> Value {
    let mut m = Map::new();
    let contents = match s.below(8) {
        0 => json!("n".repeat(8000 + usize::from(s.chaos))),
        1 if s.chaos => json!(""),
        2 => json!("line\nbreak\t(paren\\) \u{0} \u{1f} \u{feff}"),
        _ => json!(format!("N{}", text(s))),
    };
    m.insert("contents".into(), contents);
    if s.chance(100) {
        let author = if s.chance(20) {
            "a".repeat(256 + usize::from(s.chaos))
        } else {
            text(s)
        };
        m.insert("author".into(), json!(author));
    }
    if s.chance(100) {
        m.insert("color".into(), color(s));
    }
    if s.chance(80) {
        m.insert("open".into(), json!(s.chance(128)));
    }
    if s.chance(100) {
        let icon = if s.chaos && s.chance(40) {
            "Key"
        } else {
            s.pick(&[
                "note",
                "comment",
                "key",
                "help",
                "newParagraph",
                "paragraph",
                "insert",
            ])
        };
        m.insert("icon".into(), json!(icon));
    }
    Value::Object(m)
}

/// An attachment icon: a declared file (or, with chaos, another), an
/// icon, a colour, contents at their limit and past it.
fn attachment(s: &mut Src) -> Value {
    let mut m = Map::new();
    let file = if s.chaos && s.chance(30) {
        "missing.pdf"
    } else {
        s.pick(FILE_NAMES)
    };
    m.insert("file".into(), json!(file));
    if s.chance(128) {
        let icons = ["pushPin", "paperclip", "graph", "tag"];
        let icon = if s.chaos && s.chance(20) {
            "Paperclip"
        } else {
            s.pick(&icons)
        };
        m.insert("icon".into(), json!(icon));
    }
    if s.chance(80) {
        m.insert("color".into(), color(s));
    }
    if s.chance(80) {
        let contents = match s.below(4) {
            0 => "c".repeat(8000 + usize::from(s.chaos)),
            1 if s.chaos => String::new(),
            _ => text(s),
        };
        m.insert("contents".into(), json!(contents));
    }
    Value::Object(m)
}

/// The `attachments` of a document: each of [`FILE_NAMES`] from the store
/// or inline, with a type, a relationship and a date; with chaos, names
/// and values that are refused.
fn attachments(s: &mut Src) -> Value {
    let mut out = Vec::new();
    for (i, name) in FILE_NAMES.iter().enumerate() {
        if !s.chance(200) {
            continue;
        }
        let mut m = Map::new();
        let name = match s.below(10) {
            0 if s.chaos => "a/b",
            1 if s.chaos => FILE_NAMES[(i + 1) % FILE_NAMES.len()],
            _ => name,
        };
        m.insert("name".into(), json!(name));
        match s.below(4) {
            0 => m.insert("data".into(), json!("aGVsbG8=")),
            1 if s.chaos => m.insert("data".into(), json!("!not base64")),
            2 if s.chaos => m.insert("src".into(), json!("missing")),
            _ => m.insert(
                "src".into(),
                json!(if i == 0 { "att-xml" } else { "att-bin" }),
            ),
        };
        if s.chance(128) {
            let mime = if s.chaos && s.chance(30) {
                "text"
            } else {
                "text/xml"
            };
            m.insert("mimeType".into(), json!(mime));
        }
        if s.chance(64) {
            let rel = s.pick(&["source", "data", "alternative", "supplement", "unspecified"]);
            m.insert("relationship".into(), json!(rel));
        }
        if s.chance(64) {
            let date = if s.chaos && s.chance(40) {
                "yesterday"
            } else {
                "2026-10-17T09:00:00+02:00"
            };
            m.insert("modified".into(), json!(date));
        }
        if s.chance(64) {
            m.insert("description".into(), json!(text(s)));
        }
        out.push(Value::Object(m));
    }
    Value::Array(out)
}

/// Pages that show a node: a keyword, or page numbers counted from either
/// end.
fn page_filter(s: &mut Src) -> Value {
    if s.chance(100) {
        return json!(s.pick(&["first", "last", "notFirst", "notLast", "odd", "even"]));
    }
    let n = if s.chaos { s.below(3) } else { 1 + s.below(3) };
    let pages: Vec<Value> = (0..n)
        .map(|_| match s.below(8) {
            0 if s.chaos => json!(0),
            1 => json!(s.pick(&[i64::from(i32::MAX), i64::from(i32::MIN)])),
            _ => {
                let page = 1 + s.below(3) as i64;
                json!(if s.chance(100) { -page } else { page })
            }
        })
        .collect();
    Value::Array(pages)
}

fn cell(s: &mut Src, depth: usize, budget: &mut usize) -> Value {
    let mut m = Map::new();
    if s.chance(30) {
        m.insert("colSpan".into(), json!(1 + s.below(3)));
    }
    if s.chance(30) {
        m.insert("rowSpan".into(), json!(1 + s.below(3)));
    }
    if s.chance(100) {
        let mut st = style(s, false);
        if let (false, Value::Object(st)) = (s.chaos, &mut st) {
            // Not on table cells.
            st.remove("transform");
            st.remove("transformOrigin");
            st.remove("zIndex");
        }
        m.insert("style".into(), st);
    }
    if s.chance(40) {
        m.insert("header".into(), json!(true));
    }
    let flow = std::mem::replace(&mut s.flow, false);
    let kids: Vec<Value> = (0..s.below(3))
        .map(|_| node(s, depth + 1, budget))
        .collect();
    s.flow = flow;
    m.insert("children".into(), Value::Array(kids));
    Value::Object(m)
}

fn table(s: &mut Src, depth: usize, budget: &mut usize) -> Map<String, Value> {
    let mut m = Map::new();
    let ncols = if s.chaos { s.below(5) } else { 1 + s.below(4) };
    let cols: Vec<Value> = (0..ncols)
        .map(|_| match s.below(4) {
            0 => json!({"width": "auto"}),
            1 => json!({"width": format!("{}fr", 1 + s.below(3))}),
            _ => json!({"width": 10 + s.below(200)}),
        })
        .collect();
    m.insert("columns".into(), Value::Array(cols));
    let nrows = if s.chance(10) {
        50 + s.below(300)
    } else {
        s.below(6)
    };
    if s.chance(100) {
        m.insert("headerRows".into(), json!(s.below(3)));
    }
    let mut rows = Vec::new();
    for _ in 0..nrows {
        let mut r = Map::new();
        if s.chance(30) {
            r.insert("minHeight".into(), nonneg(s));
        }
        if s.chance(30) {
            r.insert("backgroundColor".into(), color(s));
        }
        let n = if s.chance(200) {
            ncols
        } else {
            s.below(ncols + 2)
        };
        let cells: Vec<Value> = (0..n).map(|_| cell(s, depth, budget)).collect();
        r.insert("cells".into(), Value::Array(cells));
        rows.push(Value::Object(r));
    }
    m.insert("rows".into(), Value::Array(rows));
    m
}

/// A chain of boxes, one inside the other, around a leaf: the general
/// recursion stops at depth 7, and this reaches the parser's nesting
/// limit (128 JSON levels, two per box), just under it and past it.
fn deep(s: &mut Src, budget: &mut usize) -> Value {
    let levels = s.pick(&[20, 50, 57, 58, 59, 60, 61, 62, 63, 80]);
    let style = match s.below(4) {
        0 => json!({"flexDirection": "row"}),
        1 => json!({"padding": 1, "borderWidth": 1}),
        2 => json!({"display": "grid"}),
        _ => json!({}),
    };
    let mut inner = node(s, 7, budget);
    for _ in 0..levels {
        inner = json!({"type": "box", "style": style, "children": [inner]});
    }
    inner
}

/// Every structure role a node takes.
const ROLES: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "section",
    "blockQuote",
    "caption",
    "figure",
    "list",
    "listItem",
    "label",
    "listBody",
    "artifact",
];

fn node(s: &mut Src, depth: usize, budget: &mut usize) -> Value {
    if *budget == 0 || s.exhausted() {
        return json!({"type": "spacer"});
    }
    *budget -= 1;
    let kind = if depth > 6 {
        s.pick(&["text", "spacer"])
    } else if s.flow || s.chaos {
        s.pick(&[
            "box",
            "box",
            "text",
            "text",
            "image",
            "table",
            "field",
            "note",
            "pageBreak",
            "spacer",
        ])
    } else {
        s.pick(&[
            "box",
            "box",
            "text",
            "text",
            "image",
            "table",
            "field",
            "note",
            "spacer",
            "attachment",
        ])
    };
    let mut m = Map::new();
    m.insert("type".into(), json!(kind));
    if s.chance(20) {
        m.insert("id".into(), json!(format!("n{}", s.below(4))));
    }
    if kind != "pageBreak" && s.chance(180) {
        m.insert("style".into(), style(s, kind == "text"));
    }
    if kind != "pageBreak" && (s.flow || s.chaos) && s.chance(50) {
        m.insert("break".into(), brk(s));
    }
    // Fields and notes take no link or bookmark.
    let linkable = !matches!(kind, "pageBreak" | "field" | "note") || s.chaos;
    if linkable && s.chance(20) {
        m.insert("link".into(), link(s));
    }
    if linkable && (s.flow || s.chaos) && s.chance(20) {
        let level = if s.chaos { s.below(40) } else { 1 + s.below(4) };
        m.insert(
            "bookmark".into(),
            json!({"title": format!("B{}", text(s)), "level": level}),
        );
    }
    if kind != "pageBreak" && s.chance(8) {
        m.insert("debug".into(), json!(true));
    }
    if kind != "pageBreak" && s.chance(16) {
        m.insert("pages".into(), page_filter(s));
    }
    // Tagged output: what each node is. Chaos puts any role anywhere.
    if (matches!(kind, "box" | "text" | "image") || s.chaos) && s.chance(30) {
        let role = match kind {
            _ if s.chaos => s.pick(ROLES),
            "text" => s.pick(&["h1", "h2", "h3", "p", "caption", "label", "artifact"]),
            "image" => s.pick(&["figure", "artifact"]),
            _ => s.pick(ROLES),
        };
        m.insert("role".into(), json!(role));
        // A figure box takes `alt`; now and then another box has one too.
        if kind == "box" && ((role == "figure" && s.chance(70)) || s.chance(2)) {
            m.insert("alt".into(), json!(format!("Box {}", text(s))));
        }
    }
    match kind {
        "box" => {
            if s.chance(24) {
                let mut bg = Map::new();
                image_props(s, &mut bg);
                m.insert("backgroundImage".into(), Value::Object(bg));
            }
            let n = if s.chance(15) {
                40 + s.below(200)
            } else {
                s.below(5)
            };
            let mut kids: Vec<Value> = if depth < 4 && s.chance(4) {
                vec![deep(s, budget)]
            } else {
                (0..n).map(|_| node(s, depth + 1, budget)).collect()
            };
            let areas = m
                .get("style")
                .is_some_and(|st| st.get("gridTemplateAreas").is_some());
            if areas {
                for k in &mut kids {
                    if !s.chance(160) {
                        continue;
                    }
                    if let Some(Value::Object(st)) = k.get_mut("style") {
                        st.remove("gridRow");
                        st.remove("gridColumn");
                        st.insert("gridArea".into(), json!(s.pick(&["a", "b"])));
                    } else if let Value::Object(k) = k {
                        if k.get("type").and_then(Value::as_str) != Some("pageBreak") {
                            k.insert("style".into(), json!({"gridArea": s.pick(&["a", "b"])}));
                        }
                    }
                }
            }
            if s.chance(40) {
                floats(s, &mut m, &mut kids, depth, budget);
            }
            m.insert("children".into(), Value::Array(kids));
        }
        "text" => {
            let runs: Vec<Value> = (0..s.below(4)).map(|_| run(s)).collect();
            m.insert("runs".into(), Value::Array(runs));
        }
        "field" => {
            m.insert("field".into(), field(s));
        }
        "note" => {
            m.insert("note".into(), note(s));
        }
        "attachment" => {
            m.insert("attachment".into(), attachment(s));
        }
        "image" => {
            image_props(s, &mut m);
            if s.chance(60) {
                m.insert("alt".into(), json!(text(s)));
            }
        }
        "table" => {
            m.extend(table(s, depth, budget));
            if s.chance(60) {
                let st = m.entry("style").or_insert_with(|| json!({}));
                if let Value::Object(st) = st {
                    st.insert("borderCollapse".into(), json!("separate"));
                    if s.chance(160) {
                        let sp = if s.chaos { num(s) } else { nonneg(s) };
                        st.insert("borderSpacing".into(), sp);
                    }
                }
            }
        }
        _ => {}
    }
    Value::Object(m)
}

/// The image properties shared by `image` nodes and `backgroundImage`:
/// `src` or `data`, `fit` and `objectPosition`.
fn image_props(s: &mut Src, m: &mut Map<String, Value>) {
    if s.chaos && s.chance(30) {
        m.insert(
            "data".into(),
            json!(s.pick(&[
                "",
                "iVBORw0KGgo=",
                "data:image/png;base64,AAAA",
                "!!",
                "data:image/svg+xml;base64,PHN2Zy8+",
                "PHN2ZyB3aWR0aD0iMCIvPg==",
            ])),
        );
    } else {
        m.insert(
            "src".into(),
            json!(if s.chaos {
                s.pick(&["bad", "badsvg", "missing"])
            } else {
                s.pick(&[
                    "logo",
                    "palette",
                    "photo",
                    "photo-cmyk",
                    "photo-rot6",
                    "photo-lossy",
                    "badge-lossless",
                    "anim",
                    "checker-la",
                    "chart",
                    "markers",
                    "svgtext",
                ])
            }),
        );
    }
    if s.chance(128) {
        m.insert(
            "fit".into(),
            json!(s.pick(&["fill", "contain", "cover", "none"])),
        );
    }
    if s.chance(96) {
        let mut axis = |keywords: [&str; 3]| match s.below(4) {
            0 => json!(s.pick(&keywords)),
            1 => json!(s.pick(&["0%", "25%", "100%", "-50%", "1000.5%"])),
            // The bounds (±10000 %, ±14400 pt), just past them, and
            // values far past them that would overflow f32 if
            // validation let them through.
            2 => match s.below(8) {
                0 => json!("10000%"),
                1 => json!("-10000%"),
                2 => json!("10000.01%"),
                3 => json!("-9999999999999999999999999999999999999%"),
                4 => json!(14_400),
                5 => json!(-14_400),
                6 => json!(14_400.01),
                _ => json!(-1e30),
            },
            _ => num(s),
        };
        let x = axis(["left", "center", "right"]);
        let y = axis(["top", "center", "bottom"]);
        m.insert("objectPosition".into(), json!({ "x": x, "y": y }));
    }
}

/// A schema-shaped document driven by `data`: mostly valid, with edge
/// values (zero, negative, huge) and occasional schema violations.
pub fn document(data: &[u8]) -> Vec<u8> {
    let s = &mut Src::new(data);
    let mut budget = 2_000;
    let mut pages = Vec::new();
    for _ in 0..=s.below(3) {
        let mut p = Map::new();
        let size = match s.below(8) {
            7 if s.chaos => json!({"width": num(s)}),
            6 | 7 if s.chaos => json!({"width": num(s), "height": num(s)}),
            5 => json!({"width": 1 + s.below(100), "height": 1 + s.below(100)}),
            4 => json!({"width": 100 + s.below(400), "height": 50 + s.below(300)}),
            3 => json!({"width": 612, "height": 792}),
            2 => json!({"width": 14_400, "height": 14_400}),
            _ => json!({"width": 595.28, "height": 841.89}),
        };
        p.insert("size".into(), size);
        if s.chance(100) {
            p.insert(
                "margin".into(),
                edges(s, if s.chaos { num } else { nonneg }),
            );
        }
        if s.chance(30) {
            p.insert("background".into(), color(s));
        }
        if s.chance(10) {
            p.insert("debug".into(), json!(true));
        }
        s.flow = false;
        if s.chance(60) {
            p.insert("header".into(), node(s, 3, &mut budget));
        }
        if s.chance(60) {
            p.insert("footer".into(), node(s, 3, &mut budget));
        }
        s.flow = true;
        let kids: Vec<Value> = (0..s.below(8)).map(|_| node(s, 0, &mut budget)).collect();
        p.insert("children".into(), Value::Array(kids));
        pages.push(Value::Object(p));
    }
    let mut meta = Map::new();
    if s.chance(60) {
        meta.insert("lang".into(), json!(s.pick(&["fr", "en", "de", "zz-ZZ"])));
    }
    if s.chance(30) {
        meta.insert(
            "creationDate".into(),
            json!(s.pick(&["2026-01-01T00:00:00Z", "2026-13-40T99:00:00Z", "bad"])),
        );
    }
    // The title is mostly printable ASCII, which the plain file shows as
    // a literal string and the encrypted one must not.
    if s.chance(128) {
        meta.insert(
            "title".into(),
            json!(s.pick(&[
                "Fuzz-Title",
                "Quarterly-Report-2026",
                "/JS (x)",
                "/AA << /JS (app.alert(1);) >>",
                "Ünïcødé ☃ 😀",
                "a(b)c\\d",
                "",
            ])),
        );
    }
    for key in ["author", "subject", "keywords", "creator", "producer"] {
        if s.chance(40) {
            let value = match s.below(6) {
                0 => "x".repeat(s.below(5000)),
                1 => "tab\there\nnew line\u{0}nul".to_string(),
                2 => "שלום, مرحبا, 你好".to_string(),
                3 => String::new(),
                _ => format!("Fuzz {key}"),
            };
            meta.insert(key.into(), json!(value));
        }
    }
    // An unknown value takes a high byte: an exhausted source reads zeros,
    // and would otherwise break every chaos document here.
    if s.chance(40) {
        let modes = [
            "useNone",
            "useOutlines",
            "useThumbs",
            "fullScreen",
            "useAttachments",
        ];
        let mode = if s.chaos && !s.chance(196) {
            "useFiles"
        } else {
            s.pick(&modes)
        };
        meta.insert("pageMode".into(), json!(mode));
    }
    if s.chance(40) {
        let layouts = [
            "singlePage",
            "oneColumn",
            "twoColumnLeft",
            "twoColumnRight",
            "twoPageLeft",
            "twoPageRight",
        ];
        let layout = if s.chaos && !s.chance(196) {
            "twoColumn"
        } else {
            s.pick(&layouts)
        };
        meta.insert("pageLayout".into(), json!(layout));
    }
    let fonts = fixture_fonts();
    let fallbacks: &[&str] = match s.below(6) {
        0 => &[],
        1 => &["NotoSans-Regular", "EmojiTest-COLR"],
        2 => &["EmojiTest-CBDT", "EmojiTest-COLR"],
        4 => &["EmojiTest-SVG", "EmojiTest-COLR"],
        3 => &[
            "NotoSansArabic-Regular",
            "NotoSansHebrew-Regular",
            "EmojiTest-sbix",
        ],
        _ => &["Inter-Regular", "EmojiTest-CBDT"],
    };
    let emoji = match s.below(6) {
        0 => json!({"😀": "logo", "🇫🇷": "chart", "👩\u{200D}💻": "palette", "❤\u{FE0F}": "photo"}),
        1 if s.chaos => json!({"a": "logo", "😀😀": "logo", "😀": "missing", "👍🏽": "bad"}),
        _ => json!({}),
    };
    let mut doc = json!({
        "version": 1,
        "meta": meta,
        "fonts": fonts,
        "fallbackFonts": fallbacks,
        "emojiImages": emoji,
        "pages": pages,
    });
    if s.chance(60) {
        doc["attachments"] = attachments(s);
    }
    if s.chance(30) {
        let prefix = if s.chaos && s.chance(40) { "dc" } else { "fx" };
        doc["meta"]["xmpExtensions"] = json!([{
            "schema": "Factur-X PDFA Extension Schema",
            "namespace": "urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#",
            "prefix": prefix,
            "properties": [{"name": "DocumentType", "value": "INVOICE", "description": "INVOICE"}]
        }]);
    }
    doc.to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_renders_follow_plain_ones() {
        let valid = br#"{"version":1,"meta":{"title":"Canary-Title"},"fonts":[{"id":"Inter-Regular","family":"Inter"}],
            "pages":[{"size":{"width":100,"height":100},"children":[{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":9}]}]}]}"#;
        let invalid = br#"{"version":1,"pages":[{"size":{"width":-1,"height":100}}]}"#;
        for doc in [&valid[..], &invalid[..], b"not json"] {
            let plain = render(doc, InputFormat::Json, None);
            let value = serde_json::from_slice::<Value>(doc).ok();
            check_encrypted(doc, &plain, value.as_ref(), &FONTS, &IMAGES);
        }
        // The plain render shows the title, so the check above looked for it.
        let plain = render(valid, InputFormat::Json, None).unwrap();
        assert!(plain.windows(14).any(|w| w == b"(Canary-Title)"));
    }

    #[test]
    fn scripts_are_checked_against_the_form_call_grammar() {
        for ok in [
            r#"AFNumber_Format(2, 0, 3, 0, "\u20ac ", true);"#,
            r#"AFDate_FormatEx("dd/mm/yyyy");"#,
            r#"AFNumber_Keystroke(0, 1, 0, 0, "\"\'\\", false);"#,
            "AFSpecial_Format(2);",
        ] {
            assert!(form_call(ok), "{ok}");
        }
        for bad in [
            r#"AFDate_FormatEx("dd");app.alert(1);//");"#,
            r#"AFDate_FormatEx("dd" + x);"#,
            r"app.alert(1);",
            r"AFSpecial_Format(2)",
            r"AFSpecial_Format(x);",
            r#"AFDate_FormatEx("d'd");"#,
            r#"AFDate_FormatEx("\x41");"#,
            "AFDate_FormatEx(\"\u{2028}\");",
        ] {
            assert!(!form_call(bad), "{bad}");
        }
        let literal = br"(AF\(1\) (a) \134 \101b)rest";
        assert_eq!(
            pdf_string(literal, 0),
            Some((br"AF(1) (a) \ Ab".to_vec(), literal.len() - 4))
        );
        check_scripts(b"/JS (AFSpecial_Format\\(0\\);)", false);
    }

    /// Scripts are read from the parsed object syntax: in hex (UTF-16BE,
    /// what a script outside printable ASCII is written as) too, and never
    /// from strings or stream data that only look like a `/JS` key.
    #[test]
    fn scripts_are_read_from_the_object_syntax() {
        let fails = |pdf: &'static [u8], pdf_a: bool| {
            std::panic::catch_unwind(|| check_scripts(pdf, pdf_a)).is_err()
        };
        // "AFSpecial_Format(0);" and "app.alert(1);" in UTF-16BE.
        let ok = b"<< /JS <FEFF00410046005300700065006300690061006C005F0046006F0072006D00610074002800300029003B> >>";
        let bad = b"<< /JS <FEFF006100700070002E0061006C006500720074002800310029003B> >>";
        assert!(!fails(ok, false));
        assert!(fails(bad, false));
        assert!(fails(ok, true));
        assert!(fails(b"<< /AA << >> >>", true));
        assert!(fails(b"<< /AA 5 0 R >>", true));
        assert!(fails(b"<< /Names << /JavaScript 6 0 R >> >>", true));
        assert!(fails(b"<< /S /JavaScript >>", true));
        assert!(fails(b"<< /JS 12 0 R >>", false));
        // Strings and stream data that look like a script.
        for decoy in [
            &b"<< /Title (/JS \\(x\\)) /Subject </4A53> >>"[..],
            b"1 0 obj << /Length 8 >> stream\n/JS (x) \nendstream endobj",
            b"1 0 obj << /Length 2 0 R >> stream\n/JS (x)\nendstream endobj",
            // Fonts named `AA` and `JavaScript`.
            b"<< /Type /Font /BaseFont /AA\n /Encoding /Identity-H >>",
            b"<< /FontName /JavaScript >>",
            b"<< /S /D /St 1 >> << /BaseFont /JavaScript >>",
        ] {
            check_scripts(decoy, false);
            check_scripts(decoy, true);
        }
        // A valid document whose title holds one renders: `render` runs
        // the oracle on it.
        for title in ["/JS (x)", "/AA << /JS (app.alert(1);) >>"] {
            let doc = json!({"version": 1, "meta": {"title": title}, "pages": [{"size": {"width": 100, "height": 100}}]});
            let doc = doc.to_string();
            for pdf_a in [None, Some(PdfALevel::A2b)] {
                render(doc.as_bytes(), InputFormat::Json, pdf_a).unwrap();
            }
        }
    }

    /// The `render_json` seeds (`fixtures/docs`) validate with the
    /// harness's fonts and images, so they reach layout.
    #[test]
    fn seed_documents_validate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/docs");
        let mut seeds = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let doc = std::fs::read(&path).unwrap();
            let errors = inkoxide::validate(&doc, InputFormat::Json, &FONTS, &IMAGES);
            assert!(errors.is_empty(), "{}: {errors:?}", path.display());
            seeds += 1;
        }
        assert!(seeds > 30, "{seeds} seeds");
    }

    #[test]
    fn check_image_says_why_it_did_not_render() {
        let Err(InkoxideError::Validation(e)) = check_image(b"abc") else {
            panic!("rendered an image that is not one");
        };
        assert!(e.message.contains("unsupported image format"), "{e}");
        assert!(check_image(include_bytes!("../../fixtures/images/logo.png")).is_ok());
        // SVG text is laid out with the fixture fonts.
        assert!(check_image(include_bytes!("../../fixtures/images/text.svg")).is_ok());
    }

    /// Colour fonts register, and render as the run's font and as the
    /// fallback for emoji sequences.
    #[test]
    fn check_font_renders_colour_fonts() {
        for (id, _, _, _, bytes) in &FIXTURE_FONTS[FIXTURE_FONTS.len() - COLOUR_FONTS..] {
            assert_eq!(check_font(bytes), 1, "{id}");
        }
    }

    /// The generator reaches the newer parts of the schema with documents
    /// that validate.
    #[test]
    fn document_covers_the_schema() {
        const FEW: usize = 2000;
        const MOST: usize = 20_000;
        let features = [
            r#""type":"field""#,
            r#""type":"note""#,
            r#""open":true"#,
            r#""kind":"select""#,
            r#""kind":"list""#,
            r#""multiSelect":true"#,
            r#""edit":true"#,
            r#""sort":true"#,
            r#""noSpell":true"#,
            r#""align":"right""#,
            r#""icon":"insert""#,
            r#""value":["#,
            r#""kind":"radio""#,
            r#""name":"set0.sub"#,
            r#""type":"number""#,
            r#""currency":"#,
            r#""pattern":"#,
            r#""type":"zip"#,
            r#""pages":"odd""#,
            r#""pages":[-"#,
            r#""zIndex":"#,
            r#""orphans":"#,
            r#""widows":"#,
            r#""direction":"rtl""#,
            r#""topLeft":"#,
            r#""borderStyle":{"#,
            r#""decorationStyle":"wavy""#,
            r#""decorationColor":"#,
            r#""decorationThickness":"#,
            r#""whiteSpace":"pre-wrap""#,
            r#""verticalAlign":"super""#,
            r#""fontFeatures":{""#,
            r#""textWrap":"pretty""#,
            "beautiful. In olden",
            r#""float":"left""#,
            r#""clear":""#,
            r#""shapeOutside":{"polygon":"#,
            r#""shapeMargin":"#,
            r#""textWrap":"balance""#,
            r#""hyphenationPenalty":"#,
            r#""backgroundImage":{"#,
            r#""fallbackFonts":["EmojiTest-CBDT""#,
            r#""image":{"src":"#,
            r#""contents":"In the line""#,
            r#""emojiImages":{"❤"#,
            r#""😀":"logo""#,
            r#""verticalAlign":"textTop""#,
            "👩\u{200D}💻",
            r#""src":"svgtext""#,
            r#""src":"photo-cmyk""#,
            r#""title":"Fuzz-Title""#,
            r#""subject":"#,
            r#""keywords":"#,
            r#""creator":"#,
            r#""producer":"#,
            r#""pageMode":"#,
            r#""pageLayout":"twoPage"#,
            r#""EmojiTest-sbix"]"#,
            r#""EmojiTest-SVG","#,
            // Alt on an image node (keys are sorted) and on an image in text.
            r#"[{"alt":"#,
            r#""image":{"alt":"#,
            // Text at instances of the variable fonts, past an axis's end
            // and in CFF2.
            r#""font":"VarTest-max""#,
            r#""font":"VarCFF2-100""#,
            // Files, their icons and the PDF/A metadata that describes them.
            r#""type":"attachment""#,
            r#""attachments":[{"#,
            r#""xmpExtensions":[{"#,
            // Tagged output: roles, a figure box's alt, a field's label and
            // a header cell.
            r#""role":""#,
            r#""role":"listItem""#,
            r#""alt":"Box "#,
            r#""label":"L"#,
            r#""header":true"#,
        ];
        // Boxes nested close to the parser's limit (keys are sorted).
        let deep = r#"{"children":["#.repeat(56);
        let features: Vec<&str> = features.into_iter().chain([&deep[..]]).collect();
        let mut seen = vec![false; features.len()];
        let mut valid = 0;
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        // Documents until every feature has shown up in a valid one, and at
        // least `FEW`; at most `MOST`, so a feature the generator no longer
        // reaches fails the test instead of hanging it.
        let mut tried = 0;
        while tried < MOST && (tried < FEW || seen.contains(&false)) {
            let data: Vec<u8> = (0..64 << (tried % 5))
                .map(|_| {
                    // xorshift64
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect();
            tried += 1;
            let doc = document(&data);
            if !inkoxide::validate(&doc, InputFormat::Json, &FONTS, &IMAGES).is_empty() {
                continue;
            }
            valid += 1;
            let json = String::from_utf8(doc).unwrap();
            for (f, seen) in features.iter().zip(&mut seen) {
                *seen |= json.contains(f);
            }
        }
        let missing: Vec<_> = features
            .iter()
            .zip(&seen)
            .filter(|(_, seen)| !**seen)
            .collect();
        assert!(
            missing.is_empty(),
            "no valid document of {tried} ({valid} valid) has {missing:?}"
        );
        assert!(
            valid * 10 >= tried,
            "{valid} of {tried} documents are valid"
        );
    }
}
