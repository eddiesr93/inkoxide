use pdf_writer::Str;

use super::content::content_stream;
use super::document::pdf_date;
use super::nav::uri_bytes;
use super::*;
use crate::paint::{Dash, FillRule, Op, Path, PathSeg};

#[test]
fn written_uri_len_bounds_what_uris_take() {
    let written = |uri: &str| {
        let mut c = pdf_writer::Chunk::new();
        c.indirect(Ref::new(1)).primitive(Str(&uri_bytes(uri)));
        let mut empty = pdf_writer::Chunk::new();
        empty.indirect(Ref::new(1)).primitive(Str(b""));
        c.as_bytes().len() - empty.as_bytes().len() + 2
    };
    for uri in [
        "https://example.com/".to_string(),
        format!("https://example.com/{}", "x".repeat(100)),
        format!("https://example.com/{}", "(".repeat(100)),
        format!("https://example.com/{}", "é 😀".repeat(100)),
    ] {
        let n = written(&uri);
        assert!(n <= written_uri_len(&uri), "{uri}: {n}");
        assert!(written_uri_len(&uri) <= 2 * n + 66, "{uri}: {n}");
    }
}

#[test]
fn numbers_are_rounded_bounded_and_never_non_finite() {
    assert_eq!(round_pt(1.234_56), 1.235);
    assert_eq!(round_pt(-0.000_4), -0.0);
    // Tiny values keep their precision where they are not rounded.
    assert_eq!(number(1e-7), 1e-7);
    assert_eq!(number(-1e-30), -1e-30);
    // Large values pass unchanged up to the bound, and stop there,
    // whether or not they are too large to scale by 1000.
    for v in [1e6, 1e10, MAX_NUMBER] {
        assert_eq!(number(v), v);
        assert_eq!(number(-v), -v);
        assert_eq!(round_pt(v), v);
    }
    for v in [1.5e11, 1e12, 1e30, f32::MAX] {
        assert_eq!(number(v), MAX_NUMBER);
        assert_eq!(number(-v), -MAX_NUMBER);
        assert_eq!(round_pt(v), MAX_NUMBER);
        assert_eq!(round_pt(-v), -MAX_NUMBER);
        assert_eq!(round_to(v, 1e5), MAX_NUMBER);
    }
    // ±2^31 would be written as a 32-bit integer: -2^31 is past the
    // range pdf-writer can register.
    assert!(number(-2_147_483_648.0) > -2_147_483_648.0);
    assert!(number(2_147_483_648.0) < 2_147_483_648.0);
    for v in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        assert_eq!(number(v), 0.0);
        assert_eq!(round_pt(v), 0.0);
        assert_eq!(round_to(v, 1e5), 0.0);
    }
}

#[test]
fn bounded_numbers_are_written_as_numbers_readers_parse() {
    // pdf-writer writes a finite f32 as an integer when it is one
    // within 32 bits, as a real below 1e12, and digits only (an
    // integer past 32 bits) from there. Every bounded number is one of
    // the first two.
    let mut c = Content::new();
    for v in [
        MAX_NUMBER,
        -MAX_NUMBER,
        3e9,
        2_147_483_648.0,
        -2_147_483_648.0,
        1e30,
        -f32::MAX,
        1e-7,
    ] {
        c.move_to(number(v), 0.0);
    }
    let out = String::from_utf8(content_bytes(c, Profile::default()).unwrap()).unwrap();
    for token in out.split_ascii_whitespace() {
        if token.parse::<f64>().is_ok() && !token.contains('.') {
            assert!(token.parse::<i32>().is_ok(), "{token} in {out}");
        }
    }
    assert!(out.contains("-100000000000.0 0 m"), "{out}");
}

