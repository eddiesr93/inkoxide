use super::probe::ImageFormat;
use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/images/");
    std::fs::read(format!("{p}{name}")).unwrap()
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
    assert!(probe(&huge).unwrap_err().contains("megapixels"));
}

#[test]
fn decodes_png_alpha_and_drops_opaque_masks() {
    let d = ImageCrateDecoder.decode(&fixture("logo.png")).unwrap();
    assert!(matches!(
        d.body,
        ImageBody::Raw {
            color: RawColor::Rgb,
            alpha: Some(_),
            ..
        }
    ));
    let d = ImageCrateDecoder.decode(&fixture("palette.png")).unwrap();
    assert!(matches!(d.body, ImageBody::Raw { alpha: None, .. }));
    let d = ImageCrateDecoder
        .decode(&fixture("photo-rot6.jpg"))
        .unwrap();
    assert_eq!(d.display_size(), (200.0, 320.0));
}

#[test]
fn fit_modes() {
    let b = (0.0, 0.0, 100.0, 50.0);
    assert_eq!(fit_rect(ImageFit::Fill, b, (10.0, 10.0)), (b, false));
    assert_eq!(
        fit_rect(ImageFit::Contain, b, (10.0, 10.0)),
        ((25.0, 0.0, 50.0, 50.0), false)
    );
    assert_eq!(
        fit_rect(ImageFit::Cover, b, (10.0, 10.0)),
        ((0.0, -25.0, 100.0, 100.0), true)
    );
    assert_eq!(
        fit_rect(ImageFit::None, b, (10.0, 10.0)),
        ((45.0, 20.0, 10.0, 10.0), false)
    );
}

/// Maps stored-pixel corner (a, b) (normalised, y down) through the matrix.
fn corner(m: [f32; 6], a: f32, b: f32) -> (f32, f32) {
    let (u, v) = (a, 1.0 - b);
    (m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5])
}

#[test]
fn orientation_matrices_move_the_top_left_pixel() {
    let r = (10.0, 20.0, 100.0, 50.0);
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
    assert!(cache::contains(a.hash));
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

    // First frame of the animation, with GIF transparency as alpha.
    let d = ImageCrateDecoder.decode(&fixture("anim.gif")).unwrap();
    assert!(matches!(d.body, ImageBody::Raw { alpha: Some(_), .. }));
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
