//! Every number in the PDF is finite and one readers parse: PDF has no
//! syntax for infinity or NaN, and one `inf` operand makes a page invalid;
//! an integer past 32 bits is an error or null to readers. Validation
//! bounds the numbers of a document; the writer turns whatever still
//! overflows into 0 and stops finite numbers at ±1e11.

use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn render(doc: &str) -> Vec<u8> {
    let mut images = ImageStore::new();
    images.insert("photo", fixture("images/photo.jpg"));
    images.insert("chart", fixture("images/chart.svg"));
    render_with(doc, &images)
}

fn render_with(doc: &str, images: &ImageStore) -> Vec<u8> {
    let mut fonts = FontStore::new();
    fonts
        .register("Inter-Regular", fixture("fonts/Inter-Regular.ttf"))
        .unwrap();
    let errors = inkoxide::validate(doc.as_bytes(), InputFormat::Json, &fonts, images);
    assert!(errors.is_empty(), "{errors:#?}");
    let options = RenderOptions::default().with_compress(false);
    inkoxide::render(doc.as_bytes(), InputFormat::Json, &options, &fonts, images).unwrap()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The tokens of the PDF's object syntax and of its uncompressed streams
/// (content streams with compression off). Compressed streams (fonts,
/// images, SVG forms) are skipped, or inflated with `inflate`, for a
/// document whose only compressed streams are SVG forms.
fn tokens(pdf: &[u8], inflate: bool) -> Vec<String> {
    let mut texts: Vec<Vec<u8>> = Vec::new();
    let mut rest = pdf;
    while let Some(i) = find(rest, b"stream\n") {
        let (head, body) = rest.split_at(i);
        let end = find(body, b"endstream").expect("endstream");
        let dict = &head[head.len().saturating_sub(300)..];
        let dict = &dict[find(dict, b" obj").unwrap_or(0)..];
        let data = &body[b"stream\n".len()..end];
        texts.push(head.to_vec());
        if find(dict, b"/Filter").is_none() {
            texts.push(data.to_vec());
        } else if inflate {
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(data), &mut out)
                .expect("a Flate stream");
            texts.push(out);
        }
        rest = &body[end + b"endstream".len()..];
    }
    texts.push(rest.to_vec());
    let delimiter = |b: u8| b.is_ascii_whitespace() || b"[]<>()/{}%".contains(&b);
    texts
        .iter()
        .flat_map(|t| t.split(|&b| delimiter(b)))
        .filter(|t| !t.is_empty())
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .collect()
}

/// Asserts that no token is a non-finite number, and that there are
/// numbers to check at all.
fn assert_finite(pdf: &[u8], min_numbers: usize) {
    assert_finite_tokens(&tokens(pdf, false), min_numbers);
}

fn assert_finite_tokens(tokens: &[String], min_numbers: usize) {
    let bad: Vec<_> = tokens
        .iter()
        .filter(|t| {
            let t = t.trim_start_matches(['-', '+']).to_ascii_lowercase();
            t == "inf" || t == "infinity" || t == "nan"
        })
        .collect();
    assert!(bad.is_empty(), "non-finite numbers in the PDF: {bad:?}");
    let numbers = tokens
        .iter()
        .filter(|t| t.parse::<f64>().is_ok_and(f64::is_finite))
        .count();
    assert!(numbers >= min_numbers, "only {numbers} numbers");
}

fn doc(page: &str, children: &[String]) -> String {
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
           "pages":[{{"size":{page},"children":[{}]}}]}}"#,
        children.join(",")
    )
}

fn text(style: &str, run: &str) -> String {
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"Finite numbers only","font":"Inter-Regular","size":12{run}}}]}}"#
    )
}

fn boxed(style: &str) -> String {
    let border = if style.contains("borderWidth") {
        ""
    } else {
        r#""borderWidth":{"top":2},"#
    };
    format!(
        r#"{{"type":"box","link":"https://example.com","style":{{"backgroundColor":[1,0,0,1],{border}{style}}},
            "children":[{{"type":"box","style":{{"height":4,"backgroundColor":[0,0,1,1]}}}},{}]}}"#,
        text("", "")
    )
}