#[test]
fn dates_keep_the_sign_of_offsets_under_an_hour() {
    let t = Timestamp {
        local: crate::date::LocalDateTime {
            year: 2026,
            month: 10,
            day: 12,
            hour: 9,
            minute: 30,
            second: 0,
        },
        offset_minutes: -30,
    };
    assert_eq!(pdf_date(t), "D:20261012093000-00'30");
    for (off, zone) in [(0, "Z"), (30, "+00'30"), (-90, "-01'30"), (150, "+02'30")] {
        let t = Timestamp {
            offset_minutes: off,
            ..t
        };
        assert_eq!(pdf_date(t), format!("D:20261012093000{zone}"));
    }
    // The information dictionary and XMP name the same instant.
    let doc = br#"{"version":1,"pages":[{"size":{"width":10,"height":10}}]}"#;
    let options = crate::RenderOptions::default()
        .with_creation_date("2026-10-12T09:30:00-00:30")
        .with_pdf_a(true);
    let pdf = crate::render(
        doc,
        crate::InputFormat::Json,
        &options,
        &crate::FontStore::new(),
        &crate::ImageStore::new(),
    )
    .unwrap();
    let pdf = String::from_utf8_lossy(&pdf);
    assert!(
        pdf.contains("/CreationDate (D:20261012093000-00'30)"),
        "{pdf}"
    );
    assert!(pdf.contains("<xmp:CreateDate>2026-10-12T09:30:00-00:30</xmp:CreateDate>"));
}

/// The `/Rect` of every annotation of a 300 × 300 pt page holding
/// `children`, in order, rounded to points.
fn annotation_rects(children: &str) -> Vec<[f32; 4]> {
    let doc = format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
           "pages":[{{"size":{{"width":300,"height":300}},"children":[{children}]}}]}}"#
    );
    let options = crate::RenderOptions::default().with_compress(false);
    let pdf = crate::render(
        doc.as_bytes(),
        crate::InputFormat::Json,
        &options,
        &crate::test_fixtures::inter(),
        &crate::ImageStore::new(),
    )
    .unwrap();
    String::from_utf8_lossy(&pdf)
        .split("/Rect [")
        .skip(1)
        .map(|s| {
            let v: Vec<f32> = s
                .split(']')
                .next()
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse::<f32>().unwrap().round())
                .collect();
            [v[0], v[1], v[2], v[3]]
        })
        .collect()
}

#[test]
fn links_and_widgets_cover_their_transformed_area() {
    // A linked 100 × 20 box at the top left, holding a linked run and a
    // text field, moved by (150, 100).
    let moved = annotation_rects(
        r#"{"type":"box","link":"https://a.example","style":{"width":100,"height":40,"transform":[{"translate":[150,100]}]},
           "children":[{"type":"text","runs":[{"text":"Click","font":"Inter-Regular","size":10,"link":"https://b.example"}]},
             {"type":"field","style":{"width":100,"height":20},"field":{"name":"t","kind":"text","font":"Inter-Regular"}}]}"#,
    );
    assert_eq!(moved.len(), 3, "{moved:?}");
    assert_eq!(moved[0], [150.0, 160.0, 250.0, 200.0]);
    let [x0, y0, _, y1] = moved[1];
    assert_eq!((x0, y1), (150.0, 200.0), "{moved:?}");
    assert!(y0 > 180.0);
    // Widgets come after links; the field sits under the 12 pt line.
    assert_eq!(moved[2], [150.0, 168.0, 250.0, 188.0]);
    // Turned a quarter around its centre (50, 20): 40 wide, 100 tall.
    let turned = annotation_rects(
        r#"{"type":"box","link":"https://a.example","style":{"width":100,"height":40,"rotate":90}}"#,
    );
    assert_eq!(turned, [[30.0, 230.0, 70.0, 330.0]]);
}

