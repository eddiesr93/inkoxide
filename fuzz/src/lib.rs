//! Shared harness for the fuzz targets: fixture fonts and images, the
//! render oracle, and a structure-aware document generator.

use std::sync::LazyLock;

pub mod svg;

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, RenderOptions};
use serde_json::{json, Map, Value};

/// The fixture fonts: id, family, weight and bytes. Generated documents
/// declare them all, and their runs pick from them.
const FIXTURE_FONTS: &[(&str, &str, u16, &[u8])] = &[
    (
        "Inter-Regular",
        "Inter",
        400,
        include_bytes!("../../fixtures/fonts/Inter-Regular.ttf"),
    ),
    (
        "Inter-Bold",
        "Inter",
        700,
        include_bytes!("../../fixtures/fonts/Inter-Bold.ttf"),
    ),
    (
        "NotoSans-Regular",
        "Noto Sans",
        400,
        include_bytes!("../../fixtures/fonts/NotoSans-Regular.ttf"),
    ),
    (
        "RobotoMono-Regular",
        "Roboto Mono",
        400,
        include_bytes!("../../fixtures/fonts/RobotoMono-Regular.ttf"),
    ),
];

pub static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let mut f = FontStore::new();
    for &(id, _, _, bytes) in FIXTURE_FONTS {
        f.register(id, bytes.to_vec()).unwrap();
    }
    f
});

pub static IMAGES: LazyLock<ImageStore> = LazyLock::new(|| {
    let mut s = ImageStore::new();
    s.insert(
        "logo",
        include_bytes!("../../fixtures/images/logo.png").to_vec(),
    );
    s.insert(
        "palette",
        include_bytes!("../../fixtures/images/palette.png").to_vec(),
    );
    s.insert(
        "photo",
        include_bytes!("../../fixtures/images/photo-gray.jpg").to_vec(),
    );
    s.insert(
        "chart",
        include_bytes!("../../fixtures/images/chart.svg").to_vec(),
    );
    s.insert("bad", b"\x89PNG\r\n\x1a\nnot really".to_vec());
    s.insert(
        "badsvg",
        b"<svg xmlns='http://www.w3.org/2000/svg'><text>hi</text></svg>".to_vec(),
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
    // Half the inputs also go through the PDF/A writer.
    let pdf_a = doc.len() % 2 == 1;
    let json = render(doc, InputFormat::Json, pdf_a);
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(doc) else {
        return;
    };
    let Ok(packed) = rmp_serde::to_vec(&value) else {
        return;
    };
    // Only a document JSON accepts has to round-trip: the `Value` detour
    // drops duplicate keys, which JSON parsing rejects.
    let msgpack = render(&packed, InputFormat::MessagePack, pdf_a);
    if let Ok(a) = &json {
        match &msgpack {
            Ok(b) => assert!(a == b, "MessagePack output differs from JSON"),
            Err(e) => panic!("MessagePack rejected a document JSON renders: {e}"),
        }
    }
}

/// Renders arbitrary bytes registered as an image (SVG or raster), drawn
/// twice at different sizes; same oracle as [`check`].
///
/// # Errors
///
/// Why the engine did not render it.
pub fn check_image(bytes: &[u8]) -> Result<(), InkoxideError> {
    let mut images = ImageStore::new();
    images.insert("fz", bytes.to_vec());
    let doc = br#"{"version":1,"fonts":[],"pages":[{"size":{"width":300,"height":400},"children":[
        {"type":"image","src":"fz"},
        {"type":"image","src":"fz","fit":"cover","style":{"width":40,"height":90}}]}]}"#;
    render_with(
        doc,
        InputFormat::Json,
        &FONTS,
        &images,
        bytes.len() % 2 == 1,
    )
    .map(|_| ())
}

/// Registers arbitrary bytes as a font, as they are and with the WOFF and
/// WOFF2 signatures in place of their first four bytes, so the decoders
/// are reached from any input. Each font that registers sets a line of
/// text; same oracle as [`check`]. Returns how many registered.
pub fn check_font(bytes: &[u8]) -> usize {
    let mut variants = vec![bytes.to_vec()];
    if let Some(rest) = bytes.get(4..) {
        for signature in [b"wOFF", b"wOF2"] {
            variants.push([&signature[..], rest].concat());
        }
    }
    let doc = r#"{"version":1,"fonts":[{"id":"fz","family":"Fz"}],"pages":[{"size":{"width":300,"height":200},"children":[
        {"type":"text","runs":[{"text":"Hello, world! 0123 Äß של fi\u00ad😀","font":"fz","size":12}]}]}]}"#;
    let mut registered = 0;
    for v in variants {
        let mut fonts = FontStore::new();
        if fonts.register("fz", v).is_ok() {
            registered += 1;
            let _ = render_with(doc.as_bytes(), InputFormat::Json, &fonts, &IMAGES, false);
        }
    }
    registered
}

fn render(doc: &[u8], format: InputFormat, pdf_a: bool) -> Result<Vec<u8>, InkoxideError> {
    render_with(doc, format, &FONTS, &IMAGES, pdf_a)
}

