use super::probe::ImageFormat;
use super::*;

fn fixture(name: &str) -> Vec<u8> {
    crate::test_fixtures::fixture(&format!("images/{name}"))
}

#[test]
fn probes_jpeg_png_and_exif() {
    let j = probe(&fixture("photo.jpg")).unwrap();
    assert_eq!(
        (j.width, j.height, j.components, j.orientation),
        (320, 200, 3, 1)
    );
    let r = probe(&fixture("photo-rot6.jpg")).unwrap();
    assert_eq!(r.orientation, 6);
    let c = probe(&fixture("photo-cmyk.jpg")).unwrap();
    assert_eq!((c.components, c.adobe), (4, true));
    let p = probe(&fixture("logo.png")).unwrap();
    assert_eq!((p.width, p.height), (160, 160));
    assert!(probe(b"GIF89a..").is_err());
    assert!(probe(&[0xFF, 0xD8, 0xFF]).is_err());
    let mut huge = fixture("logo.png");
    huge[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    assert!(matches!(
        probe(&huge),
        Err(ProbeError::TooLarge(m)) if m.contains("megapixels")
    ));
}

#[test]
fn decodes_png_alpha_and_drops_opaque_masks() {
    let d = ImageCrateDecoder.decode(&fixture("logo.png")).unwrap();
    assert!(matches!(
        d.body,
        ImageBody::Raw {
            color: RawColor::Rgb,
            alpha: Some(ref a),
            ..
        } if !a.binary
    ));
    let d = ImageCrateDecoder.decode(&fixture("palette.png")).unwrap();
    assert!(matches!(d.body, ImageBody::Raw { alpha: None, .. }));
    let d = ImageCrateDecoder
        .decode(&fixture("photo-rot6.jpg"))
        .unwrap();
    assert_eq!(d.display_size(), (200.0, 320.0));
}

#[test]
fn converts_cmyk_jpegs_to_rgb_samples_on_request() {
    let d = ImageCrateDecoder
        .decode(&fixture("photo-cmyk.jpg"))
        .unwrap();
    assert!(matches!(
        d.body,
        ImageBody::Jpeg {
            color: JpegColor::Cmyk { .. },
            ..
        }
    ));
    let rgb = d.cmyk_to_rgb().unwrap();
    assert_eq!(rgb.len(), d.width as usize * d.height as usize * 3);
    let png = ImageCrateDecoder.decode(&fixture("palette.png")).unwrap();
    assert!(png.cmyk_to_rgb().is_err());
}

#[test]
fn fit_modes() {
    let b = Rect::new(0.0, 0.0, 100.0, 50.0);
    let fit = |f| fit_rect(f, ObjectPosition::default(), b, (10.0, 10.0));
    assert_eq!(fit(ImageFit::Fill), (b, false));
    assert_eq!(
        fit(ImageFit::Contain),
        (Rect::new(25.0, 0.0, 50.0, 50.0), false)
    );
    assert_eq!(
        fit(ImageFit::Cover),
        (Rect::new(0.0, -25.0, 100.0, 100.0), true)
    );
    assert_eq!(
        fit(ImageFit::None),
        (Rect::new(45.0, 20.0, 10.0, 10.0), false)
    );
}

/// An `objectPosition` from its JSON form.
fn pos(json: &str) -> ObjectPosition {
    serde_json::from_str(json).unwrap()
}

#[test]
fn object_position_keywords_are_fractions_of_the_free_space() {
    use crate::schema::LengthPercent::{Percent, Points};
    let p = |x, y| ObjectPosition { x, y };
    assert_eq!(pos("{}"), ObjectPosition::default());
    assert_eq!(pos("{}"), p(Percent(0.5), Percent(0.5)));
    assert_eq!(
        pos(r#"{"x":"left","y":"top"}"#),
        p(Percent(0.0), Percent(0.0))
    );
    assert_eq!(
        pos(r#"{"x":"center","y":"center"}"#),
        p(Percent(0.5), Percent(0.5))
    );
    assert_eq!(
        pos(r#"{"x":"right","y":"bottom"}"#),
        p(Percent(1.0), Percent(1.0))
    );
    assert_eq!(
        pos(r#"{"x":"25%","y":"-50%"}"#),
        p(Percent(0.25), Percent(-0.5))
    );
    assert_eq!(pos(r#"{"x":10,"y":-2.5}"#), p(Points(10.0), Points(-2.5)));
    assert_eq!(pos(r#"{"y":"bottom"}"#), p(Percent(0.5), Percent(1.0)));
    for bad in [
        r#"{"x":"top"}"#,
        r#"{"x":"bottom"}"#,
        r#"{"y":"left"}"#,
        r#"{"y":"right"}"#,
        r#"{"x":"start"}"#,
        r#"{"x":"10pt"}"#,
        r#"{"z":0}"#,
    ] {
        assert!(
            serde_json::from_str::<ObjectPosition>(bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn object_position_places_contain_cover_and_none() {
    // A 100 × 50 box at (10, 20) and a square image.
    let b = Rect::new(10.0, 20.0, 100.0, 50.0);
    let at = |fit, json: &str| fit_rect(fit, pos(json), b, (10.0, 10.0));

    // contain: 50 × 50, free space 50 across and none down.
    let contain = |json| at(ImageFit::Contain, json);
    assert_eq!(
        contain(r#"{"x":"left"}"#),
        (Rect::new(10.0, 20.0, 50.0, 50.0), false)
    );
    assert_eq!(
        contain(r#"{"x":"center"}"#),
        (Rect::new(35.0, 20.0, 50.0, 50.0), false)
    );
    assert_eq!(
        contain(r#"{"x":"right"}"#),
        (Rect::new(60.0, 20.0, 50.0, 50.0), false)
    );
    assert_eq!(
        contain(r#"{"x":"20%"}"#),
        (Rect::new(20.0, 20.0, 50.0, 50.0), false)
    );
    assert_eq!(
        contain(r#"{"x":5}"#),
        (Rect::new(15.0, 20.0, 50.0, 50.0), false)
    );
    assert_eq!(
        contain(r#"{"y":"top"}"#),
        contain(r#"{"y":"bottom"}"#),
        "no free space down"
    );

    // cover: 100 × 100, 50 to spare down; always clipped.
    let cover = |json| at(ImageFit::Cover, json);
    assert_eq!(
        cover(r#"{"y":"top"}"#),
        (Rect::new(10.0, 20.0, 100.0, 100.0), true)
    );
    assert_eq!(
        cover(r#"{"y":"center"}"#),
        (Rect::new(10.0, -5.0, 100.0, 100.0), true)
    );
    assert_eq!(
        cover(r#"{"y":"bottom"}"#),
        (Rect::new(10.0, -30.0, 100.0, 100.0), true)
    );
    assert_eq!(
        cover(r#"{"y":"10%"}"#),
        (Rect::new(10.0, 15.0, 100.0, 100.0), true)
    );
    assert_eq!(
        cover(r#"{"y":-10}"#),
        (Rect::new(10.0, 10.0, 100.0, 100.0), true)
    );
    assert_eq!(
        cover(r#"{"x":"left"}"#),
        cover(r#"{"x":"right"}"#),
        "no spare width"
    );

    // none: intrinsic 10 × 10 with 90 and 40 to spare.
    let none = |json| at(ImageFit::None, json);
    assert_eq!(
        none(r#"{"x":"left","y":"top"}"#),
        (Rect::new(10.0, 20.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":"right","y":"top"}"#),
        (Rect::new(100.0, 20.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":"left","y":"bottom"}"#),
        (Rect::new(10.0, 60.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":"right","y":"bottom"}"#),
        (Rect::new(100.0, 60.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":"center","y":"center"}"#),
        (Rect::new(55.0, 40.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":"25%","y":"75%"}"#),
        (Rect::new(32.5, 50.0, 10.0, 10.0), false)
    );
    assert_eq!(
        none(r#"{"x":12,"y":4}"#),
        (Rect::new(22.0, 24.0, 10.0, 10.0), false)
    );

    // fill: the box, wherever the position says.
    for json in [r#"{"x":"right","y":"bottom"}"#, r#"{"x":-30,"y":"200%"}"#] {
        assert_eq!(at(ImageFit::Fill, json), (b, false));
    }
}

#[test]
fn an_image_placed_past_its_box_is_clipped() {
    let b = Rect::new(0.0, 0.0, 100.0, 50.0);
    let none = |json: &str| fit_rect(ImageFit::None, pos(json), b, (10.0, 10.0)).1;
    // Inside the box, edges included.
    for json in [
        "{}",
        r#"{"x":0,"y":0}"#,
        r#"{"x":90,"y":40}"#,
        r#"{"x":"0%","y":"100%"}"#,
    ] {
        assert!(!none(json), "{json}");
    }
    // Past an edge: lengths, percentages beyond 0–100 %, negative values.
    for json in [
        r#"{"x":-1}"#,
        r#"{"x":91}"#,
        r#"{"y":-0.5}"#,
        r#"{"y":45}"#,
        r#"{"x":"110%"}"#,
        r#"{"y":"-10%"}"#,
    ] {
        assert!(none(json), "{json}");
    }
    // A contained image moved off its centre line by a length.
    let contain = fit_rect(ImageFit::Contain, pos(r#"{"y":5}"#), b, (10.0, 10.0));
    assert_eq!(contain, (Rect::new(25.0, 5.0, 50.0, 50.0), true));
    // An image larger than the box is clipped wherever it sits.
    for json in [r#"{"x":"left","y":"top"}"#, r#"{"x":"right","y":"bottom"}"#] {
        assert!(
            fit_rect(ImageFit::None, pos(json), b, (200.0, 10.0)).1,
            "{json}"
        );
    }
}

/// `(x, y, w, h)`.
type Xywh = (f32, f32, f32, f32);

/// The rectangle and clip an image node is painted with, from a document
/// whose only node is `node` (store images `photo`, 240 × 150 pt, and
/// `chart`, an SVG of 300 × 180 pt), in a 100 × 100 box at (10, 20).
fn painted(node: &str) -> (Xywh, Option<Xywh>) {
    use crate::paint::Op;
    let mut clip = None;
    let mut rect = None;
    for op in &painted_ops(node) {
        match op {
            Op::ClipRect(r) => clip = Some((r.x, r.y, r.w, r.h)),
            // Orientation 1: [w 0 0 −h x y+h].
            Op::Image { matrix: m, .. } => rect = Some((m[4], m[5] + m[3], m[0], -m[3])),
            _ => {}
        }
    }
    (rect.expect("an image op"), clip)
}

/// The ops an image node is painted with (see [`painted`]).
fn painted_ops(node: &str) -> Vec<crate::paint::Op> {
    use crate::paint::ContentCtx;
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":200,"height":200}},"children":[{node}]}}]}}"#
    );
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let mut store = ImageStore::new();
    store.insert("photo", fixture("photo.jpg"));
    store.insert("chart", fixture("chart.svg"));
    let set = ImageSet::load(&doc.pages, &store, &ImageCrateDecoder).unwrap();
    let geo = crate::layout::Geometry {
        x: 10.0,
        y: 20.0,
        w: 100.0,
        h: 100.0,
        ..Default::default()
    };
    let ctx = ContentCtx {
        dx: 0.0,
        dy: 0.0,
        shift: 0.0,
        line_shifts: &[],
        window: None,
        opacity: 1.0,
        line_boxes: None,
        ctm: None,
        tag: None,
    };
    let no_fonts = crate::FontStore::new();
    let fonts = crate::fonts::FontSet::load(&doc, &no_fonts).unwrap();
    let mut ops = Vec::new();
    set.paint(&doc.pages[0].children[0], &geo, &ctx, &fonts, &mut ops);
    ops
}

#[test]
fn border_radius_rounds_an_image_without_overflow_hidden() {
    use crate::paint::{Op, PathSeg};
    let clip = |style: &str| {
        let ops = painted_ops(&format!(
            r#"{{"type":"image","src":"photo","fit":"cover","style":{{{style}}}}}"#
        ));
        assert!(!ops.iter().any(|op| matches!(op, Op::ClipRect(_))));
        ops.into_iter().find_map(|op| match op {
            Op::ClipPath { path, .. } => Some(path.segs),
            _ => None,
        })
    };
    let round = clip(r#""borderRadius":50"#).expect("a rounded clip");
    assert_eq!(round[0], PathSeg::MoveTo(60.0, 20.0));
    assert!(round.iter().any(|s| matches!(s, PathSeg::CubicTo(..))));
    // Square corners: the plain clip of an overflowing fit.
    let (_, square) = painted(r#"{"type":"image","src":"photo","fit":"cover"}"#);
    assert!(square.is_some());
}

fn near(a: Xywh, b: Xywh) -> bool {
    let d = [a.0 - b.0, a.1 - b.1, a.2 - b.2, a.3 - b.3];
    d.iter().all(|d| d.abs() < 1e-3)
}

#[test]
fn raster_and_svg_images_follow_their_object_position() {
    const BOX: Xywh = (10.0, 20.0, 100.0, 100.0);
    let image = |src: &str, fit: &str, position: &str| {
        painted(&format!(
            r#"{{"type":"image","src":"{src}","fit":"{fit}","objectPosition":{position}}}"#
        ))
    };
    for (src, fit, position, rect, clipped) in [
        // photo, contain: 100 × 62.5 with 37.5 to spare down.
        ("photo", "contain", "{}", (10.0, 38.75, 100.0, 62.5), false),
        (
            "photo",
            "contain",
            r#"{"y":"top"}"#,
            (10.0, 20.0, 100.0, 62.5),
            false,
        ),
        (
            "photo",
            "contain",
            r#"{"y":"bottom"}"#,
            (10.0, 57.5, 100.0, 62.5),
            false,
        ),
        // photo, cover: 160 × 100, 60 too wide.
        (
            "photo",
            "cover",
            r#"{"x":"left"}"#,
            (10.0, 20.0, 160.0, 100.0),
            true,
        ),
        (
            "photo",
            "cover",
            r#"{"x":"right"}"#,
            (-50.0, 20.0, 160.0, 100.0),
            true,
        ),
        (
            "photo",
            "cover",
            r#"{"x":"25%"}"#,
            (-5.0, 20.0, 160.0, 100.0),
            true,
        ),
        // photo, none: 240 × 150.
        (
            "photo",
            "none",
            r#"{"x":10,"y":"bottom"}"#,
            (20.0, -30.0, 240.0, 150.0),
            true,
        ),
        // chart (SVG), contain: 100 × 60 with 40 to spare down.
        (
            "chart",
            "contain",
            r#"{"x":"right","y":"bottom"}"#,
            (10.0, 60.0, 100.0, 60.0),
            false,
        ),
        (
            "chart",
            "contain",
            r#"{"y":"25%"}"#,
            (10.0, 30.0, 100.0, 60.0),
            false,
        ),
        // chart, cover: 166.67 × 100.
        (
            "chart",
            "cover",
            r#"{"x":"right"}"#,
            (-56.666_67, 20.0, 166.666_67, 100.0),
            true,
        ),
        (
            "chart",
            "none",
            r#"{"x":"left","y":"top"}"#,
            (10.0, 20.0, 300.0, 180.0),
            true,
        ),
        // fill ignores objectPosition. The box is the chart's viewport,
        // which its viewBox (xMidYMid meet) fits: 100 × 60, centred.
        ("photo", "fill", r#"{"x":"right","y":"bottom"}"#, BOX, false),
        (
            "chart",
            "fill",
            r#"{"x":"right","y":"bottom"}"#,
            (10.0, 40.0, 100.0, 60.0),
            false,
        ),
    ] {
        let (r, clip) = image(src, fit, position);
        assert!(near(r, rect), "{src} {fit} {position}: {r:?}");
        assert_eq!(
            clip.is_some(),
            clipped,
            "{src} {fit} {position}: clip {clip:?}"
        );
        if let Some(c) = clip {
            assert!(near(c, BOX), "clipped to the content box: {c:?}");
        }
    }
    // Without `objectPosition`, the image is centred as before.
    assert_eq!(
        painted(r#"{"type":"image","src":"photo","fit":"cover"}"#),
        image("photo", "cover", r#"{"x":"center","y":"center"}"#)
    );
}

#[test]
fn an_overflowing_object_position_is_centred_not_infinite() {
    use crate::schema::LengthPercent::Percent;
    // Validation keeps percentages within ±10000 %; without it, a huge one
    // times the free space overflows f32.
    let b = Rect::new(0.0, 0.0, 100.0, 50.0);
    let at = |x, y| fit_rect(ImageFit::Cover, ObjectPosition { x, y }, b, (10.0, 10.0));
    let centred = fit_rect(ImageFit::Cover, ObjectPosition::default(), b, (10.0, 10.0));
    for y in [Percent(f32::MAX), Percent(-f32::MAX)] {
        assert_eq!(at(Percent(0.5), y), centred, "{y:?}");
    }
    // Only the overflowing axis falls back.
    let (Rect { x: rx, y: ry, .. }, _) = at(Percent(0.0), Percent(f32::MAX));
    assert_eq!((rx, ry), (0.0, centred.0.y));
}

/// The operands of every `cm` and `re` operator in an uncompressed PDF.
fn placement_operands(pdf: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(pdf);
    let mut out = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for (i, t) in tokens.iter().enumerate() {
            let n = match *t {
                "cm" => 6,
                "re" => 4,
                _ => continue,
            };
            if i >= n {
                out.extend(tokens[i - n..i].iter().map(ToString::to_string));
            }
        }
    }
    out
}

#[test]
fn object_positions_at_the_bounds_render_finite_numbers() {
    let mut store = ImageStore::new();
    store.insert("photo", fixture("photo.jpg"));
    store.insert("chart", fixture("chart.svg"));
    let mut nodes = Vec::new();
    for src in ["photo", "chart"] {
        for fit in ["contain", "cover", "none"] {
            for (x, y) in [
                (r#""10000%""#, r#""-10000%""#),
                (r#""-10000%""#, r#""10000%""#),
                ("14400", "-14400"),
                ("-14400", "14400"),
            ] {
                nodes.push(format!(
                    r#"{{"type":"image","src":"{src}","fit":"{fit}","style":{{"width":100,"height":60}},"objectPosition":{{"x":{x},"y":{y}}}}}"#
                ));
            }
        }
    }
    let json = format!(
        r#"{{"version":1,"pages":[{{"size":{{"width":300,"height":300}},"children":[{}]}}]}}"#,
        nodes.join(",")
    );
    let options = crate::RenderOptions::default().with_compress(false);
    let pdf = crate::render(
        json.as_bytes(),
        crate::InputFormat::Json,
        &options,
        &crate::FontStore::new(),
        &store,
    )
    .unwrap();
    let operands = placement_operands(&pdf);
    // 24 images, each drawn with a matrix (6) and clipped to its box (4).
    assert!(operands.len() >= 24 * 10, "{} operands", operands.len());
    for n in &operands {
        assert!(
            n.parse::<f64>().is_ok_and(f64::is_finite),
            "non-finite operand `{n}` in the content stream"
        );
    }
}

/// Maps stored-pixel corner (a, b) (normalised, y down) through the matrix.
fn corner(m: [f32; 6], a: f32, b: f32) -> (f32, f32) {
    let (u, v) = (a, 1.0 - b);
    (m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5])
}

#[test]
fn orientation_matrices_move_the_top_left_pixel() {
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    let tl = |o| corner(image_matrix(o, r), 0.0, 0.0);
    assert_eq!(tl(1), (10.0, 20.0));
    assert_eq!(tl(2), (110.0, 20.0));
    assert_eq!(tl(3), (110.0, 70.0));
    assert_eq!(tl(4), (10.0, 70.0));
    assert_eq!(tl(5), (10.0, 20.0));
    assert_eq!(tl(6), (110.0, 20.0));
    assert_eq!(tl(7), (110.0, 70.0));
    assert_eq!(tl(8), (10.0, 70.0));
    // Rotation 6: the stored top-right pixel lands bottom-right.
    assert_eq!(corner(image_matrix(6, r), 1.0, 0.0), (110.0, 70.0));
}

#[test]
fn separate_stores_share_one_decode() {
    let bytes: Arc<[u8]> = fixture("palette.png").into();
    let a = ImageData::new(bytes.clone());
    let b = ImageData::new(bytes.to_vec().into());
    let da = a.decoded(&ImageCrateDecoder).unwrap();
    assert!(cache::contains(&a.bytes, a.hash));
    let db = b.decoded(&ImageCrateDecoder).unwrap();
    assert!(Arc::ptr_eq(&da, &db));
}

#[test]
fn probes_and_decodes_gif_and_webp() {
    let g = probe(&fixture("anim.gif")).unwrap();
    assert_eq!((g.format, g.width, g.height), (ImageFormat::Gif, 64, 64));
    let lossy = probe(&fixture("photo-lossy.webp")).unwrap();
    assert_eq!(
        (lossy.format, lossy.width, lossy.height),
        (ImageFormat::WebP, 160, 100)
    );
    let lossless = probe(&fixture("badge-lossless.webp")).unwrap();
    assert_eq!((lossless.width, lossless.height), (96, 96));
    assert!(probe(b"RIFF\0\0\0\0WEBPVP8Z").is_err());

    // First frame of the animation, with GIF transparency as alpha, which
    // is binary.
    let d = ImageCrateDecoder.decode(&fixture("anim.gif")).unwrap();
    assert!(matches!(d.body, ImageBody::Raw { alpha: Some(ref a), .. } if a.binary));
    let d = ImageCrateDecoder
        .decode(&fixture("photo-lossy.webp"))
        .unwrap();
    assert!(matches!(
        d.body,
        ImageBody::Raw {
            color: RawColor::Rgb,
            alpha: None,
            ..
        }
    ));
    let d = ImageCrateDecoder
        .decode(&fixture("badge-lossless.webp"))
        .unwrap();
    assert!(matches!(d.body, ImageBody::Raw { alpha: Some(_), .. }));
}

/// An emoji image the document already shows is the same image, as are
/// two emoji with the same bytes under two ids.
#[test]
fn emoji_images_are_deduplicated_with_the_document_s() {
    let json = r#"{"version":1,"emojiImages":{"😀":"logo","👍":"copy","❤":"other"},
        "pages":[{"size":{"width":100,"height":100},"children":[{"type":"image","src":"logo"}]}]}"#;
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let mut store = ImageStore::new();
    store.insert("logo", fixture("logo.png"));
    store.insert("copy", fixture("logo.png"));
    store.insert("other", fixture("palette.png"));
    let mut set = ImageSet::load(&doc.pages, &store, &ImageCrateDecoder).unwrap();
    assert_eq!(set.images.len(), 1);
    set.load_emoji(&doc.emoji_images, &store, &ImageCrateDecoder)
        .unwrap();
    assert_eq!(set.images.len(), 2);
    let node = set.key(&doc.pages[0].children[0]);
    assert_eq!(set.by_emoji.get("😀").copied(), node);
    assert_eq!(set.by_emoji.get("👍").copied(), node);
    assert_eq!(set.by_emoji.get("❤").copied(), Some(1));
}

/// Two different images are two images, even when their hashes are equal.
#[test]
fn images_with_equal_hashes_stay_apart() {
    let mut store = ImageStore::new();
    store.insert("logo", fixture("logo.png"));
    let hash = store.get("logo").unwrap().hash;
    store.insert_with_hash("other", fixture("palette.png"), hash);
    let size = |id: &str| {
        let d = ImageCrateDecoder
            .decode(&store.get(id).unwrap().bytes)
            .unwrap();
        (d.width, d.height)
    };
    assert_ne!(size("logo"), size("other"));

    let json = r#"{"version":1,"pages":[{"size":{"width":100,"height":100},"children":[
        {"type":"image","src":"logo"},{"type":"image","src":"other"}]}]}"#;
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let set = ImageSet::load(&doc.pages, &store, &ImageCrateDecoder).unwrap();
    assert_eq!(set.images.len(), 2);
    for (node, id) in doc.pages[0].children.iter().zip(["logo", "other"]) {
        let image = set.get(set.key(node).unwrap());
        assert_eq!((image.width, image.height), size(id), "{id}");
    }

    let json = r#"{"version":1,"emojiImages":{"😀":"other"},
        "pages":[{"size":{"width":100,"height":100},"children":[{"type":"image","src":"logo"}]}]}"#;
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let mut set = ImageSet::load(&doc.pages, &store, &ImageCrateDecoder).unwrap();
    set.load_emoji(&doc.emoji_images, &store, &ImageCrateDecoder)
        .unwrap();
    assert_eq!(set.images.len(), 2);
    let emoji = set.get(set.by_emoji["😀"]);
    assert_eq!((emoji.width, emoji.height), size("other"));
}

/// A stencil mask is 1 where the image is clear, each row padded to a
/// byte; flattening draws the samples on white with their alpha.
#[test]
fn masks_and_flattens_alpha() {
    let image = |samples: &[u8], alpha: &[u8], width: u32| DecodedImage {
        width,
        height: (alpha.len() as u32) / width,
        orientation: 1,
        body: ImageBody::Raw {
            color: RawColor::Gray,
            samples: crate::flate::deflate(samples, 6).unwrap(),
            alpha: Some(super::decode::Alpha {
                plane: crate::flate::deflate(alpha, 6).unwrap(),
                binary: alpha.iter().all(|&a| a == 0 || a == 255),
            }),
        },
    };
    let mut alpha = vec![255u8; 18];
    alpha[0] = 0;
    alpha[8] = 0;
    alpha[17] = 0;
    let d = image(&[0; 18], &alpha, 9);
    assert_eq!(d.stencil_mask().unwrap(), [0x80, 0x80, 0x00, 0x80]);
    let d = image(&[0, 0, 100, 200], &[0, 255, 128, 51], 2);
    // c·a + 255·(1 − a): 255, 0, 177.5 → 177, 255 − 55·0.2 = 244.
    assert_eq!(d.flattened_samples().unwrap(), [255, 0, 177, 244]);
}