#[test]
fn the_content_stream_writes_non_finite_numbers_as_zero_and_bounds_the_rest() {
    let doc: crate::schema::Document =
        serde_json::from_str(r#"{"version":1,"pages":[{"size":{"width":10,"height":10}}]}"#)
            .unwrap();
    let store = crate::FontStore::new();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let (inf, nan) = (f32::INFINITY, f32::NAN);
    let path = Path {
        segs: vec![
            PathSeg::MoveTo(inf, 1.0),
            PathSeg::LineTo(nan, -inf),
            PathSeg::CubicTo(1.0, inf, nan, 2.0, -inf, f32::MAX),
            PathSeg::Close,
        ],
    };
    let mut page = PageContent::new(10.0, 10.0);
    page.ops = vec![
        Op::Transform([inf, 0.0, nan, 1.0, -inf, f32::MAX]),
        Op::ClipRect(Rect::new(nan, 0.0, inf, 1.0)),
        Op::FillRect {
            rect: Rect::new(1.0, nan, 2.0, -inf),
            color: [nan, 0.5, inf, 1.0],
        },
        Op::FillPath {
            path: path.clone(),
            color: [0.0; 4],
            rule: FillRule::EvenOdd,
        },
        Op::StrokePath {
            path,
            color: [0.0; 4],
            width: inf,
            dash: Some(Dash {
                pattern: [nan, inf],
                round_cap: false,
            }),
        },
        Op::SetOpacity(nan),
        Op::Image {
            key: 0,
            matrix: [inf, nan, 0.0, -inf, 1.0, 2.0],
        },
    ];
    let out =
        String::from_utf8(content_stream(&page, &fonts, Profile::default()).unwrap()).unwrap();
    for token in out.split_whitespace() {
        let t = token.trim_start_matches('-');
        assert!(t != "inf" && t != "NaN", "`{token}` in:\n{out}");
        if t.parse::<f64>().is_ok() && !t.contains('.') {
            assert!(t.parse::<i32>().is_ok(), "`{token}` in:\n{out}");
        }
    }
    // f32::MAX stops at the bound, as a real.
    assert!(out.contains("0 0 0 1 0 100000000000.0 cm"), "{out}");
}

#[test]
fn meta_sets_the_producer_page_mode_and_page_layout() {
    let render = |meta: &str, bookmark: bool, pdf_a: bool| {
        let child = if bookmark {
            r#"{"type":"box","bookmark":{"title":"A"}}"#
        } else {
            ""
        };
        let doc = format!(
            r#"{{"version":1,"meta":{meta},"pages":[{{"size":{{"width":10,"height":10}},"children":[{child}]}}]}}"#
        );
        let options = crate::RenderOptions::default().with_pdf_a(pdf_a);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &options,
            &crate::FontStore::new(),
            &crate::ImageStore::new(),
        )
        .unwrap();
        String::from_utf8_lossy(&pdf).into_owned()
    };
    // By default, bookmarks open with the outline shown, and the
    // layout is the viewer's.
    let plain = render("{}", false, false);
    assert!(!plain.contains("/PageMode") && !plain.contains("/PageLayout"));
    let producer = concat!("/Producer (Inkoxide ", env!("CARGO_PKG_VERSION"), ")");
    assert!(plain.contains(producer), "{plain}");
    assert!(render("{}", true, false).contains("/PageMode /UseOutlines"));
    assert!(render(r#"{"pageMode":"useNone"}"#, true, false).contains("/PageMode /UseNone"));

    let set = render(
        r#"{"producer":"Acme Reports","pageMode":"fullScreen","pageLayout":"twoPageLeft"}"#,
        false,
        true,
    );
    for part in [
        "/PageMode /FullScreen",
        "/PageLayout /TwoPageLeft",
        "/Producer (Acme Reports)",
        "<pdf:Producer>Acme Reports</pdf:Producer>",
    ] {
        assert!(set.contains(part), "{part}");
    }
    assert!(!set.contains("Inkoxide"));
}

/// A document in Inter, its font store and an empty 100 pt page.
fn inter_page() -> (crate::schema::Document, crate::FontStore, PageContent) {
    let doc: crate::schema::Document = serde_json::from_str(
        r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
            "pages":[{"size":{"width":100,"height":100}}]}"#,
    )
    .unwrap();
    (
        doc,
        crate::test_fixtures::inter(),
        PageContent::new(100.0, 100.0),
    )
}

/// A span with `actual_text` around a red square, and an invisible
/// space as wide as the square.
fn span_ops(fonts: &FontSet<'_>, actual_text: &str) -> Vec<Op> {
    let font = fonts.key("Inter-Regular").unwrap();
    let space = crate::paint::InvisibleGlyph::space(fonts, font, 10.0, (20.0, 40.0), 30.0);
    assert!(space.is_some());
    let mut ops = Vec::new();
    crate::paint::span(&mut ops, actual_text, space, |ops| {
        ops.push(Op::FillRect {
            rect: Rect::new(20.0, 10.0, 30.0, 30.0),
            color: [1.0, 0.0, 0.0, 1.0],
        });
    });
    ops
}

#[test]
fn a_span_carries_its_actual_text_around_what_it_draws() {
    let (doc, store, mut page) = inter_page();
    let fonts = FontSet::load(&doc, &store).unwrap();
    page.ops = span_ops(&fonts, "a (b) \\ ☃");
    assert!(matches!(page.ops[0], Op::Span { .. }));
    assert!(matches!(page.ops[1], Op::FillRect { .. }));
    assert!(matches!(page.ops[2], Op::InvisibleText(_)));
    assert!(matches!(page.ops[3], Op::EndSpan));
    let text =
        String::from_utf8(content_stream(&page, &fonts, Profile::default()).unwrap()).unwrap();
    // Not ASCII: UTF-16BE with a byte order mark, as PDF text strings are.
    let mut hex = String::from("FEFF");
    for u in "a (b) \\ ☃".encode_utf16() {
        std::fmt::Write::write_fmt(&mut hex, format_args!("{u:04X}")).unwrap();
    }
    let span = format!("/Span <<\n  /ActualText <{hex}>\n>> BDC\n1 0 0 rg\n20 10 30 30 re\nf\n");
    assert!(text.contains(&span), "{span} in:\n{text}");
    // The invisible glyph: in its own save level, in mode 3, scaled to
    // 30 pt from the space's advance.
    let font = fonts.get(fonts.key("Inter-Regular").unwrap());
    let space = font.glyph(' ').unwrap();
    let scaling = round_pt(100.0 * 30.0 / (font.advance(space) * font.scale(10.0)));
    let glyph = format!("q\nBT\n/F0 10 Tf\n3 Tr\n{scaling} Tz\n1 0 0 -1 20 40 Tm\n");
    let g = text
        .find(&glyph)
        .unwrap_or_else(|| panic!("{glyph} in:\n{text}"));
    assert!(text[g..].contains("Tj\nET\nQ\nEMC"), "{text}");
}

/// Text extraction reads the span's `ActualText`, not the invisible
/// glyph (checked with poppler's `pdftotext` when it is installed).
#[test]
fn extraction_reads_the_actual_text() {
    let (doc, store, mut page) = inter_page();
    let fonts = FontSet::load(&doc, &store).unwrap();
    page.ops = span_ops(&fonts, "Snowman ☃");
    let mut sink = PdfWriterSink {
        compression: None,
        subset_fonts: true,
        profile: Profile::default(),
        encryption: None,
        parallel: false,
    };
    let pages = [sink.encode_page(&page, &fonts, None).unwrap()];
    let images = ImageSet::load(
        &doc.pages,
        &crate::ImageStore::new(),
        &crate::images::ImageCrateDecoder,
    )
    .unwrap();
    let pdf = sink
        .write(&DocumentOut {
            pages: &pages,
            meta: &doc.meta,
            creation_date: None,
            fonts: &fonts,
            unicode: &UnicodeMap::default(),
            files: &[],
            plan: None,
            images: &images,
            colors: &crate::text::ColorGlyphs::default(),
            marks: &Marks::default(),
        })
        .unwrap();
    let dir = std::env::temp_dir().join(format!("inkoxide-span-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("span.pdf");
    std::fs::write(&file, &pdf).unwrap();
    let out = std::process::Command::new("pdftotext")
        .args(["-enc", "UTF-8"])
        .arg(&file)
        .arg("-")
        .output();
    std::fs::remove_dir_all(&dir).unwrap();
    let Ok(out) = out else {
        // CI installs pdftotext, so there a missing one fails.
        assert!(
            std::env::var_os("CI").is_none(),
            "pdftotext is not installed"
        );
        return;
    };
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Snowman ☃");
}

#[test]
fn the_invisible_glyph_is_embedded() {
    let (doc, store, mut page) = inter_page();
    let fonts = FontSet::load(&doc, &store).unwrap();
    page.ops = span_ops(&fonts, "x");
    let sink = PdfWriterSink {
        compression: None,
        subset_fonts: true,
        profile: Profile::default(),
        encryption: None,
        parallel: false,
    };
    let encoded = sink.encode_page(&page, &fonts, None).unwrap();
    let font = fonts.key("Inter-Regular").unwrap();
    let space = fonts.get(font).glyph(' ').unwrap();
    assert_eq!(
        encoded
            .glyphs
            .get(&font)
            .map(|g| g.iter().copied().collect::<Vec<_>>()),
        Some(vec![space])
    );
}

#[test]
fn a_wide_invisible_glyph_keeps_to_the_pdf_a_1_bound() {
    let (doc, store, mut page) = inter_page();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let font = fonts.key("Inter-Regular").unwrap();
    let g = crate::paint::InvisibleGlyph::space(&fonts, font, 1.0, (0.0, 0.0), 5000.0);
    page.ops = vec![Op::InvisibleText(g.unwrap())];
    let operand = |profile: Profile, op: &str| -> f32 {
        let out = String::from_utf8(content_stream(&page, &fonts, profile).unwrap()).unwrap();
        let line = out.lines().find(|l| l.ends_with(op)).unwrap();
        line.split_whitespace()
            .rev()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    };
    // About 1.8 million percent at 1 pt, at every level but 1b.
    assert!(operand(Profile::default(), " Tz") > 1e6);
    for level in [crate::PdfALevel::A2b, crate::PdfALevel::A3b] {
        let p = Profile::new(Some(level));
        assert!(operand(p, " Tz") > 1e6);
        assert_eq!(operand(p, " Tf"), 1.0);
    }
    let one_b = Profile::new(Some(crate::PdfALevel::A1b));
    let (tz, size) = (operand(one_b, " Tz"), operand(one_b, " Tf"));
    assert_eq!(tz, pdfa::limits::REAL);
    // The same width: the font size makes up the scaling.
    let plain = (
        operand(Profile::default(), " Tz"),
        operand(Profile::default(), " Tf"),
    );
    assert!((tz * size - plain.0 * plain.1).abs() / (plain.0 * plain.1) < 1e-3);
}

#[test]
fn an_invisible_glyph_without_a_width_draws_nothing() {
    let (doc, store, mut page) = inter_page();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let font = fonts.key("Inter-Regular").unwrap();
    for width in [0.0, -5.0, f32::NAN, f32::INFINITY] {
        let g = crate::paint::InvisibleGlyph::space(&fonts, font, 10.0, (0.0, 0.0), width);
        page.ops = vec![Op::InvisibleText(g.unwrap())];
        let out =
            String::from_utf8(content_stream(&page, &fonts, Profile::default()).unwrap()).unwrap();
        assert!(!out.contains("Tr"), "{width}: {out}");
    }
}

#[test]
fn outline_entries_start_closed_and_open_the_view_they_ask_for() {
    use crate::pipeline::{Dest, OutlineEntry};
    use crate::schema::BookmarkZoom;
    let node: crate::schema::Node = serde_json::from_str(r#"{"type":"box"}"#).unwrap();
    let entry = |title, level, expanded, zoom| OutlineEntry {
        title,
        level,
        expanded,
        zoom,
        dest: Dest {
            page: 0,
            x: 10.0,
            y: 20.0,
        },
        node: &node,
    };
    // A (closed) holds B, which holds C; D (open) holds E and F.
    let entries = [
        entry("A", 1, false, None),
        entry("B", 2, true, Some(BookmarkZoom::Page)),
        entry("C", 3, true, Some(BookmarkZoom::Width)),
        entry("D", 1, true, Some(BookmarkZoom::Scale(1.5))),
        entry("E", 2, false, None),
        entry("F", 2, true, None),
    ];
    let mut w = Writer::new(Out::plain());
    let page = Ref::new(1000);
    let root = super::nav::write_outline(&mut w, &entries, &[page], &[]).unwrap();
    let out = String::from_utf8_lossy(&w.pdf.finish()).into_owned();
    let object = |r: Ref| {
        let start = out.find(&format!("\n{} 0 obj", r.get())).unwrap();
        out[start..start + out[start..].find("endobj").unwrap()].to_string()
    };
    let item = |n: i32| object(Ref::new(root.get() + n));
    // A closed entry counts what opening it shows, negated; the root
    // counts the entries shown: A, D, E and F.
    assert!(item(1).contains("/Count -2"), "{}", item(1));
    assert!(item(2).contains("/Count 1"), "{}", item(2));
    assert!(item(4).contains("/Count 2"), "{}", item(4));
    assert!(!item(5).contains("/Count"), "{}", item(5));
    assert!(object(root).contains("/Count 4"), "{}", object(root));
    // Each opens its own view of the node's top-left corner.
    assert!(
        item(1).contains("/Dest [1000 0 R /XYZ 10 20 0]"),
        "{}",
        item(1)
    );
    assert!(item(2).contains("/Dest [1000 0 R /Fit]"), "{}", item(2));
    assert!(item(3).contains("/Dest [1000 0 R /FitH 20]"), "{}", item(3));
    assert!(
        item(4).contains("/Dest [1000 0 R /XYZ 10 20 1.5]"),
        "{}",
        item(4)
    );
}