fn render_with(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
    images: &ImageStore,
    pdf_a: bool,
) -> Result<Vec<u8>, InkoxideError> {
    let errors = inkoxide::validate(doc, format, fonts, images);
    let options = RenderOptions::default()
        .with_compress(false)
        .with_threads(1)
        .with_pdf_a(pdf_a);
    let result = inkoxide::render(doc, format, &options, fonts, images);
    match &result {
        Ok(pdf) => {
            assert!(
                errors.is_empty(),
                "rendered a document validate rejects: {errors:?}"
            );
            assert!(pdf.starts_with(b"%PDF-") && pdf.trim_ascii_end().ends_with(b"%%EOF"));
            assert_finite_numbers(pdf, true);
        }
        Err(InkoxideError::Validation(e)) => {
            assert!(
                !errors.is_empty(),
                "render rejected a valid document: {e:?}"
            );
        }
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
                    if !skip {
                        assert_finite_numbers(&data[body.min(end)..end], false);
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
    "textIndent",
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
        "backgroundColor" => color(s),
        "opacity" => json!(s.below(5) as f64 / 4.0),
        "borderWidth" => edges(s, nonneg),
        "borderColor" => edges(s, color),
        "borderStyle" => e(s, &["solid", "dashed", "dotted"]),
        "textAlign" => e(s, &["left", "center", "right", "justify"]),
        "maxLines" => json!(s.below(5)),
        "ellipsis" | "hyphenate" => json!(s.chance(128)),
        "lang" => e(s, &["fr", "en", "en-US", "de", "xx", ""]),
        "whiteSpace" if s.chaos => e(s, &["pre-wrap", "pre-line"]),
        "whiteSpace" => e(s, &["normal", "nowrap", "pre"]),
        "verticalAlign" => e(s, &["top", "middle", "bottom"]),
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
    "fi",
    "AV",
    "-",
    "\u{feff}",
    "",
];

fn text(s: &mut Src) -> String {
    let mut t = String::new();
    for _ in 0..s.below(24) {
        t.push_str(s.pick(WORDS));
        if s.chance(200) {
            t.push(' ');
        }
    }
    if s.chance(6) {
        t = "x".repeat(s.below(5000));
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

fn run(s: &mut Src) -> Value {
    let mut m = Map::new();
    if s.chance(20) {
        m.insert(
            "placeholder".into(),
            json!(s.pick(&["pageNumber", "totalPages"])),
        );
    } else {
        m.insert("text".into(), json!(text(s)));
    }
    // A fixture font, or one the document does not declare.
    let font = FIXTURE_FONTS
        .get(s.below(FIXTURE_FONTS.len() + 1))
        .map_or("Missing", |f| f.0);
    m.insert("font".into(), json!(font));
    m.insert(
        "size".into(),
        match s.below(8) {
            0 => json!(1000),
            1 => json!(0.01),
            2 => json!(0),
            _ => json!(4 + s.below(30)),
        },
    );
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
    }
    if s.chance(20) {
        m.insert("link".into(), link(s));
    }
    if s.chance(10) {
        m.insert("hyphenate".into(), json!(s.chance(50)));
    }
    Value::Object(m)
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
        }
        m.insert("style".into(), st);
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
            "pageBreak",
            "spacer",
        ])
    } else {
        s.pick(&["box", "box", "text", "text", "image", "table", "spacer"])
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
    if kind != "pageBreak" && s.chance(20) {
        m.insert("link".into(), link(s));
    }
    if kind != "pageBreak" && (s.flow || s.chaos) && s.chance(20) {
        let level = if s.chaos { s.below(40) } else { 1 + s.below(4) };
        m.insert(
            "bookmark".into(),
            json!({"title": format!("B{}", text(s)), "level": level}),
        );
    }
    if kind != "pageBreak" && s.chance(8) {
        m.insert("debug".into(), json!(true));
    }
    match kind {
        "box" => {
            let n = if s.chance(15) {
                40 + s.below(200)
            } else {
                s.below(5)
            };
            let mut kids: Vec<Value> = (0..n).map(|_| node(s, depth + 1, budget)).collect();
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
            m.insert("children".into(), Value::Array(kids));
        }
        "text" => {
            let runs: Vec<Value> = (0..s.below(4)).map(|_| run(s)).collect();
            m.insert("runs".into(), Value::Array(runs));
        }
        "image" => {
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
                        s.pick(&["logo", "palette", "photo", "chart"])
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
    let fonts: Vec<Value> = FIXTURE_FONTS
        .iter()
        .map(|(id, family, weight, _)| json!({"id": id, "family": family, "weight": weight}))
        .collect();
    let doc = json!({
        "version": 1,
        "meta": meta,
        "fonts": fonts,
        "pages": pages,
    });
    doc.to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_image_says_why_it_did_not_render() {
        let Err(InkoxideError::Validation(e)) = check_image(b"abc") else {
            panic!("rendered an image that is not one");
        };
        assert!(e.message.contains("unsupported image format"), "{e}");
        assert!(check_image(include_bytes!("../../fixtures/images/logo.png")).is_ok());
    }
}
