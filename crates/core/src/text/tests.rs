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
    let one_line = lines::break_lines(&paragraph(&filled), f32::INFINITY);
    let written = lines(&doc(&para("Page 7 of 1234", "")), &s, 500.0).1;
    assert!((one_line.width - written.width).abs() < 0.01);
    let narrow = lines(&doc(&para("Page 7 of", "")), &s, 500.0).1.width + 1.0;
    let wrapped = lines::break_lines(&paragraph(&filled), narrow);
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
