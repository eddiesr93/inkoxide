use super::*;

fn fixture(path: &str) -> Vec<u8> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read(format!("{root}{path}")).unwrap()
}

/// Registers every fixture font, as the contract tests do.
fn register_fonts() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/fonts");
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if let Some(id) = name.strip_suffix(".ttf") {
            register_font(id.as_bytes(), &fixture(&format!("fonts/{name}"))).unwrap();
        }
    }
}

/// The failure of a render that should fail.
fn failed(r: Result<Vec<u8>, NotRendered>) -> Failure {
    match r {
        Err(NotRendered::Failed(f)) => f,
        Err(NotRendered::Needs(missing)) => panic!("needs {missing}"),
        Ok(_) => panic!("rendered"),
    }
}

fn images_buffer(images: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = (images.len() as u32).to_le_bytes().to_vec();
    for (id, bytes) in images {
        buf.extend((id.len() as u32).to_le_bytes());
        buf.extend(id.as_bytes());
        buf.extend((bytes.len() as u32).to_le_bytes());
        buf.extend(*bytes);
    }
    buf
}

/// Calls an `ink_*` export with buffers from `ink_alloc`, as JavaScript
/// does, and returns its status and result.
fn call(f: impl FnOnce(&mut dyn FnMut(&[u8]) -> (*mut u8, usize)) -> u32) -> (u32, Vec<u8>) {
    let mut alloc = |bytes: &[u8]| {
        let ptr = ink_alloc(bytes.len());
        // SAFETY: `ink_alloc` gave `bytes.len()` writable bytes at `ptr`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
        (ptr, bytes.len())
    };
    let status = f(&mut alloc);
    // SAFETY: the result stays put until the next call.
    let result = unsafe { std::slice::from_raw_parts(ink_result_ptr(), ink_result_len()) }.to_vec();
    ink_result_free();
    (status, result)
}

#[test]
fn renders_as_the_engine_does() {
    register_fonts();
    let doc = fixture("docs/invoice.json");
    let options = br#"{"creationDate":"2026-10-08T00:00:00Z","compressionLevel":9}"#;
    let (status, pdf) = call(|alloc| {
        let (d, dl) = alloc(&doc);
        let (o, ol) = alloc(options);
        let (i, il) = alloc(&[]);
        // SAFETY: buffers from `ink_alloc`, each passed once.
        unsafe { ink_render(d, dl, o, ol, i, il) }
    });
    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&pdf));
    let core = inkoxide::RenderOptions::default()
        .with_creation_date("2026-10-08T00:00:00Z")
        .with_compression_level(9);
    let native = FONTS.with(|f| {
        inkoxide::render(
            &doc,
            inkoxide::InputFormat::Json,
            &core,
            &f.borrow(),
            &inkoxide::ImageStore::new(),
        )
    });
    assert!(pdf == native.unwrap(), "the binding changed the output");
}

#[test]
fn errors_are_json() {
    let (status, err) = call(|alloc| {
        let (d, dl) = alloc(b"{");
        let (o, ol) = alloc(b"");
        let (i, il) = alloc(b"");
        // SAFETY: buffers from `ink_alloc`, each passed once.
        unsafe { ink_render(d, dl, o, ol, i, il) }
    });
    assert_eq!(status, 1);
    let err: serde_json::Value = serde_json::from_slice(&err).unwrap();
    assert_eq!(err["code"], "PARSE_ERROR");
    assert_eq!(err["path"], "");
}

/// A valid document of one empty page.
const EMPTY: &[u8] =
    br#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[]}]}"#;

