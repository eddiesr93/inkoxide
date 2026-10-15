use super::*;

fn texts(body: &str) -> Vec<SvgText> {
    let src =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">{body}</svg>"#);
    let doc = Document::parse(&src).unwrap();
    extract(&doc, usize::MAX).unwrap().0.texts
}

fn one(body: &str) -> SvgText {
    let mut t = texts(body);
    assert_eq!(t.len(), 1, "{body}");
    t.remove(0)
}

fn string(t: &SvgText) -> String {
    t.chars.iter().map(|c| c.c).collect()
}

fn error(body: &str) -> String {
    let src = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
    let doc = Document::parse(&src).unwrap();
    match extract(&doc, usize::MAX) {
        Err(TextError::Unsupported(m) | TextError::Limit(m)) => m,
        Ok(_) => panic!("accepted {body}"),
    }
}

#[test]
fn processes_white_space_as_usvg() {
    for (body, want) in [
        ("<text>  Hello \n\t world  </text>", "Hello world"),
        (
            "<text>Text <tspan> text </tspan> text</text>",
            "Text text text",
        ),
        ("<text><tspan>a </tspan> b</text>", "a b"),
        ("<text> </text>", ""),
        (r#"<text xml:space="preserve">  a  b </text>"#, "  a  b "),
        (
            r#"<g xml:space="preserve"><text> a<tspan xml:space="default">  b  </tspan></text></g>"#,
            " ab",
        ),
    ] {
        assert_eq!(string(&one(body)), want, "{body}");
    }
}

#[test]
fn spans_hold_their_own_properties() {
    let t = one(
        r#"<text font-family="Inter, 'Noto Sans', serif" font-size="20" font-weight="bold">
             A<tspan font-style="italic" font-size="50%" font-weight="lighter">b</tspan>
             <a font-size="2em">c</a></text>"#,
    );
    assert_eq!(string(&t), "Ab c");
    assert_eq!(t.spans.len(), 3);
    let [a, b, c] = [&t.spans[0], &t.spans[1], &t.spans[2]];
    assert_eq!(*a.families, ["Inter", "Noto Sans", "serif"]);
    assert_eq!((a.size, a.weight, a.italic), (20.0, 700, false));
    assert_eq!((b.size, b.weight, b.italic), (10.0, 600, true));
    assert_eq!(c.size, 40.0);
    let span_of = |i: usize| t.chars[i].span;
    assert_eq!([span_of(0), span_of(1), span_of(3)], [0, 1, 2]);
}

#[test]
fn resolves_the_cascade_like_usvg() {
    let t = one(r#"<style>
             text { font-size: 30px; fill: green }
             .big { font-size: 40px !important }
             g text tspan { font-weight: 900 }
             #t { letter-spacing: 2px }
             tspan:first-child { word-spacing: 3px }
           </style>
           <g font-family="Brand">
             <text id="t" class="big" font-size="10" style="font-size: 50px; fill: red">
               a<tspan>b</tspan><tspan style="font: italic 300 12px Other, serif">c</tspan>
             </text>
           </g>"#);
    let [a, b, c] = [&t.spans[0], &t.spans[1], &t.spans[2]];
    // `!important` in a sheet wins over the `style` attribute.
    assert_eq!(a.size, 40.0);
    assert_eq!(*a.families, ["Brand"]);
    assert_eq!(a.letter_spacing, 2.0);
    assert_eq!(
        (b.weight, b.word_spacing, b.letter_spacing),
        (900, 3.0, 2.0)
    );
    assert_eq!(*c.families, ["Other", "serif"]);
    assert_eq!(
        (c.size, c.weight, c.italic, c.word_spacing),
        (12.0, 300, true, 0.0)
    );
    // The `style` attribute wins over the sheet for paint.
    assert!(t.paints[0].starts_with("fill:red;"), "{}", t.paints[0]);
}

#[test]
fn inherit_takes_the_parent_value() {
    let t = one(
        r#"<g font-size="30" fill="blue"><text font-size="inherit" fill="inherit">a
             <tspan fill="red" stroke="inherit">b</tspan></text></g>"#,
    );
    assert_eq!(t.spans[0].size, 30.0);
    // Paint not declared on the text or its spans inherits through the
    // placeholder.
    assert_eq!(t.paints, ["", "fill:red;"]);
    // Where a rule could reach the placeholder, it says so.
    let t = one(
        r#"<style>path { stroke: red }</style><g fill="blue"><text fill="inherit">a
             <tspan fill="red" stroke="inherit">b</tspan></text></g>"#,
    );
    assert_eq!(t.paints, ["stroke:inherit;", "fill:red;stroke:inherit;"]);
}

#[test]
fn placeholders_avoid_elements_important_rules_name() {
    let form = |sheet: &str| {
        let src = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{sheet}</style><text>a</text></svg>"#
        );
        let doc = Document::parse(&src).unwrap();
        let f = extract(&doc, usize::MAX).unwrap().0.form;
        (f.shape, f.group, f.group_paint)
    };
    let plain = (ProbeShape::Path, "a", String::new());
    assert_eq!(form(""), plain);
    // Rules without `!important` lose to the placeholder's `style`.
    assert_eq!(
        form("path, a { fill: none }"),
        (ProbeShape::Path, "a", "fill:inherit;".into())
    );
    assert_eq!(
        form("path { fill: none !important } a { opacity: 0 !important }"),
        (ProbeShape::Polygon, "g", "fill:inherit;".into())
    );
    assert_eq!(
        form("svg [d] { marker: url(#m) !important }").0,
        ProbeShape::Polygon
    );
    // Rules that would match the text too, or about something else, stay.
    let elements = |sheet: &str| {
        let f = form(sheet);
        (f.0, f.1)
    };
    assert_eq!(
        elements("* { fill: none !important }"),
        (ProbeShape::Path, "a")
    );
    assert_eq!(
        elements("#t { fill: none !important }"),
        (ProbeShape::Path, "a")
    );
    assert_eq!(form("path { font-size: 9px !important }"), plain);
    // Every shape named: the first.
    assert_eq!(
        form("path, polygon { fill: none !important }").0,
        ProbeShape::Path
    );
}

#[test]
fn spans_with_the_same_paint_share_a_style() {
    let t = one(
        r#"<text fill="red">a<tspan font-size="20">b</tspan><tspan fill="blue">c</tspan>
             <tspan font-size="30">d</tspan></text>"#,
    );
    assert_eq!(t.spans.len(), 4);
    assert_eq!(t.paints.len(), 2);
    let paint: Vec<u32> = t.spans.iter().map(|s| s.paint).collect();
    assert_eq!(paint, [0, 0, 1, 0]);
}

#[test]
fn keeps_out_values_that_would_end_a_declaration() {
    let t = one(
        r#"<text fill="red;stroke:blue" stroke="url(&quot;#g&quot;)" opacity="0.5 !important">a</text>"#,
    );
    assert_eq!(t.paints[0], r##"stroke:url("#g");"##);
    assert!(!t.effects.contains("opacity"), "{}", t.effects);
}

#[test]
fn the_placeholder_takes_the_effects_and_the_id() {
    let t = one(r#"<clipPath id="c"><rect width="1" height="1"/></clipPath>
           <switch><text id="x&lt;" systemLanguage="en" transform="rotate(5)" opacity="0.5"
             clip-path="url(#c)" style="mix-blend-mode: multiply" class="k">a</text></switch>"#);
    assert_eq!(t.attrs, r#" id="x&lt;" systemLanguage="en""#);
    assert_eq!(
        t.effects,
        "transform:rotate(5);opacity:0.5;clip-path:url(#c);mix-blend-mode:multiply;"
    );
    // Initial values where a style sheet could set others.
    let t = one("<style>a { mask: url(#m); filter: none }</style><text>a</text>");
    assert_eq!(t.effects, "mask:none;filter:none;");
    assert!(!t.in_clip);
    let c = one(r#"<clipPath id="c"><text>a</text></clipPath>"#);
    assert!(c.in_clip);
}

#[test]
fn resolves_positions_and_rotations() {
    // As in `usvg`'s documentation of `resolve_positions_list`.
    let t = one(
        r#"<text>a<tspan x="10 20 30" dy="1em 2" rotate="5 6">bc</tspan>d
             <tspan x="100 110 120 130" font-size="10">e<tspan x="50" dx="10%">fg</tspan></tspan>h</text>"#,
    );
    assert_eq!(string(&t), "abcd efgh");
    let x: Vec<Option<f32>> = t.chars.iter().map(|c| c.x).collect();
    assert_eq!(
        x,
        [
            None,
            Some(10.0),
            Some(20.0),
            None,
            None,
            Some(100.0),
            Some(50.0),
            Some(120.0),
            None
        ]
    );
    assert_eq!(t.chars[1].dy, 16.0);
    assert_eq!(t.chars[2].dy, 2.0);
    assert_eq!(t.chars[6].dx, 20.0);
    // The last rotation repeats in its element and the ones after it.
    let r: Vec<f32> = t.chars.iter().map(|c| c.rotate).collect();
    assert_eq!(r, [0.0, 5.0, 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let t = one(r#"<text rotate="10 20">abc<tspan rotate="30">de</tspan>f</text>"#);
    let r: Vec<f32> = t.chars.iter().map(|c| c.rotate).collect();
    assert_eq!(r, [10.0, 20.0, 20.0, 30.0, 30.0, 20.0]);
}

#[test]
fn reads_anchors_baselines_shifts_and_decorations() {
    let t = one(
        r#"<g text-decoration="underline"><text text-anchor="middle" dominant-baseline="central"
             font-size="20">a<tspan alignment-baseline="hanging" baseline-shift="super">b</tspan>
             <tspan baseline-shift="50%" text-decoration="line-through overline"><tspan
               baseline-shift="sub" dominant-baseline="no-change">c</tspan></tspan></text></g>"#,
    );
    let [a, b, c] = [&t.spans[0], &t.spans[1], &t.spans[2]];
    assert_eq!((a.anchor, a.baseline), (Anchor::Middle, Baseline::Central));
    assert!(a.underline && !a.overline);
    assert_eq!(
        (b.baseline, b.shift.as_slice()),
        (Baseline::Hanging, &[Shift::Super][..])
    );
    assert_eq!(c.anchor, Anchor::Middle);
    assert_eq!(c.shift, [Shift::Sub, Shift::Length(10.0)]);
    assert!(c.underline && c.overline && c.line_through);
}

#[test]
fn reads_text_length_innermost_first() {
    let t = one(
        r#"<text textLength="100">ab<tspan textLength="2em" lengthAdjust="spacingAndGlyphs"
             font-size="10">cd</tspan></text>"#,
    );
    assert_eq!(
        t.adjust,
        [
            LengthAdjust {
                chars: 2..4,
                length: 20.0,
                glyphs: true
            },
            LengthAdjust {
                chars: 0..4,
                length: 100.0,
                glyphs: false
            },
        ]
    );
}

#[test]
fn hides_spans_that_are_not_displayed() {
    let t = one(r#"<text x="1 2 3">a<tspan display="none">b</tspan>c</text>"#);
    let hidden: Vec<bool> = t.chars.iter().map(|c| c.hidden).collect();
    assert_eq!(hidden, [false, true, false]);
    // A hidden character still takes its position.
    assert_eq!(t.chars[2].x, Some(3.0));
}

#[test]
fn reads_direction_and_percentages_of_the_viewport() {
    let t = one(r#"<text direction="rtl" x="50%" y="10%">a</text>"#);
    assert!(t.rtl);
    assert_eq!((t.chars[0].x, t.chars[0].y), (Some(100.0), Some(10.0)));
}

#[test]
fn rejects_text_on_a_path_and_vertical_text() {
    assert!(
        error(r##"<path id="p" d="M0 0L9 9"/><text><textPath href="#p">a</textPath></text>"##)
            .contains("<textPath>")
    );
    for mode in ["tb", "tb-rl", "vertical-rl", "vertical-lr"] {
        let e = error(&format!(r#"<g writing-mode="{mode}"><text>a</text></g>"#));
        assert!(e.contains("vertical"), "{mode}: {e}");
    }
    one(r#"<text writing-mode="lr-tb">a</text>"#);
}

#[test]
fn ignores_elements_other_than_spans() {
    let t = one("<text>a<title>no</title><desc>no</desc><tspan>b</tspan></text>");
    assert_eq!(string(&t), "ab");
    assert_eq!(texts("<text>a<text>b</text></text>").len(), 1);
}

#[test]
fn refuses_text_from_entities() {
    let src = r#"<!DOCTYPE svg [<!ENTITY t "<text>hi</text>">]><svg xmlns="http://www.w3.org/2000/svg">&t;<text>b</text></svg>"#;
    let opts = usvg::roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = Document::parse_with_options(src, opts).unwrap();
    let Err(TextError::Unsupported(e)) = extract(&doc, usize::MAX) else {
        panic!("accepted text from an entity");
    };
    assert!(e.contains("entity"), "{e}");
}

/// Spans with their own paint and text nodes split by comments are read
/// in linear time: four times the input takes well under the sixteen
/// times a quadratic read would. A ratio, not a wall-clock limit, so a
/// slow or busy machine does not fail it.
#[test]
fn reads_many_spans_and_text_nodes_in_linear_time() {
    let spans = |n: usize| {
        let mut s = String::from("<text>");
        for i in 0..n {
            let _ = write!(s, r##"<tspan fill="#{i:06x}">a</tspan>"##);
        }
        s.push_str("</text>");
        let t = std::time::Instant::now();
        assert_eq!(one(&s).paints.len(), n);
        t.elapsed()
    };
    let comments = |n: usize| {
        let s = format!("<text>{}</text>", "a<!---->".repeat(n));
        let t = std::time::Instant::now();
        assert_eq!(one(&s).chars.len(), n);
        t.elapsed()
    };
    for (what, read, n) in [
        (
            "spans",
            &spans as &dyn Fn(usize) -> std::time::Duration,
            25_000,
        ),
        ("comments", &comments, 75_000),
    ] {
        let (small, big) = (read(n), read(4 * n));
        assert!(
            big < small * 8 + std::time::Duration::from_millis(500),
            "{what}: {n} took {small:?}, {} took {big:?}",
            4 * n
        );
    }
}

/// Spans that inherit one `font-family` share its list, read once, and
/// the values the texts keep count toward the bytes allowed.
#[test]
fn shares_inherited_font_families_and_counts_what_texts_keep() {
    let families = vec!["Inter"; 1_000].join(",");
    let body = format!(
        r#"<g font-family="{families}"><text>{}</text><text>b</text></g>"#,
        "<tspan>a</tspan>".repeat(100)
    );
    let t = texts(&body);
    let first = &t[0].spans[0].families;
    assert_eq!(first.len(), 1_000);
    assert!(t
        .iter()
        .flat_map(|t| &t.spans)
        .all(|s| Arc::ptr_eq(&s.families, first)));

    // One list is kept once, however many spans inherit it.
    let src = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
    let doc = Document::parse(&src).unwrap();
    let once = families.len() + 1_000 * (5 + size_of::<String>());
    assert!(extract(&doc, once + 1_000).is_ok());
    let Err(TextError::Limit(e)) = extract(&doc, once / 2) else {
        panic!("kept more than allowed");
    };
    assert!(e.contains("text properties"), "{e}");
    // Paint declarations count too.
    let src = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><text>{}</text></svg>"#,
        r#"<tspan fill="red">a</tspan><tspan fill="blue">a</tspan>"#
    );
    let doc = Document::parse(&src).unwrap();
    assert!(matches!(extract(&doc, 10), Err(TextError::Limit(_))));
}
