use super::*;
use crate::schema::Document;
use crate::{FontStore, InputFormat};

fn store() -> FontStore {
    let mut s = FontStore::new();
    for name in ["Inter-Regular", "Inter-Bold", "NotoSans-Regular"] {
        let bytes = std::fs::read(format!(
            "{}/../../fixtures/fonts/{name}.ttf",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        s.register(name, bytes).unwrap();
    }
    s
}

fn doc(text_node: &str) -> Document {
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"Inter-Bold","family":"Inter","weight":700}},
            {{"id":"NotoSans-Regular","family":"Noto Sans"}}],"fallbackFonts":["NotoSans-Regular"],
            "pages":[{{"size":{{"width":600,"height":800}},"children":[{text_node}]}}]}}"#
    );
    crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap()
}

fn para(text: &str, style: &str) -> String {
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{{"text":"{text}","font":"Inter-Regular","size":10}}]}}"#
    )
}

/// Lays out the document's first node at `width`; returns line texts and
/// the layout.
fn lines(d: &Document, store: &FontStore, width: f32) -> (Vec<String>, LineLayout) {
    let fonts = FontSet::load(d, store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout(node, width).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    let texts = layout
        .lines
        .iter()
        .map(|l| {
            let s = p
                .glyphs
                .get(l.start as usize)
                .map_or(p.text.len(), |g| g.cluster as usize);
            let e = p
                .glyphs
                .get(l.end as usize)
                .map_or(p.text.len(), |g| g.cluster as usize);
            let mut t = p.text[s..e].replace('\u{AD}', "");
            if l.hyphen.is_some() {
                t.push('-');
            }
            if l.ellipsis.is_some() {
                t.push('…');
            }
            t.trim_end().to_string()
        })
        .collect();
    (texts, layout)
}

const LOREM: &str = "The quick brown fox jumps over the lazy dog and keeps running far away";

#[test]
fn wraps_within_width_and_stacks_lines() {
    let s = store();
    let d = doc(&para(LOREM, ""));
    let (texts, l) = lines(&d, &s, 120.0);
    assert!(texts.len() > 2, "{texts:?}");
    assert_eq!(texts.join(" "), LOREM);
    for line in &l.lines {
        assert!(line.width <= 120.0 + 1e-3, "{}", line.width);
    }
    for w in l.lines.windows(2) {
        assert!((w[1].top - (w[0].top + w[0].height)).abs() < 1e-4);
    }
    assert!((l.height - l.lines.iter().map(|x| x.height).sum::<f32>()).abs() < 1e-3);
}

#[test]
fn infinite_width_is_one_line() {
    let s = store();
    let d = doc(&para(LOREM, ""));
    let (texts, _) = lines(&d, &s, f32::INFINITY);
    assert_eq!(texts, vec![LOREM]);
}

#[test]
fn line_height_multiplier() {
    let s = store();
    let d = doc(&para("a b c", r#""lineHeight":2"#));
    let (_, l) = lines(&d, &s, 500.0);
    assert!((l.height - 20.0).abs() < 1e-3, "{}", l.height);
}

#[test]
fn newline_forces_break() {
    let s = store();
    let d = doc(&para("one\\ntwo", ""));
    let (texts, l) = lines(&d, &s, 500.0);
    assert_eq!(texts, vec!["one", "two"]);
    assert!(l.lines[0].forced);
}

#[test]
fn long_word_breaks_anywhere_in_normal_but_not_in_nowrap() {
    let s = store();
    let word = "Supercalifragilisticexpialidocious";
    let (texts, _) = lines(&doc(&para(word, "")), &s, 50.0);
    assert!(texts.len() > 1);
    assert_eq!(texts.concat(), word);
    let (texts, l) = lines(&doc(&para(word, r#""whiteSpace":"nowrap""#)), &s, 50.0);
    assert_eq!(texts.len(), 1);
    assert!(l.width > 50.0);
}

#[test]
fn hyphenation_shows_a_hyphen() {
    let s = store();
    let d = doc(&para(
        "Internationalization specifications recommend automatic hyphenation",
        r#""hyphenate":true,"lang":"en""#,
    ));
    let (texts, l) = lines(&d, &s, 90.0);
    assert!(texts.iter().any(|t| t.ends_with('-')), "{texts:?}");
    assert!(l.lines.iter().all(|x| x.width <= 90.0 + 1e-3));
}

#[test]
fn max_lines_with_ellipsis() {
    let s = store();
    let d = doc(&para(LOREM, r#""maxLines":2,"ellipsis":true"#));
    let (texts, l) = lines(&d, &s, 120.0);
    assert_eq!(texts.len(), 2);
    assert!(texts[1].ends_with('…'), "{texts:?}");
    assert!(l.lines[1].width <= 120.0 + 1e-3);
}

#[test]
fn falls_back_for_missing_glyphs() {
    let s = store();
    // U+0D9A (Sinhala) is in neither font: stays in the primary font.
    // U+20BF (Bitcoin sign) is missing from Inter 4.1 but present in Noto.
    let d = doc(&para("a \u{20BF} b", ""));
    let fonts = FontSet::load(&d, &s).unwrap();
    let inter = fonts.key("Inter-Regular").unwrap();
    let noto = fonts.key("NotoSans-Regular").unwrap();
    let mut shaper = RustybuzzShaper::new(&fonts);
    let crate::schema::NodeKind::Text { runs } = &d.pages[0].children[0].kind else {
        panic!()
    };
    let p = shaper.prepare(
        runs,
        &ParaStyle::from_style(&crate::schema::Style::default(), None),
    );
    let fonts_used: Vec<_> = p.segments.iter().map(|s| s.font).collect();
    if fonts.get(inter).glyph('\u{20BF}').is_none() {
        assert_eq!(fonts_used, vec![inter, noto, inter]);
    } else {
        assert_eq!(fonts_used, vec![inter]);
    }
}

#[test]
fn placeholder_reserves_width_of_999() {
    let s = store();
    let d = doc(
        r#"{"type":"text","runs":[{"placeholder":"totalPages","font":"Inter-Regular","size":10}]}"#,
    );
    let (_, l) = lines(&d, &s, 500.0);
    let d2 = doc(&para("999", ""));
    let (_, l2) = lines(&d2, &s, 500.0);
    assert!((l.width - l2.width).abs() < 1e-4);
}

#[test]
fn letter_spacing_widens() {
    let s = store();
    let plain = lines(&doc(&para("abcd", "")), &s, 500.0).1.width;
    let d = doc(
        r#"{"type":"text","runs":[{"text":"abcd","font":"Inter-Regular","size":10,"letterSpacing":2}]}"#,
    );
    let spaced = lines(&d, &s, 500.0).1.width;
    assert!((spaced - plain - 8.0).abs() < 1e-3, "{plain} {spaced}");
}

#[test]
fn empty_text_has_no_height() {
    let s = store();
    let (_, l) = lines(&doc(&para("   ", "")), &s, 100.0);
    assert_eq!(l.height, 0.0);
}

/// The closed-form cell layout (layout/fast.rs) must match Taffy exactly.
#[test]
fn simple_cell_fast_path_matches_taffy() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    let mut store = FontStore::new();
    for name in [
        "Inter-Regular",
        "Inter-Bold",
        "Inter-Italic",
        "NotoSans-Regular",
        "RobotoMono-Regular",
    ] {
        let bytes = std::fs::read(format!("{dir}fonts/{name}.ttf")).unwrap();
        store.register(name, bytes).unwrap();
    }
    let cell = |style: &str, text: &str| {
        format!(
            r#"{{"style":{{{style}}},"children":[{{"type":"text","style":{{"maxLines":2,"ellipsis":true}},"runs":[{{"text":"{text}","font":"Inter-Regular","size":9}}]}}]}}"#
        )
    };
    let synthetic = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":400,"height":800}},"children":[
            {{"type":"table","columns":[{{"width":"auto"}},{{"width":"1fr"}},{{"width":60}}],"rows":[
              {{"minHeight":40,"cells":[{a},{b},{c}]}},
              {{"cells":[{d},{e},{f}]}}]}}]}}]}}"#,
        a = cell(
            r#""padding":{"top":2,"left":4},"verticalAlign":"middle","borderWidth":{"bottom":2}"#,
            "Short"
        ),
        b = cell(
            r#""verticalAlign":"bottom","padding":{"right":6}"#,
            "A longer cell that wraps over more than two lines so that the ellipsis cuts it"
        ),
        c = cell("", "Fixed-width column"),
        d = cell(
            r#""padding":{"top":1,"right":1,"bottom":1,"left":1},"borderWidth":{"top":1,"right":3,"bottom":1,"left":1}"#,
            "x"
        ),
        e = cell(
            r#""backgroundColor":[1,0,0,1]"#,
            "Hyphenation-less wordsthatareverylongindeed"
        ),
        f = cell(r#""verticalAlign":"middle""#, ""),
    );
    let mut docs = vec![synthetic.into_bytes()];
    for f in ["table-basic", "table-split-header-repeat", "invoice"] {
        docs.push(std::fs::read(format!("{dir}docs/{f}.json")).unwrap());
    }
    for json in docs {
        let d = crate::validate::parse(&json, InputFormat::Json).unwrap();
        let fonts = FontSet::load(&d, &store).unwrap();
        let spec = &d.pages[0];
        let w = spec.size.width - spec.margin.left - spec.margin.right;
        let run = |fast: bool| {
            let mut cache = TextCache::new(RustybuzzShaper::new(&fonts), None);
            let mut tree = crate::layout::Tree::build(&spec.children);
            crate::layout::compute_tree(&mut tree, w, 0.0, &mut cache, fast).unwrap();
            tree.nodes.iter().map(|n| n.geo).collect::<Vec<_>>()
        };
        let (fast, slow) = (run(true), run(false));
        assert_eq!(fast.len(), slow.len());
        for (i, (a, b)) in fast.iter().zip(&slow).enumerate() {
            let close = |x: f32, y: f32| (x - y).abs() < 1e-3;
            let same = close(a.x, b.x)
                && close(a.y, b.y)
                && close(a.w, b.w)
                && close(a.h, b.h)
                && a.border.iter().zip(&b.border).all(|(x, y)| close(*x, *y))
                && a.padding.iter().zip(&b.padding).all(|(x, y)| close(*x, *y));
            assert!(same, "node {i}: fast {a:?} vs taffy {b:?}");
        }
    }
}
