use super::*;
use crate::schema::Document;
use crate::{FontStore, InputFormat};

fn store() -> FontStore {
    crate::test_fixtures::fonts(&["Inter-Regular", "Inter-Bold", "NotoSans-Regular"])
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
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout(node, width).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    (line_texts(p, &layout), layout)
}

/// The text of each line, mapped back through the glyph clusters.
fn line_texts(p: &Prepared, layout: &LineLayout) -> Vec<String> {
    layout
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
        .collect()
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
fn min_content_includes_the_hyphen() {
    // At its min-content width, a hyphenated paragraph breaks at every
    // hyphenation point and every line, hyphen included, fits.
    let s = store();
    let d = doc(&para("aaa\u{AD}bbbbbb\u{AD}cc", ""));
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let min = cache
        .measure(
            node,
            Size::NONE,
            Size {
                width: AvailableSpace::MinContent,
                height: AvailableSpace::MaxContent,
            },
        )
        .width;
    let (texts, l) = lines(&d, &s, min);
    assert_eq!(texts, ["aaa-", "bbbbbb-", "cc"]);
    let widest = l.lines.iter().map(|x| x.width).fold(0.0, f32::max);
    assert!((widest - min).abs() < 1e-3, "{widest} vs {min}");
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
    let mut shaper = RustybuzzShaper::new(&fonts).unwrap();
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
fn fill_numbers_puts_the_digits_in_place_of_the_stand_ins() {
    let s = store();
    let run = |r: &str| format!(r#"{{{r},"font":"Inter-Regular","size":10}}"#);
    let d = doc(&format!(
        r#"{{"type":"text","runs":[{},{},{},{}]}}"#,
        run(r#""text":"Page ""#),
        run(r#""placeholder":"pageNumber""#),
        run(r#""text":" of ""#),
        run(r#""placeholder":"totalPages""#),
    ));
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let NodeKind::Text { runs } = &node.kind else {
        unreachable!()
    };
    cache.layout(node, 500.0);
    let (p, style) = cache.prepared(node).unwrap();
    assert!(p.placeholders);
    assert_eq!(p.text, "Page 999 of 999");

    let numbers = PageNumbers {
        current: 7,
        total: 1234,
    };
    let paragraph = |prepared| Para {
        prepared,
        runs,
        style,
        fonts: &fonts,
    };
    let filled = prepare::fill_numbers(&paragraph(p), numbers);
    assert!(!filled.placeholders);
    assert_eq!(filled.text, "Page 7 of 1234");
    let pieces: Vec<&str> = filled
        .segments
        .iter()
        .map(|seg| &filled.text[seg.text.start as usize..seg.text.end as usize])
        .collect();
    assert_eq!(pieces, ["Page ", "7", " of ", "1234"]);
    for b in &filled.breaks {
        let at = filled.glyphs[b.glyph as usize].cluster as usize;
        assert!(filled.text[..at].ends_with(' '), "break at {at}");
    }

    // As wide as the numbers written out, and wrapped like them.
    let one_line = lines::break_lines(&paragraph(&filled), &LineBoxes::plain(f32::INFINITY));
    let written = lines(&doc(&para("Page 7 of 1234", "")), &s, 500.0).1;
    assert!((one_line.width - written.width).abs() < 0.01);
    let narrow = lines(&doc(&para("Page 7 of", "")), &s, 500.0).1.width + 1.0;
    let wrapped = lines::break_lines(&paragraph(&filled), &LineBoxes::plain(narrow));
    assert_eq!(line_texts(&filled, &wrapped), ["Page 7 of", "1234"]);
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

/// Per line, where each glyph that follows a tab starts, from the start of
/// the line.
fn after_tabs(d: &Document, store: &FontStore) -> Vec<Vec<f32>> {
    let fonts = FontSet::load(d, store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout(node, f32::INFINITY).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    let char_at = |i: usize| p.text[p.glyphs[i].cluster as usize..].chars().next();
    layout
        .lines
        .iter()
        .map(|line| {
            let mut x = 0.0;
            let mut xs = Vec::new();
            for i in line.start as usize..line.end as usize {
                if i > line.start as usize
                    && char_at(i - 1) == Some('\t')
                    && char_at(i) != Some('\t')
                {
                    xs.push(x);
                }
                x += p.glyphs[i].advance;
            }
            xs
        })
        .collect()
}

#[test]
fn tabs_advance_to_stops_eight_spaces_apart() {
    let s = store();
    let pre = |text: &str, ws: &str| doc(&para(text, &format!(r#""whiteSpace":"{ws}""#)));
    let width = |text: &str| lines(&pre(text, "pre"), &s, 500.0).1.width;
    let stop = width("        x") - width("x");
    for ws in ["pre", "pre-wrap"] {
        let xs = after_tabs(&pre(r"a\tx\nabc\tx\nWWW\tx\na\tb\tx\n\t\tx", ws), &s);
        // Columns line up in a proportional font: the stops are 8 spaces
        // of the font apart from the start of each line, not 8 characters.
        let expected = [
            vec![stop],
            vec![stop],
            vec![2.0 * stop],
            vec![stop, 2.0 * stop],
            vec![2.0 * stop],
        ];
        assert_eq!(xs.len(), expected.len(), "{ws}: {xs:?}");
        for (got, want) in xs.iter().zip(&expected) {
            assert_eq!(got.len(), want.len(), "{ws}: {xs:?}");
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 1e-3, "{ws}: {xs:?}, stops {stop} apart");
            }
        }
    }
    // Text ending less than half a `0` before a stop goes on to the next.
    let half_zero = width("0") / 2.0;
    let mut near = String::from("i");
    while width(&near) <= stop - half_zero {
        near.push('i');
    }
    assert!(width(&near) < stop, "{near}");
    let xs = after_tabs(&pre(&format!(r"{near}\tx"), "pre"), &s);
    assert!((xs[0][0] - 2.0 * stop).abs() < 1e-3, "{xs:?}");
}

#[test]
fn tabs_after_page_numbers_keep_their_stop() {
    let s = store();
    let d = doc(r#"{"type":"text","style":{"whiteSpace":"pre"},"runs":[
            {"placeholder":"pageNumber","font":"Inter-Regular","size":10},
            {"text":"\tx","font":"Inter-Regular","size":10}]}"#);
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let NodeKind::Text { runs } = &node.kind else {
        unreachable!()
    };
    cache.layout(node, 500.0);
    let (p, style) = cache.prepared(node).unwrap();
    let para = Para {
        prepared: p,
        runs,
        style,
        fonts: &fonts,
    };
    let x_after_tab = |p: &Prepared| {
        let tab = p.glyphs.iter().position(Glyph::is_tab).unwrap();
        p.glyphs[..=tab].iter().map(|g| g.advance).sum::<f32>()
    };
    let stand_in = x_after_tab(p);
    let filled = prepare::fill_numbers(
        &para,
        PageNumbers {
            current: 7,
            total: 9,
        },
    );
    assert_eq!(filled.text, "7\tx");
    assert!((x_after_tab(&filled) - stand_in).abs() < 1e-3);
}

/// The segments of `text` in Inter, with `fallbacks`: each as its text
/// and font id, and the glyphs of each, from fonts of `fixtures/fonts`.
fn segments(text: &str, fallbacks: &[&str]) -> Vec<(String, String, usize)> {
    let mut store = crate::test_fixtures::fonts(&["Inter-Regular", "NotoSans-Regular"]);
    for id in ["EmojiTest-COLR", "EmojiTest-CBDT"] {
        store
            .register(
                id,
                crate::test_fixtures::fixture(&format!("fonts/{id}.ttf")),
            )
            .unwrap();
    }
    let fallbacks = fallbacks
        .iter()
        .map(|f| format!("\"{f}\""))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},{{"id":"NotoSans-Regular","family":"Noto Sans"}},
            {{"id":"EmojiTest-COLR","family":"Emoji"}},{{"id":"EmojiTest-CBDT","family":"Emoji Bitmap"}}],
            "fallbackFonts":[{fallbacks}],
            "pages":[{{"size":{{"width":600,"height":800}},"children":[
            {{"type":"text","runs":[{{"text":"{text}","font":"Inter-Regular","size":10}}]}}]}}]}}"#
    );
    let d = crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap();
    let fonts = FontSet::load(&d, &store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    cache.layout(node, 600.0);
    let (p, _) = cache.prepared(node).unwrap();
    p.segments
        .iter()
        .map(|s| {
            (
                p.text[s.text.start as usize..s.text.end as usize].to_string(),
                fonts.get(s.font).id.to_string(),
                (s.glyphs.end - s.glyphs.start) as usize,
            )
        })
        .collect()
}

#[test]
fn an_emoji_sequence_stays_in_one_font_and_forms_its_ligature() {
    // Inter has none of them: each sequence goes whole to the colour font,
    // whose ligatures make one glyph of it (spaces stay with it).
    let segs = segments(
        "a \u{1F469}\u{200D}\u{1F4BB} \u{1F1EB}\u{1F1F7} 1\u{FE0F}\u{20E3} \u{1F44D}\u{1F3FD} b",
        &["NotoSans-Regular", "EmojiTest-COLR"],
    );
    let emoji: Vec<_> = segs
        .iter()
        .filter(|s| s.1 == "EmojiTest-COLR")
        .map(|s| (s.0.trim_end(), s.2))
        .collect();
    assert_eq!(
        emoji,
        [(
            "\u{1F469}\u{200D}\u{1F4BB} \u{1F1EB}\u{1F1F7} 1\u{FE0F}\u{20E3} \u{1F44D}\u{1F3FD}",
            8
        )],
        "{segs:?}"
    );
    assert_eq!(segs[0].1, "Inter-Regular");
    assert_eq!(segs.last().unwrap().1, "Inter-Regular");
}

#[test]
fn emoji_presentation_prefers_a_colour_fallback() {
    // `1` with a keycap: Inter has the digit, but the sequence is an
    // emoji, and the colour fallback has it too.
    let segs = segments("1\u{FE0F}\u{20E3}", &["NotoSans-Regular", "EmojiTest-COLR"]);
    assert_eq!(
        segs,
        [("1\u{FE0F}\u{20E3}".into(), "EmojiTest-COLR".into(), 1)]
    );
    // A plain digit stays in the run's font.
    let segs = segments("1", &["EmojiTest-COLR"]);
    assert_eq!(segs[0].1, "Inter-Regular");
    // The first colour fallback with the base wins over earlier others.
    let segs = segments("\u{1F600}", &["EmojiTest-COLR", "EmojiTest-CBDT"]);
    assert_eq!(segs[0].1, "EmojiTest-COLR");
    let segs = segments("\u{1F600}", &["EmojiTest-CBDT", "EmojiTest-COLR"]);
    assert_eq!(segs[0].1, "EmojiTest-CBDT");
    // Inter has a heart: as text, the run's font draws it; as an emoji,
    // the colour fallback.
    let fallbacks = ["NotoSans-Regular", "EmojiTest-COLR"];
    assert_eq!(
        segments("\u{2764}\u{FE0E}", &fallbacks)[0].1,
        "Inter-Regular"
    );
    assert_eq!(segments("\u{2764}", &fallbacks)[0].1, "Inter-Regular");
    assert_eq!(
        segments("\u{2764}\u{FE0F}", &fallbacks)[0].1,
        "EmojiTest-COLR"
    );
}

/// A text node of `runs`, each `(text, extra run properties)` in Inter at
/// 10 points.
fn runs(runs: &[(&str, &str)]) -> String {
    let runs: Vec<String> = runs
        .iter()
        .map(|(t, extra)| format!(r#"{{"text":"{t}","font":"Inter-Regular","size":10{extra}}}"#))
        .collect();
    format!(r#"{{"type":"text","runs":[{}]}}"#, runs.join(","))
}

#[test]
fn superscripts_and_subscripts_move_the_baseline_and_grow_the_line() {
    let s = store();
    let (_, plain) = lines(&doc(&runs(&[("H2O", "")])), &s, f32::INFINITY);
    let (_, sup) = lines(
        &doc(&runs(&[("x", ""), ("2", r#","verticalAlign":"super""#)])),
        &s,
        f32::INFINITY,
    );
    let (_, sub) = lines(
        &doc(&runs(&[("H", ""), ("2", r#","verticalAlign":"sub""#)])),
        &s,
        f32::INFINITY,
    );
    let (p, sup, sub) = (&plain.lines[0], &sup.lines[0], &sub.lines[0]);
    // Inter's OS/2: superscripts 717 up and subscripts 154 down, for a
    // size of 1229 (units of 2048 per em).
    let up = 10.0 * 717.0 / 1229.0;
    let down = 10.0 * 154.0 / 1229.0;
    assert!((sup.ascent - (p.ascent + up)).abs() < 1e-3, "{sup:?}");
    assert!((sup.height - (p.height + up)).abs() < 1e-3);
    assert!((sub.ascent - p.ascent).abs() < 1e-3);
    assert!((sub.height - (p.height + down)).abs() < 1e-3, "{sub:?}");
    // `baseline` is the default.
    let (_, base) = lines(
        &doc(&runs(&[("H2O", r#","verticalAlign":"baseline""#)])),
        &s,
        f32::INFINITY,
    );
    assert_eq!(base.lines[0].height, p.height);
}

#[test]
fn a_smaller_superscript_stays_within_the_line() {
    // A superscript at 6 points next to 10-point text: raised by its own
    // size's offset, it reaches no higher than the larger text.
    let s = store();
    let one = r#"{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":10},
        {"text":"2","font":"Inter-Regular","size":6,"verticalAlign":"super"}]}"#;
    let (_, l) = lines(&doc(one), &s, f32::INFINITY);
    let (_, p) = lines(&doc(&runs(&[("x", "")])), &s, f32::INFINITY);
    assert_eq!(l.lines[0].height, p.lines[0].height);
}

/// The advances of the glyphs a paragraph of `runs` shapes to.
fn advances(runs_json: &str) -> Vec<f32> {
    let s = store();
    let d = doc(runs_json);
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    cache.layout(node, f32::INFINITY);
    let (p, _) = cache.prepared(node).unwrap();
    p.glyphs.iter().map(|g| g.advance).collect()
}

#[test]
fn font_features_reach_the_shaper() {
    // Inter's digits are proportional; `tnum` makes them one width.
    let plain = advances(&runs(&[("1180", "")]));
    assert!(
        plain.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-3),
        "{plain:?}"
    );
    let tnum = advances(&runs(&[("1180", r#","fontFeatures":{"tnum":1}"#)]));
    assert!(
        tnum.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-3),
        "{tnum:?}"
    );
    // Turned off, or in another order, the same set.
    let off = advances(&runs(&[("1180", r#","fontFeatures":{"tnum":0}"#)]));
    assert_eq!(off, plain);
    let ordered = advances(&runs(&[("1180", r#","fontFeatures":{"zero":1,"tnum":1}"#)]));
    let reordered = advances(&runs(&[("1180", r#","fontFeatures":{"tnum":1,"zero":1}"#)]));
    assert_eq!(ordered, reordered);
    assert_eq!(ordered, tnum);
}

#[test]
fn the_same_word_shapes_apart_with_other_features() {
    // One paragraph, one word in two runs: the shaped word is cached by
    // its features too.
    let both = advances(&runs(&[
        ("1180 ", ""),
        ("1180", r#","fontFeatures":{"tnum":1}"#),
    ]));
    let plain = advances(&runs(&[("1180", "")]));
    let tnum = advances(&runs(&[("1180", r#","fontFeatures":{"tnum":1}"#)]));
    assert_eq!(&both[..4], &plain[..]);
    assert_eq!(&both[5..], &tnum[..]);
}

/// Lays out the document's first node in `boxes`; returns line texts and
/// the layout.
fn lines_in(d: &Document, store: &FontStore, boxes: &LineBoxes) -> (Vec<String>, LineLayout) {
    let fonts = FontSet::load(d, store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout_in(node, boxes).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    (line_texts(p, &layout), layout)
}

fn banded(width: f32, bands: &[(f32, f32, f32, f32)]) -> LineBoxes {
    LineBoxes {
        width,
        bands: bands
            .iter()
            .map(|&(top, bottom, left, right)| crate::layout::float::Band {
                top,
                bottom,
                left,
                right,
            })
            .collect(),
    }
}

#[test]
fn lines_beside_a_band_are_narrower_and_the_rest_full_width() {
    let s = store();
    let d = doc(&para(&format!("{LOREM} {LOREM}"), ""));
    let boxes = banded(200.0, &[(0.0, 30.0, 80.0, 0.0)]);
    let (texts, l) = lines_in(&d, &s, &boxes);
    let (plain, _) = lines(&d, &s, 200.0);
    assert_eq!(texts.join(" "), plain.join(" "));
    assert_eq!(l.boxes.len(), l.lines.len());
    let mut narrow = 0;
    for (line, &(left, avail)) in l.lines.iter().zip(&l.boxes) {
        assert!(line.width <= avail + 1e-3, "{line:?} in {avail}");
        if line.top < 30.0 {
            assert_eq!((left, avail), (80.0, 120.0));
            narrow += 1;
        } else {
            assert_eq!((left, avail), (0.0, 200.0));
        }
    }
    assert!(narrow >= 2 && texts.len() > plain.len(), "{texts:?}");
    // Without bands, the layout keeps no boxes.
    assert!(lines(&d, &s, 200.0).1.boxes.is_empty());
}

#[test]
fn a_line_that_does_not_fit_beside_a_band_moves_below_it() {
    let s = store();
    let d = doc(&para("Unbreakable words", ""));
    // Only 10 points free over the first 25: no word fits there.
    let boxes = banded(200.0, &[(0.0, 25.0, 0.0, 190.0)]);
    let (texts, l) = lines_in(&d, &s, &boxes);
    assert_eq!(texts, ["Unbreakable words"]);
    assert_eq!(l.lines[0].top, 25.0);
    assert_eq!(l.boxes[0], (0.0, 200.0));
    assert!((l.height - (25.0 + l.lines[0].height)).abs() < 1e-4);
    // A band that ends leaves room to every later line; with no band below,
    // a word too wide for the line breaks anyway.
    let (texts, _) = lines_in(&d, &s, &banded(30.0, &[(0.0, 5.0, 0.0, 20.0)]));
    assert!(texts.len() > 2, "{texts:?}");
}

#[test]
fn indent_and_ellipsis_use_the_line_box() {
    let s = store();
    let d = doc(&para(
        LOREM,
        r#""textIndent":20,"maxLines":1,"ellipsis":true"#,
    ));
    let boxes = banded(200.0, &[(0.0, 100.0, 50.0, 30.0)]);
    let (texts, l) = lines_in(&d, &s, &boxes);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].ends_with('…'), "{texts:?}");
    let line = &l.lines[0];
    assert!(line.width + line.indent <= 120.0 + 1e-3, "{line:?}");
    assert_eq!(l.boxes[0], (50.0, 120.0));
}

/// `node`, with `textWrap: pretty`.
fn pretty(mut d: Document) -> Document {
    let node = &mut d.pages[0].children[0];
    let style = crate::schema::Style {
        text_wrap: Some(crate::schema::TextWrap::Pretty),
        ..(*node.style).clone()
    };
    node.style = crate::schema::NodeStyle::new(style);
    d
}

/// Every line fits its box: the content width, less the indent on the
/// first line.
fn assert_fits(l: &LineLayout, width: f32) {
    assert_fits_shrunk(l, width, &[]);
}

/// [`assert_fits`], for lines that may shrink their spaces (`shrink[i]`
/// for line `i`) by up to a third.
fn assert_fits_shrunk(l: &LineLayout, width: f32, spaces: &[f32]) {
    for (i, line) in l.lines.iter().enumerate() {
        let avail = l.boxes.get(i).map_or(width, |b| b.1);
        let shrink = spaces.get(i).map_or(0.0, |s| s / 3.0);
        assert!(
            line.width + line.indent <= avail + shrink + 1e-3,
            "line {i}: {line:?} in {avail}"
        );
    }
}

/// The advance of the spaces on each line of the document's first node at
/// `width`.
fn line_spaces(d: &Document, store: &FontStore, width: f32) -> (LineLayout, Vec<f32>) {
    let fonts = FontSet::load(d, store).unwrap();
    let mut cache = TextCache::new(RustybuzzShaper::new(&fonts).unwrap(), None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout(node, width).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    let spaces = layout
        .lines
        .iter()
        .map(|l| {
            p.glyphs[l.start as usize..l.end as usize]
                .iter()
                .filter(|g| g.is_space())
                .map(|g| g.advance)
                .sum()
        })
        .collect();
    (layout, spaces)
}

/// Sum of the squared room left on every line but the last.
fn raggedness(l: &LineLayout, width: f32) -> f32 {
    let n = l.lines.len().saturating_sub(1);
    l.lines[..n]
        .iter()
        .map(|x| (width - x.width - x.indent).powi(2))
        .sum()
}

const PROSE: &str = "In olden times when wishing still helped one, there lived a king \
    whose daughters were all beautiful, but the youngest was so beautiful that the sun \
    itself, which has seen so much, was astonished whenever it shone in her face. Close \
    by the king's castle lay a great dark forest, and under an old lime tree in the forest \
    was a well, and when the day was very warm, the king's child went out into the forest";

#[test]
fn pretty_lines_are_more_even_than_first_fit() {
    let s = store();
    for (width, style) in [
        (150.0, ""),
        (210.0, ""),
        (150.0, r#""textAlign":"justify""#),
        (180.0, r#""textAlign":"justify","textIndent":15"#),
    ] {
        let d = doc(&para(PROSE, style));
        let (greedy_texts, greedy) = lines(&d, &s, width);
        let (texts, l) = lines(&pretty(doc(&para(PROSE, style))), &s, width);
        assert_eq!(texts.join(" "), greedy_texts.join(" "));
        let (_, spaces) = line_spaces(&pretty(doc(&para(PROSE, style))), &s, width);
        assert_fits_shrunk(&l, width, &spaces);
        // Justified lines shrink as well as stretch: compare ragged ones.
        assert!(
            !style.is_empty() || raggedness(&l, width) <= raggedness(&greedy, width) + 1e-3,
            "{width} {style}: {texts:#?}"
        );
    }
    // Here first fit leaves short lines in the middle that pretty evens out.
    let d = doc(&para(PROSE, ""));
    let (_, greedy) = lines(&d, &s, 140.0);
    let (_, l) = lines(&pretty(doc(&para(PROSE, ""))), &s, 140.0);
    assert!(raggedness(&l, 140.0) < 0.9 * raggedness(&greedy, 140.0));
}

#[test]
fn pretty_matches_first_fit_when_that_is_already_best() {
    // Equal words, three to a line exactly.
    let s = store();
    let word_doc = doc(&para("mmm", ""));
    let (_, one) = lines(&word_doc, &s, f32::INFINITY);
    let space = lines(&doc(&para("mmm mmm", "")), &s, f32::INFINITY).1.width - 2.0 * one.width;
    let width = 3.0 * one.width + 2.0 * space + 0.01;
    let text = ["mmm"; 9].join(" ");
    let (greedy, _) = lines(&doc(&para(&text, "")), &s, width);
    let (texts, l) = lines(&pretty(doc(&para(&text, ""))), &s, width);
    assert_eq!(texts, greedy);
    assert_eq!(texts.len(), 3);
    assert_fits(&l, width);
}

#[test]
fn pretty_falls_back_to_first_fit() {
    let s = store();
    // A word wider than the line breaks anywhere, as first fit breaks it.
    let long = "Supercalifragilisticexpialidocious and words";
    let (greedy, _) = lines(&doc(&para(long, "")), &s, 60.0);
    let (texts, _) = lines(&pretty(doc(&para(long, ""))), &s, 60.0);
    assert_eq!(texts, greedy);
    // Text without spaces, justified, has nothing to stretch.
    let cjk = "\u{4e00}\u{4e8c}\u{4e09}\u{56db}\u{4e94}\u{516d}\u{4e03}\u{516b}\u{4e5d}\u{5341}";
    let justified = r#""textAlign":"justify""#;
    let (greedy, _) = lines(&doc(&para(cjk, justified)), &s, 35.0);
    let (texts, _) = lines(&pretty(doc(&para(cjk, justified))), &s, 35.0);
    assert_eq!(texts, greedy);
    // `nowrap` does not break.
    let (texts, _) = lines(
        &pretty(doc(&para(PROSE, r#""whiteSpace":"nowrap""#))),
        &s,
        100.0,
    );
    assert_eq!(texts.len(), 1);
}

#[test]
fn pretty_keeps_hyphens_apart() {
    let s = store();
    let style = r#""hyphenate":true,"lang":"en","textAlign":"justify""#;
    for width in [140.0, 170.0] {
        let (texts, l) = lines(&pretty(doc(&para(PROSE, style))), &s, width);
        let (_, spaces) = line_spaces(&pretty(doc(&para(PROSE, style))), &s, width);
        assert_fits_shrunk(&l, width, &spaces);
        assert!(
            l.lines.iter().any(|x| x.hyphen.is_some()),
            "{width}: {texts:#?}"
        );
        // Two hyphens in a row cost more than evener spaces gain here.
        for w in l.lines.windows(2) {
            assert!(
                w[0].hyphen.is_none() || w[1].hyphen.is_none(),
                "{width}: {texts:#?}"
            );
        }
    }
}

#[test]
fn pretty_lines_follow_forced_breaks_ellipsis_and_bands() {
    let s = store();
    let text = format!("{PROSE}\\n{PROSE}");
    let (greedy, _) = lines(&doc(&para(&text, "")), &s, 160.0);
    let (texts, l) = lines(&pretty(doc(&para(&text, ""))), &s, 160.0);
    assert_fits(&l, 160.0);
    // The forced break ends a line in both.
    let ends = |t: &[String]| t.iter().filter(|x| x.ends_with("forest")).count();
    assert_eq!(ends(&texts), ends(&greedy));
    // `maxLines` and the ellipsis cut the optimal lines.
    let cut = r#""maxLines":2,"ellipsis":true"#;
    let (texts, l) = lines(&pretty(doc(&para(PROSE, cut))), &s, 160.0);
    assert_eq!(texts.len(), 2);
    assert!(texts[1].ends_with('…'));
    assert_fits(&l, 160.0);
    // Beside a band, lines fit their own boxes.
    let boxes = banded(200.0, &[(0.0, 40.0, 60.0, 0.0), (40.0, 70.0, 0.0, 30.0)]);
    let (texts, l) = lines_in(&pretty(doc(&para(PROSE, r#""textIndent":12"#))), &s, &boxes);
    assert!(texts.len() > 4);
    assert_fits(&l, 200.0);
}

#[test]
fn a_long_pretty_paragraph_is_quick() {
    let s = store();
    let text = [PROSE; 40].join(" ");
    let start = std::time::Instant::now();
    let (l, spaces) = line_spaces(
        &pretty(doc(&para(&text, r#""textAlign":"justify""#))),
        &s,
        300.0,
    );
    assert!(l.lines.len() > 100);
    assert_fits_shrunk(&l, 300.0, &spaces);
    assert!(start.elapsed().as_secs_f32() < 5.0, "{:?}", start.elapsed());
}

#[test]
fn justified_pretty_lines_may_shrink_their_spaces_by_a_third() {
    let s = store();
    let style = r#""textAlign":"justify""#;
    let mut shrunk = 0;
    for width in [130.0, 150.0, 170.0, 190.0, 230.0] {
        let d = pretty(doc(&para(PROSE, style)));
        let (l, spaces) = line_spaces(&d, &s, width);
        assert_fits_shrunk(&l, width, &spaces);
        shrunk += l.lines.iter().filter(|x| x.width > width + 1e-3).count();
        // The last line keeps its spaces.
        assert!(l.lines.last().unwrap().width <= width);
        // Ragged lines never shrink.
        let (_, l) = lines(&pretty(doc(&para(PROSE, ""))), &s, width);
        assert_fits(&l, width);
    }
    assert!(shrunk > 0);
}

/// Lays out a text node of `runs` (JSON) with `style` at `width`, with
/// image `logo` (160 × 160 px, 120 pt) for its image runs: the line texts
/// (an object as `#`), the layout and the objects.
fn objects_layout(
    runs: &str,
    style: &str,
    width: f32,
) -> (Vec<String>, LineLayout, Vec<objects::InlineObject>) {
    let d = doc(&format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{runs}]}}"#
    ));
    let s = store();
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut images = crate::ImageStore::new();
    images.insert("logo", crate::test_fixtures::fixture("images/logo.png"));
    let set = crate::images::ImageSet::load(&d.pages, &images, &crate::images::ImageCrateDecoder)
        .unwrap();
    let shaper = RustybuzzShaper::new(&fonts)
        .unwrap()
        .with_objects(Arc::new(set.inline_objects()));
    let mut cache = TextCache::new(shaper, None);
    let node = &d.pages[0].children[0];
    let layout = (*cache.layout(node, width).unwrap()).clone();
    let (p, _) = cache.prepared(node).unwrap();
    let texts = line_texts(p, &layout)
        .into_iter()
        .map(|t| t.replace('\u{FFFC}', "#"))
        .collect();
    (texts, layout, p.objects.clone())
}

fn image_run(extra: &str) -> String {
    format!(r#"{{"font":"Inter-Regular","size":10,"image":{{"src":"logo"{extra}}}}}"#)
}

fn text_run(text: &str) -> String {
    format!(r#"{{"text":"{text}","font":"Inter-Regular","size":10}}"#)
}

#[test]
fn an_inline_image_is_one_cluster_lines_break_around() {
    let runs = [text_run("aaa"), image_run(""), text_run("bbb ccc")].join(",");
    let (one, l, objects) = objects_layout(&runs, "", 600.0);
    assert_eq!(one, ["aaa#bbb ccc"]);
    // 1em high by default, square like the logo.
    assert_eq!(objects.len(), 1);
    assert_eq!((objects[0].width, objects[0].height), (10.0, 10.0));
    let wide = l.lines[0].width;
    // Breaks are allowed on both sides of an object (UAX #14 class CB).
    let (lines, _, _) = objects_layout(&runs, "", wide - 1.0);
    assert_eq!(lines, ["aaa#bbb", "ccc"]);
    let (lines, _, _) = objects_layout(&runs, "", 20.0);
    assert_eq!(lines, ["aaa", "#", "bbb", "ccc"]);
    // `nowrap` keeps them on one line.
    let (lines, _, _) = objects_layout(&runs, r#""whiteSpace":"nowrap""#, 20.0);
    assert_eq!(lines, ["aaa#bbb ccc"]);
    // An ellipsis can follow an object.
    let style = r#""maxLines":1,"ellipsis":true"#;
    let (lines, l, _) = objects_layout(&runs, style, 32.0);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(l.lines[0].ellipsis.is_some());
    assert!(l.lines[0].width <= 32.0 + 1e-3);
}

#[test]
fn inline_images_take_the_size_asked_and_grow_their_line() {
    let line = |extra: &str, align: &str| {
        let runs = [
            text_run("x"),
            image_run(extra).replace("}}", &format!("}}{align}}}")),
        ]
        .join(",");
        let (_, l, objects) = objects_layout(&runs, r#""lineHeight":1"#, 600.0);
        let o = &objects[0];
        (l.lines[0].ascent, l.lines[0].height, o.width, o.height)
    };
    // Text alone: 10 pt high, its baseline 10 · ascender / (asc + desc)
    // from the top, with `lineHeight: 1`.
    let (text_ascent, text_height, ..) = line(r#","height":1"#, "");
    assert!((text_height - 10.0).abs() < 1e-3);
    let descent = text_height - text_ascent;
    // On the baseline: 30 pt above it.
    let (a, h, w, ih) = line(r#","height":30"#, "");
    assert_eq!((w, ih), (30.0, 30.0));
    assert!(
        (a - 30.0).abs() < 1e-3 && (h - (30.0 + descent)).abs() < 1e-3,
        "{a} {h}"
    );
    // A width alone keeps the ratio.
    assert_eq!(line(r#","width":8"#, "").2, 8.0);
    // Points: the bottom 5 pt below the baseline.
    let (a, h, ..) = line(r#","height":30"#, r#","verticalAlign":-5"#);
    assert!(
        (a - 25.0).abs() < 1e-3 && (h - 25.0 - descent.max(5.0)).abs() < 1e-3,
        "{a} {h}"
    );
    // Top and bottom: as tall as the image, whatever the text.
    for align in ["top", "bottom"] {
        let (_, h, ..) = line(r#","height":30"#, &format!(r#","verticalAlign":"{align}""#));
        assert!((h - 30.0).abs() < 1e-3, "{align}: {h}");
    }
    // Middle: centred half an x-height above the baseline.
    let (a, ..) = line(r#","height":30"#, r#","verticalAlign":"middle""#);
    assert!(a > 15.0 && a < 20.0, "{a}");
    // Text top: the image's top at Inter's ascender (0.96875 em), above the
    // text's half-leading.
    let (a, ..) = line(r#","height":30"#, r#","verticalAlign":"textTop""#);
    assert!(
        (a - 9.6875).abs() < 1e-3 && a > text_ascent,
        "{a} {text_ascent}"
    );
}

#[test]
fn placeholders_keep_the_objects() {
    let runs = [
        image_run(""),
        r#"{"placeholder":"pageNumber","font":"Inter-Regular","size":10}"#.to_string(),
    ]
    .join(",");
    let d = doc(&format!(r#"{{"type":"text","runs":[{runs}]}}"#));
    let s = store();
    let fonts = FontSet::load(&d, &s).unwrap();
    let mut images = crate::ImageStore::new();
    images.insert("logo", crate::test_fixtures::fixture("images/logo.png"));
    let set = crate::images::ImageSet::load(&d.pages, &images, &crate::images::ImageCrateDecoder)
        .unwrap();
    let shaper = RustybuzzShaper::new(&fonts)
        .unwrap()
        .with_objects(Arc::new(set.inline_objects()));
    let mut cache = TextCache::new(shaper, None);
    let node = &d.pages[0].children[0];
    cache.layout(node, 600.0);
    let numbers = PageNumbers {
        current: 12345,
        total: 99999,
    };
    assert!(cache.set_numbers(node, numbers));
    let (p, _) = cache.prepared(node).unwrap();
    assert_eq!(p.objects.len(), 1);
    assert!(p.glyphs[0].is_object());
    assert_eq!(p.segments[0].object, Some(0));
}