#[test]
fn every_property_at_its_bounds_renders_finite_numbers() {
    let children = vec![
        // Percentages at ±10000 %.
        boxed(r#""width":"10000%","height":20"#),
        boxed(r#""minWidth":"10000%","maxHeight":"10000%","flexBasis":"10000%""#),
        boxed(r#""margin":{"left":"-10000%","right":"10000%","top":"-10000%"},"padding":{"left":"10000%"}"#),
        boxed(r#""position":"absolute","inset":{"left":"10000%","top":"-10000%"},"width":"10000%""#),
        boxed(r#""position":"absolute","inset":{"right":"-10000%","bottom":"10000%"}"#),
        boxed(r#""flexDirection":"row","flexWrap":"wrap","gap":"10000%","columnGap":"10000%""#),
        boxed(r#""transform":[{"translate":["10000%","-10000%"]},{"rotate":1000000}],"transformOrigin":["-10000%","10000%"]"#),
        boxed(r#""display":"grid","gridTemplateColumns":["10000%",{"min":"10000%","max":"1000000fr"},{"fitContent":"10000%"}],"gridAutoRows":["10000%"]"#),
        // Points at ±1000000, where they do not add pages.
        boxed(r#""width":1000000,"maxWidth":1000000,"minHeight":10"#),
        boxed(r#""margin":{"left":-1000000,"bottom":-1000000},"padding":{"right":1000000}"#),
        boxed(r#""position":"absolute","inset":{"left":1000000,"top":-1000000}"#),
        boxed(r#""position":"absolute","inset":{"right":-1000000,"bottom":1000000},"width":1000000"#),
        boxed(r#""transform":[{"translate":[1000000,-1000000]}],"transformOrigin":[1000000,-1000000]"#),
        boxed(r#""transform":[{"scale":[1000000,-1000000]},{"skew":[89.9,-89.9]}],"rotate":-1000000"#),
        boxed(r#""transform":[{"matrix":[1000000,-1000000,1000000,-1000000,1000000,-1000000]}]"#),
        boxed(r#""borderWidth":{"left":1000000,"top":1000000},"borderRadius":1000000,"width":40,"height":40"#),
        boxed(r#""borderRadius":{"topLeft":1000000,"bottomRight":1000000},"borderStyle":"dashed","borderWidth":{"top":3},"height":20"#),
        boxed(r#""flexDirection":"row","width":100"#)
            .replace(r#""height":4"#, r#""flexGrow":1000000,"flexShrink":1000000,"flexBasis":1000000,"height":4"#),
        boxed(r#""aspectRatio":0.000001,"width":0.5"#),
        boxed(r#""aspectRatio":1000000,"height":0.5"#),
        boxed(r#""display":"grid","gridTemplateColumns":[1000000,"1000000fr",{"min":0,"max":1000000}],"gridAutoColumns":[{"fitContent":1000000}]"#),
        // Text at its own bounds.
        text(r#""textIndent":-10000,"lineHeight":100"#, r#","letterSpacing":1000"#),
        text(r#""textIndent":10000"#, r#","letterSpacing":-1000,"decoration":"underline""#),
        text("", r#","size":1000,"verticalAlign":"super","fontFeatures":{"salt":65535}"#)
            .replace(r#""size":12,"#, ""),
        text(r#""lineHeight":100"#, r#","verticalAlign":"sub","decoration":"underline""#),
        // Images placed at the objectPosition bounds.
        r#"{"type":"image","src":"photo","fit":"none","objectPosition":{"x":"10000%","y":-14400},"style":{"width":"10000%","height":40}}"#.into(),
        r#"{"type":"image","src":"chart","fit":"cover","objectPosition":{"x":14400,"y":"-10000%"},"style":{"transform":[{"scale":[1000000,1000000]}],"height":40}}"#.into(),
        // Form fields and tables.
        r#"{"type":"field","style":{"width":"10000%","height":20,"transform":[{"translate":[-1000000,0]}]},"field":{"name":"t","kind":"text","font":"Inter-Regular","value":"x"}}"#.into(),
        r#"{"type":"field","style":{"width":1000000,"height":20},"field":{"name":"c","kind":"checkbox","checked":true}}"#.into(),
        r#"{"type":"table","style":{"borderCollapse":"separate","borderSpacing":1000000},"columns":[{"width":1000000},{"width":"1000000fr"},{"width":"auto"}],
            "rows":[{"minHeight":20,"cells":[{"style":{"padding":{"left":"10000%"},"backgroundColor":[0,1,0,1]},"children":[]},{"children":[]},{"children":[]}]}]}"#.into(),
        r#"{"type":"box","break":{"minPresenceAhead":1000000},"style":{"height":10}}"#.into(),
    ];
    let pdf = render(&doc(r#"{"width":14400,"height":14400}"#, &children));
    assert_finite(&pdf, 500);
}

#[test]
fn percentages_compounding_past_f32_render_finite_numbers() {
    // Each level is within the bounds, but 10000 % of 10000 % of … forty
    // levels deep overflows f32 in layout: the writer keeps the PDF valid.
    let mut node =
        r##"{"type":"box","link":"#top","style":{"height":5,"backgroundColor":[0,1,0,1]},
        "children":[{"type":"text","runs":[{"text":"deep","font":"Inter-Regular","size":10}]}]}"##
            .to_string();
    for _ in 0..40 {
        node = format!(
            r#"{{"type":"box","style":{{"width":"10000%","padding":{{"left":"1%"}},"backgroundColor":[1,0,0,1],"borderWidth":{{"left":1}}}},"children":[{node}]}}"#
        );
    }
    let children = [
        r#"{"type":"box","id":"top","style":{"height":5}}"#.to_string(),
        node,
    ];
    let pdf = render(&doc(r#"{"width":200,"height":200}"#, &children));
    assert_finite(&pdf, 100);
}

#[test]
fn svg_numbers_that_overflow_render_finite_numbers() {
    // SVG images bring numbers of their own, which the document's bounds
    // do not cover: the smallest size and viewBox scaled up, and gradient
    // coordinates and dashes past f32 (usvg reads them as f64).
    const NS: &str = "xmlns='http://www.w3.org/2000/svg'";
    let svgs = [
        format!("<svg {NS} width='1e-5' height='1e-5'><rect width='1' height='1'/></svg>"),
        format!("<svg {NS} width='100000' height='100000' viewBox='0 0 1e-5 1e-5'><rect width='1e-6' height='1e-6'/></svg>"),
        format!("<svg {NS} width='100' height='100'><defs><linearGradient id='g' x1='-1e39' x2='1e39' gradientUnits='userSpaceOnUse'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue' stop-opacity='0.5'/></linearGradient></defs><rect width='100' height='100' fill='url(#g)'/></svg>"),
        format!("<svg {NS} width='100' height='100'><path d='M0 0 L 50 50' stroke='red' stroke-width='2' stroke-dasharray='1e39 1' stroke-dashoffset='1e39'/></svg>"),
    ];
    let mut images = ImageStore::new();
    let mut children = Vec::new();
    for (i, svg) in svgs.iter().enumerate() {
        images.insert(format!("svg{i}"), svg.as_bytes().to_vec());
        children.push(format!(
            r#"{{"type":"image","src":"svg{i}","style":{{"width":50,"height":50}}}}"#
        ));
    }
    let pdf = render_with(&doc(r#"{"width":200,"height":200}"#, &children), &images);
    assert_finite_tokens(&tokens(&pdf, true), 50);
}

#[test]
fn svgs_too_small_to_scale_are_invalid_images() {
    // The writer scales an SVG to the unit square by the inverse of its
    // size, and its viewBox up to that size. Below 1e-5 px the scale is a
    // number PDF readers cannot parse (qpdf treats the image as null), so
    // such an SVG is refused where the document names it.
    const NS: &str = "xmlns='http://www.w3.org/2000/svg'";
    let json = doc(
        r#"{"width":200,"height":200}"#,
        &[r#"{"type":"image","src":"svg","style":{"width":100,"height":100}}"#.into()],
    );
    let mut fonts = FontStore::new();
    fonts
        .register("Inter-Regular", fixture("fonts/Inter-Regular.ttf"))
        .unwrap();
    for side in ["1e-12", "1e-20", "1e-30", "1e-39"] {
        for svg in [
            format!("<svg {NS} width='{side}' height='{side}'/>"),
            format!("<svg {NS} width='100' height='100' viewBox='0 0 {side} {side}'><rect width='1' height='1'/></svg>"),
        ] {
            let mut images = ImageStore::new();
            images.insert("svg", svg.as_bytes().to_vec());
            let errors = inkoxide::validate(json.as_bytes(), InputFormat::Json, &fonts, &images);
            let got: Vec<_> = errors
                .iter()
                .map(|e| (e.code.as_str(), e.path.as_str()))
                .collect();
            assert_eq!(got, [("INVALID_IMAGE", "pages[0].children[0].src")], "{svg}");
            let options = RenderOptions::default();
            assert!(
                inkoxide::render(json.as_bytes(), InputFormat::Json, &options, &fonts, &images)
                    .is_err(),
                "{svg}"
            );
        }
    }

    // The smallest SVG and viewBox accepted, scaled up as far as the
    // limits allow, write numbers every reader parses: integers within 32
    // bits (pdf-writer writes digits only past 1e12, reals below it).
    let json = doc(
        r#"{"width":200,"height":200}"#,
        &[
            r#"{"type":"image","src":"small","style":{"width":100,"height":100}}"#.into(),
            r#"{"type":"image","src":"zoomed","style":{"width":100,"height":100}}"#.into(),
        ],
    );
    let mut images = ImageStore::new();
    images.insert(
        "small",
        format!("<svg {NS} width='1e-5' height='1e-5'><rect width='1' height='1'/></svg>")
            .into_bytes(),
    );
    images.insert(
        "zoomed",
        format!("<svg {NS} width='100000' height='100000' viewBox='0 0 1e-5 1e-5'><rect width='1e-6' height='1e-6'/></svg>").into_bytes(),
    );
    let pdf = render_with(&json, &images);
    let tokens = tokens(&pdf, true);
    assert_finite_tokens(&tokens, 50);
    assert_parsable_tokens(&tokens);
    assert_qpdf_check("smallest-svgs", &pdf);
}

/// Asserts that every number token is one readers parse: an integer within
/// 32 bits, or a real within the writer's bound (1e11).
fn assert_parsable_tokens(tokens: &[String]) {
    let bad: Vec<_> = tokens
        .iter()
        .filter(|t| match t.parse::<f64>() {
            Ok(v) if !t.contains('.') => t.parse::<i32>().is_err() || v.abs() > 1e11,
            Ok(v) => v.abs() > 1e11,
            Err(_) => false,
        })
        .collect();
    assert!(bad.is_empty(), "numbers readers do not parse: {bad:?}");
}

/// Asserts that `qpdf --check` passes `pdf` without warnings (exit status
/// 0), when qpdf is installed.
fn assert_qpdf_check(name: &str, pdf: &[u8]) {
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.pdf"));
    std::fs::write(&out, pdf).unwrap();
    if let Ok(o) = std::process::Command::new("qpdf")
        .arg("--check")
        .arg(&out)
        .output()
    {
        assert!(
            o.status.success(),
            "{name}: qpdf --check failed:\n{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
    }
}

#[test]
fn huge_svg_numbers_are_written_within_what_readers_parse() {
    // Finite SVG numbers past what PDF readers parse: pdf-writer writes
    // them as digits only, integers far past 32 bits. The writer stops
    // them at ±1e11, as reals.
    const NS: &str = "xmlns='http://www.w3.org/2000/svg'";
    let svg = |body: &str| format!("<svg {NS} width='100' height='100'>{body}</svg>");
    let svgs = [
        svg("<g transform='scale(1e30)'><rect width='1' height='1'/></g>"),
        svg("<g transform='translate(1e30 -1e30)'><rect width='10' height='10'/></g>"),
        svg("<g transform='scale(-2147483648)'><rect width='1' height='1'/></g>"),
        svg("<defs><linearGradient id='g' x1='-1e30' y1='0' x2='1e30' y2='1e30' gradientUnits='userSpaceOnUse'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue' stop-opacity='0.5'/></linearGradient></defs><rect width='100' height='100' fill='url(#g)'/>"),
        svg("<defs><radialGradient id='g' cx='1e30' cy='1e30' fx='-1e30' gradientUnits='userSpaceOnUse' spreadMethod='repeat'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue'/></radialGradient></defs><rect width='100' height='100' fill='url(#g)'/>"),
        svg("<line x1='0' y1='0' x2='1e30' y2='-1e30' stroke='red'/><rect x='-1e30' y='1e30' width='1e30' height='1e30'/>"),
        svg("<defs><clipPath id='c' transform='scale(1e30)'><rect width='1' height='1'/></clipPath></defs><rect width='100' height='100' clip-path='url(#c)'/>"),
        svg("<defs><mask id='m' x='-1e30' y='-1e30' width='1e30' height='1e30' maskUnits='userSpaceOnUse'><rect width='1e30' height='100' fill='white'/></mask></defs><rect width='100' height='100' mask='url(#m)'/>"),
        svg("<defs><pattern id='p' width='1e30' height='1e30' patternUnits='userSpaceOnUse'><rect width='1e30' height='1'/></pattern></defs><rect width='100' height='100' fill='url(#p)'/>"),
    ];
    let mut images = ImageStore::new();
    let mut children = Vec::new();
    for (i, svg) in svgs.iter().enumerate() {
        images.insert(format!("svg{i}"), svg.as_bytes().to_vec());
        children.push(format!(
            r#"{{"type":"image","src":"svg{i}","style":{{"width":50,"height":50}}}}"#
        ));
    }
    let pdf = render_with(&doc(r#"{"width":200,"height":200}"#, &children), &images);
    let tokens = tokens(&pdf, true);
    assert_finite_tokens(&tokens, 100);
    assert_parsable_tokens(&tokens);
    assert!(
        tokens.iter().any(|t| t == "100000000000.0"),
        "nothing reached the bound"
    );
    assert_qpdf_check("huge-svg-numbers", &pdf);
}
