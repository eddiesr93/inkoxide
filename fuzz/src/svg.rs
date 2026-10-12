//! Structure-aware SVG generator for `render_svg_tree`: the bytes pick
//! elements, attributes and references from what `usvg` expands (`use`,
//! markers, patterns, gradients, clip paths, masks, filters, style sheets,
//! entities and nested SVG images), so most inputs exercise the complexity
//! limits instead of failing to parse.

use std::fmt::Write as _;

use super::Src;

/// Ids every reference picks from; repeated on purpose, so that elements
/// share ids and references find several targets.
const IDS: &[&str] = &["a", "b", "c", "d", "e"];

const CONTAINERS: &[&str] = &[
    "g", "defs", "symbol", "marker", "pattern", "clipPath", "mask", "svg", "switch", "a",
];
const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon",
];

/// An SVG document from `data`.
pub fn svg(data: &[u8]) -> Vec<u8> {
    let s = &mut Src::new(data);
    let mut out = String::new();
    let entities = s.chance(40);
    if entities {
        doctype(s, &mut out);
    }
    let mut budget = 300;
    document(s, &mut out, 0, entities, &mut budget);
    out.into_bytes()
}

fn document(s: &mut Src, out: &mut String, level: usize, entities: bool, budget: &mut usize) {
    let (w, h) = (1 + s.below(200), 1 + s.below(200));
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}">"#
    );
    if s.chance(60) {
        style_sheet(s, out);
    }
    while *budget > 0 && !s.exhausted() && s.chance(220) {
        element(s, out, 0, level, entities, budget);
    }
    out.push_str("</svg>");
}

