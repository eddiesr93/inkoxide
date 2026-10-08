//! Shared harness for the fuzz targets: fixture fonts and images, the
//! render oracle, and a structure-aware document generator.

use std::sync::LazyLock;

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat, RenderOptions};
use serde_json::{json, Map, Value};

pub static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let mut f = FontStore::new();
    for (id, bytes) in [
        (
            "Inter-Regular",
            &include_bytes!("../../fixtures/fonts/Inter-Regular.ttf")[..],
        ),
        (
            "Inter-Bold",
            &include_bytes!("../../fixtures/fonts/Inter-Bold.ttf")[..],
        ),
        (
            "NotoSans-Regular",
            &include_bytes!("../../fixtures/fonts/NotoSans-Regular.ttf")[..],
        ),
        (
            "RobotoMono-Regular",
            &include_bytes!("../../fixtures/fonts/RobotoMono-Regular.ttf")[..],
        ),
    ] {
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
    s.insert("bad", b"\x89PNG\r\n\x1a\nnot really".to_vec());
    s
});

/// Validates and renders `doc`; panics (a finding) when the engine panics
/// or when `validate` and `render` disagree. A JSON document that renders
/// must render to the same bytes as MessagePack.
pub fn check(doc: &[u8]) {
    let json = render(doc, InputFormat::Json);
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(doc) else {
        return;
    };
    let Ok(packed) = rmp_serde::to_vec(&value) else {
        return;
    };
    // Only a document JSON accepts has to round-trip: the `Value` detour
    // drops duplicate keys, which JSON parsing rejects.
    let msgpack = render(&packed, InputFormat::MessagePack);
    if let Ok(a) = &json {
        match &msgpack {
            Ok(b) => assert!(a == b, "MessagePack output differs from JSON"),
            Err(e) => panic!("MessagePack rejected a document JSON renders: {e}"),
        }
    }
}

fn render(doc: &[u8], format: InputFormat) -> Result<Vec<u8>, InkoxideError> {
    let errors = inkoxide::validate(doc, format, &FONTS, &IMAGES);
    let options = RenderOptions {
        compress: false,
        threads: Some(1),
        ..Default::default()
    };
    let result = inkoxide::render(doc, format, &options, &FONTS, &IMAGES);
    match &result {
        Ok(pdf) => {
            assert!(
                errors.is_empty(),
                "rendered a document validate rejects: {errors:?}"
            );
            assert!(pdf.starts_with(b"%PDF-") && pdf.trim_ascii_end().ends_with(b"%%EOF"));
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

/// Entropy source over the fuzzer's bytes; reads zeros once exhausted, so
/// generation always terminates.
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
        2 => json!(1e9),
        3 => json!(1e-6),
        4 => json!(f32::MAX as f64),
        5 => json!(14_400),
        _ => json!(s.below(600) as f64 + s.below(4) as f64 * 0.25),
    }
}

fn nonneg(s: &mut Src) -> Value {
    match s.below(6) {
        0 => json!(0),
        1 => json!(1e7),
        _ => json!(s.below(50)),
    }
}

fn length(s: &mut Src) -> Value {
    match s.below(5) {
        0 => json!("auto"),
        1 => json!(format!("{}%", s.below(250))),
        _ => num(s),
    }
}

fn length_pct(s: &mut Src) -> Value {
    if s.chance(60) {
        json!(format!("{}%", s.below(120)))
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
    match s.below(6) {
        0 => json!(format!("{}fr", s.below(4))),
        1 => json!("auto"),
        2 => json!("min-content"),
        3 => json!("max-content"),
        4 => json!(format!("{}%", s.below(100))),
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
        let tracks: Vec<Value> = (0..1 + s.below(2)).map(|_| fixed(s)).collect();
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
                let tracks: Vec<Value> = (0..1 + s.below(3)).map(|_| track(s)).collect();
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
];

const PROPS: &[&str] = &[
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
    "textAlign",
    "lineHeight",
    "maxLines",
    "ellipsis",
    "hyphenate",
    "lang",
    "whiteSpace",
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
        // Placed by `node` under a parent with areas; random names in chaos.
        "gridArea" if s.chaos => e(s, &["a", "b", "zz", "9"]),
        "gridArea" => Value::Null,
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
        "lang" => e(s, &["ro", "en", "en-US", "de", "xx", ""]),
        "whiteSpace" if s.chaos => e(s, &["pre-wrap", "pre-line"]),
        "whiteSpace" => e(s, &["normal", "nowrap", "pre"]),
        "verticalAlign" => e(s, &["top", "middle", "bottom"]),
        _ => Value::Null,
    }
}

fn style(s: &mut Src, text: bool) -> Value {
    let mut m = Map::new();
    for _ in 0..s.below(7) {
        let mut p = s.pick(PROPS);
        if !text && !s.chaos && TEXT_ONLY.contains(&p) {
            continue;
        }
        if text && s.chance(128) {
            p = s.pick(&[
                "textAlign",
                "lineHeight",
                "maxLines",
                "ellipsis",
                "hyphenate",
                "lang",
                "whiteSpace",
            ]);
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
    "șțăîâ",
    "internaționalizare",
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
    let font = s.pick(&[
        "Inter-Regular",
        "Inter-Bold",
        "NotoSans-Regular",
        "RobotoMono-Regular",
        "Missing",
    ]);
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
        m.insert("style".into(), style(s, false));
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
                    json!(s.pick(&["", "iVBORw0KGgo=", "data:image/png;base64,AAAA", "!!"])),
                );
            } else {
                m.insert(
                    "src".into(),
                    json!(if s.chaos {
                        s.pick(&["bad", "missing"])
                    } else {
                        s.pick(&["logo", "palette", "photo"])
                    }),
                );
            }
            if s.chance(128) {
                m.insert(
                    "fit".into(),
                    json!(s.pick(&["fill", "contain", "cover", "none"])),
                );
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
    for _ in 0..1 + s.below(3) {
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
        meta.insert("lang".into(), json!(s.pick(&["ro", "en", "de", "zz-ZZ"])));
    }
    if s.chance(30) {
        meta.insert(
            "creationDate".into(),
            json!(s.pick(&["2026-01-01T00:00:00Z", "2026-13-40T99:00:00Z", "bad"])),
        );
    }
    let doc = json!({
        "version": 1,
        "meta": meta,
        "fonts": [
            {"id": "Inter-Regular", "family": "Inter"},
            {"id": "Inter-Bold", "family": "Inter", "weight": 700},
            {"id": "NotoSans-Regular", "family": "Noto Sans"},
            {"id": "RobotoMono-Regular", "family": "Roboto Mono"}
        ],
        "pages": pages,
    });
    serde_json::to_vec(&doc).unwrap()
}