#[test]
fn options_are_checked_at_their_path() {
    let path = |json: &str| failed(render(EMPTY, json.as_bytes(), b""));
    for (json, at) in [
        ("[]", "options"),
        (r#"{"threads":"4"}"#, "options.threads"),
        (r#"{"threads":null}"#, "options.threads"),
        (r#"{"threads":2}"#, "options.threads"),
        (r#"{"threads":"NaN"}"#, "options.threads"),
        (
            r#"{"compressionLevel":"Infinity"}"#,
            "options.compressionLevel",
        ),
        (r#"{"compress":"no"}"#, "options.compress"),
        (r#"{"pdfA":5}"#, "options.pdfA"),
        (r#"{"pdfA":"4z"}"#, "options.pdfA"),
        (r#"{"format":"yaml"}"#, "options.format"),
        (r#"{"encryption":null}"#, "options.encryption"),
        (
            r#"{"encryption":{"userPassword":"u","permissions":{"copying":"yes"}}}"#,
            "options.encryption.permissions.copying",
        ),
        (r#"{"creationDate":"yesterday"}"#, "options.creationDate"),
    ] {
        let f = path(json);
        assert_eq!(
            (f.code.as_str(), f.path.as_str()),
            ("INVALID_ARGUMENT", at),
            "{json}: {f:?}"
        );
    }
    // NaN reads as the engine's own message reads it, without the value
    // of anything else.
    assert!(path(r#"{"threads":"NaN"}"#).message.ends_with("found NaN"));
    let secret = path(r#"{"encryption":{"userPassword":"hunter2","permissions":{"copying":1}}}"#);
    assert!(!secret.message.contains("hunter2"));
}

#[test]
fn msgpack_is_a_parse_error() {
    let f = failed(render(&[0x80], br#"{"format":"msgpack"}"#, b""));
    assert_eq!(f.code, "PARSE_ERROR");
    let errors: serde_json::Value =
        serde_json::from_str(&validate(&[0x80], b"msgpack", None).unwrap()).unwrap();
    assert_eq!(errors[0]["code"], "PARSE_ERROR");
}

#[test]
fn images_come_in_one_buffer() {
    let png = fixture("images/logo.png");
    let store = images::parse(&images_buffer(&[("logo", &png), ("é", b"x")])).unwrap();
    let doc = br#"{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[
        {"type":"image","src":"logo","style":{"width":10,"height":10}}]}]}"#;
    let fonts = FontStore::new();
    assert!(inkoxide::validate(doc, inkoxide::InputFormat::Json, &fonts, &store).is_empty());
    let good = images_buffer(&[("logo", &png)]);
    for bad in [
        &good[..good.len() - 1],
        &[good.as_slice(), &[0]].concat()[..],
        &[1, 0, 0, 0, 9, 0, 0, 0][..],
    ] {
        assert_eq!(images::parse(bad).unwrap_err().code, "INTERNAL");
    }
}

#[test]
fn version_is_json() {
    let v: serde_json::Value = serde_json::from_str(&version()).unwrap();
    assert_eq!(v["schema"], 1);
    assert_eq!(v["engine"], inkoxide::ENGINE_VERSION);
}

/// A render that needs built-in hyphenation patterns names their files with
/// status 2, and renders once they are registered. (In a workspace build the
/// engine may embed them, and render at once.)
#[test]
fn hyphenation_patterns_come_when_a_render_needs_them() {
    register_fonts();
    let doc = br#"{"version":1,"meta":{"lang":"de"},"fonts":[{"id":"Inter-Regular","family":"Inter"}],
      "pages":[{"size":{"width":200,"height":400},"children":[{"type":"text","style":{"width":80,"hyphenate":true},
      "runs":[{"text":"Donaudampfschifffahrtsgesellschaft","font":"Inter-Regular","size":10}]}]}]}"#;
    let call_render = || {
        call(|alloc| {
            let (d, dl) = alloc(doc);
            let (o, ol) = alloc(b"");
            let (i, il) = alloc(b"");
            // SAFETY: buffers from `ink_alloc`, each passed once.
            unsafe { ink_render(d, dl, o, ol, i, il) }
        })
    };
    let (status, result) = call_render();
    if status == 2 {
        assert_eq!(result, br#"{"fonts":[],"hyphenation":["de"]}"#);
        assert_eq!(hyphenation_file(b"de-AT"), "de");
        let automaton = fixture("../crates/core/hyphenation/de.bin");
        let (status, _) = call(|alloc| {
            let (f, fl) = alloc(b"de");
            let (a, al) = alloc(&automaton);
            // SAFETY: buffers from `ink_alloc`, each passed once.
            unsafe { ink_register_builtin_hyphenation(f, fl, a, al) }
        });
        assert_eq!(status, 0);
        assert_eq!(call_render().0, 0);
    } else {
        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&result));
    }
}