fn element(
    s: &mut Src,
    out: &mut String,
    depth: usize,
    level: usize,
    entities: bool,
    budget: &mut usize,
) {
    *budget = budget.saturating_sub(1);
    let kind = s.below(10);
    if depth > 6 || *budget == 0 || kind < 4 {
        let tag = match kind {
            0 => "use",
            1 => "image",
            _ => s.pick(SHAPES),
        };
        leaf(s, out, tag, level, entities, budget);
        return;
    }
    let tag = match kind {
        4 => "linearGradient",
        5 => "radialGradient",
        6 => "filter",
        _ => s.pick(CONTAINERS),
    };
    let _ = write!(out, "<{tag}");
    attributes(s, out, tag, entities);
    out.push('>');
    match tag {
        "linearGradient" | "radialGradient" => {
            for _ in 0..s.below(4) {
                let _ = write!(
                    out,
                    r#"<stop offset="{}" stop-color="{}"/>"#,
                    s.below(101) as f32 / 100.0,
                    color(s)
                );
            }
        }
        "filter" => {
            for _ in 0..=s.below(3) {
                match s.below(4) {
                    0 => {
                        let _ = write!(out, r##"<feImage href="#{}"/>"##, s.pick(IDS));
                    }
                    1 => out.push_str(r"<feTile/>"),
                    2 => {
                        let _ = write!(out, r#"<feGaussianBlur stdDeviation="{}"/>"#, number(s));
                    }
                    _ => out.push_str(r#"<feOffset dx="3" dy="4"/>"#),
                }
            }
        }
        _ => {
            for _ in 0..s.below(5) {
                element(s, out, depth + 1, level, entities, budget);
            }
        }
    }
    let _ = write!(out, "</{tag}>");
}

fn leaf(
    s: &mut Src,
    out: &mut String,
    tag: &str,
    level: usize,
    entities: bool,
    budget: &mut usize,
) {
    let _ = write!(out, "<{tag}");
    attributes(s, out, tag, entities);
    if tag == "image" {
        out.push_str(r#" width="10" height="10""#);
        let src = image_src(s, level, budget);
        let attr = if s.chance(128) { "href" } else { "xlink:href" };
        let _ = write!(out, r#" {attr}="{src}""#);
    }
    out.push_str("/>");
}

/// A `data:` URL: a nested SVG (up to five levels), a PNG header that
/// claims a size, or a tiny real PNG.
fn image_src(s: &mut Src, level: usize, budget: &mut usize) -> String {
    match s.below(4) {
        0 | 1 if level < 5 => {
            let mut inner = String::new();
            let mut inner_budget = (*budget).min(60);
            document(s, &mut inner, level + 1, false, &mut inner_budget);
            *budget = budget.saturating_sub(60 - inner_budget.min(60));
            if s.chance(128) {
                format!("data:image/svg+xml;base64,{}", base64(inner.as_bytes()))
            } else {
                format!("data:image/svg+xml,{}", percent(inner.as_bytes()))
            }
        }
        2 => {
            let side = s.pick(&[1u32, 100, 4000, 7000, 20_000]);
            let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
            png.extend(side.to_be_bytes());
            png.extend(side.to_be_bytes());
            png.extend([8, 2, 0, 0, 0, s.byte(), 0, 0, 0]);
            format!("data:image/png;base64,{}", base64(&png))
        }
        _ => format!(
            "data:image/png;base64,{}",
            base64(include_bytes!("../../fixtures/images/logo.png"))
        ),
    }
}

fn attributes(s: &mut Src, out: &mut String, tag: &str, entities: bool) {
    if s.chance(150) {
        let _ = write!(out, r#" id="{}""#, s.pick(IDS));
    }
    if s.chance(60) {
        let _ = write!(out, r#" class="{}""#, s.pick(&["c", "k", "c k"]));
    }
    if tag == "use" || (tag != "image" && s.chance(30)) {
        link(s, out);
    }
    match tag {
        "path" => {
            let _ = write!(out, r#" d="{}""#, path_data(s));
        }
        "polyline" | "polygon" => {
            let mut points = String::new();
            for _ in 0..=(s.below(64) * if s.chance(20) { 64 } else { 1 }) {
                let _ = write!(points, "{},{} ", number(s), number(s));
            }
            let _ = write!(out, r#" points="{points}""#);
        }
        "circle" => {
            let _ = write!(out, r#" r="{}""#, radius(s));
        }
        "ellipse" => {
            let _ = write!(out, r#" rx="{}" ry="{}""#, radius(s), radius(s));
        }
        "rect" => {
            let _ = write!(out, r#" width="{}" height="{}""#, number(s), number(s));
            if s.chance(80) {
                let _ = write!(out, r#" rx="{}""#, radius(s));
            }
        }
        "line" => {
            let _ = write!(out, r#" x2="{}" y2="{}""#, number(s), number(s));
        }
        "pattern" => {
            out.push_str(r#" width="5" height="5" patternUnits="userSpaceOnUse""#);
        }
        "marker" => {
            out.push_str(r#" markerWidth="3" markerHeight="3""#);
        }
        _ => {}
    }
    // Each attribute at most once, as XML requires.
    let mut seen: Vec<&str> = Vec::new();
    for _ in 0..s.below(4) {
        let (name, value) = match s.below(9) {
            0 => ("fill", paint(s)),
            1 => ("stroke", paint(s)),
            2 => (
                s.pick(&["marker-start", "marker-mid", "marker-end"]),
                url(s),
            ),
            3 => ("clip-path", url(s)),
            4 => ("mask", url(s)),
            5 => ("filter", url(s)),
            6 => ("stroke-dasharray", dashes(s)),
            7 => (
                "transform",
                format!("scale({}) rotate({})", number(s), s.below(360)),
            ),
            _ => ("style", declarations(s)),
        };
        if !seen.contains(&name) {
            seen.push(name);
            let _ = write!(out, r#" {name}="{value}""#);
        }
    }
    if entities && s.chance(60) {
        let _ = write!(out, r#" data-x="&e{};""#, s.below(3));
    }
}

/// An `href` to one of the ids, in the spellings `usvg` accepts.
fn link(s: &mut Src, out: &mut String) {
    let id = s.pick(IDS);
    let value = match s.below(4) {
        0 => format!(" #{id} "),
        1 => format!("#{id} "),
        _ => format!("#{id}"),
    };
    match s.below(3) {
        0 => {
            let _ = write!(out, r#" xlink:href="{value}""#);
        }
        1 => {
            let _ = write!(out, r##" xlink:href="#{}" href="{value}""##, s.pick(IDS));
        }
        _ => {
            let _ = write!(out, r#" href="{value}""#);
        }
    }
}

fn url(s: &mut Src) -> String {
    let id = s.pick(IDS);
    match s.below(4) {
        0 => format!("url( '#{id} ' )"),
        1 => format!("url(&quot;#{id}&quot;)"),
        _ => format!("url(#{id})"),
    }
}

fn paint(s: &mut Src) -> String {
    if s.chance(140) {
        url(s)
    } else {
        color(s).into()
    }
}

fn color(s: &mut Src) -> &'static str {
    s.pick(&["red", "#00f", "none", "currentColor", "rgba(0,0,0,0.5)"])
}

fn number(s: &mut Src) -> String {
    // Numbers past the limits only in the one document in eight that may
    // break the rules, so the others get past the checks.
    match s.below(40) {
        0 if s.chaos => s
            .pick(&["1e7", "9999999", "1e8", "1e30", "-1e7", "3e38", "1e400"])
            .into(),
        0 => s.pick(&["1e6", "9999999", "-1e6", "1e-30"]).into(),
        1 => "0".into(),
        _ => format!("{}", s.below(300) as f32 / 4.0),
    }
}

fn radius(s: &mut Src) -> String {
    if s.chance(12) && s.chaos {
        s.pick(&[
            "1e6", "1e7", "1.5e7", "1e9", "1e30", "3e38", "1e300", "50%", "1e7in",
        ])
        .into()
    } else if s.chance(12) {
        s.pick(&["1e6", "9999999", "50%", "1e5in", "1e-30"]).into()
    } else {
        number(s)
    }
}

fn path_data(s: &mut Src) -> String {
    let mut d = String::from("M0 0");
    let segments = 1 + s.below(40) * if s.chance(20) { 200 } else { 1 };
    for _ in 0..segments {
        let _ = match s.below(8) {
            0 => write!(d, "L{} {}", number(s), number(s)),
            1 => write!(d, "h{}", number(s)),
            2 => write!(d, "v{}", number(s)),
            3 => write!(
                d,
                "C{} {} {} {} {} {}",
                number(s),
                number(s),
                number(s),
                number(s),
                number(s),
                number(s)
            ),
            4 | 5 => write!(
                d,
                "{}{} {} {} {} {} {} {}",
                s.pick(&["A", "a"]),
                radius(s),
                radius(s),
                s.below(360),
                s.below(2),
                s.below(2),
                number(s),
                number(s)
            ),
            6 => write!(
                d,
                "Q{} {} {} {}",
                number(s),
                number(s),
                number(s),
                number(s)
            ),
            _ => write!(d, "z"),
        };
    }
    d
}

fn dashes(s: &mut Src) -> String {
    let n = 1 + s.below(8) * if s.chance(20) { 4000 } else { 1 };
    vec!["1 2"; n].join(" ")
}

fn declarations(s: &mut Src) -> String {
    let mut out = String::new();
    for _ in 0..=(s.below(4) * if s.chance(10) { 500 } else { 1 }) {
        let _ = match s.below(7) {
            0 => write!(out, "fill:{};", paint(s)),
            1 => write!(out, "marker:{};", url(s)),
            2 => write!(out, "MARKER-MID:{};", url(s)),
            3 => write!(out, "/**/marker-end:{};", url(s)),
            4 => write!(out, "stroke-dasharray:{};", dashes(s)),
            5 => write!(out, "r:{};", radius(s)),
            _ => write!(out, "stroke:{};", paint(s)),
        };
    }
    out
}

fn style_sheet(s: &mut Src, out: &mut String) {
    out.push_str("<style>");
    for _ in 0..=(s.below(6) * if s.chance(20) { 500 } else { 1 }) {
        let mut selectors = Vec::new();
        for _ in 0..=s.below(3) {
            let parts: Vec<&str> = (0..=(s.below(4) * if s.chance(20) { 8 } else { 1 }))
                .map(|_| s.pick(&["*", "g", "path", ".c", "#a", "rect", "svg"]))
                .collect();
            let joiner = s.pick(&[" ", " > ", " ~ ", " + "]);
            selectors.push(parts.join(joiner));
        }
        let _ = write!(out, "{}{{{}}}", selectors.join(","), declarations(s));
    }
    out.push_str("</style>");
}

/// A DOCTYPE with up to three entities, sometimes long or referring to
/// each other.
fn doctype(s: &mut Src, out: &mut String) {
    out.push_str("<!DOCTYPE svg [");
    for i in 0..3 {
        let value = match s.below(8) {
            0 | 3 => "x".repeat(s.below(4) * 1000),
            1 if s.chance(64) => format!("&e{};&e{};", (i + 1) % 3, (i + 2) % 3),
            2 | 4 => "http://www.w3.org/2000/svg".into(),
            _ => "ab".into(),
        };
        let _ = write!(out, r#"<!ENTITY e{i} "{value}">"#);
    }
    out.push_str("]>");
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(A[(n >> (18 - 6 * k)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn percent(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if b.is_ascii_alphanumeric() {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
