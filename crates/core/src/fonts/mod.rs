//! Font registry, metrics and the per-render font set.
//!
//! Fonts are registered explicitly (no system font discovery: determinism).
//! Registration decodes WOFF and WOFF2 files to the font they wrap, then
//! parses and validates the font once; per render, the fonts a document
//! declares are wrapped in shaping faces (`FontSet`).

mod woff;

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use crate::schema::Document;
use crate::InkoxideError;

/// Metrics read once at registration, in font units unless noted.
#[derive(Debug, Clone)]
pub(crate) struct FontMetrics {
    pub units_per_em: f32,
    pub ascender: f32,
    /// Negative (below the baseline).
    pub descender: f32,
    pub line_gap: f32,
    pub cap_height: f32,
    pub italic_angle: f32,
    pub bbox: [f32; 4],
    pub weight: u16,
    pub is_italic: bool,
    pub is_fixed_pitch: bool,
    pub is_serif: bool,
    pub post_script_name: String,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
    /// CFF outlines (embedded as `CIDFontType0C`) rather than TrueType.
    pub cff: bool,
}

/// A registered font face.
#[derive(Debug)]
pub struct FontData {
    /// The sfnt font (TrueType or CFF OpenType), decoded from WOFF or WOFF2
    /// when it was registered in one of them.
    pub(crate) bytes: Arc<[u8]>,
    /// xxh3 of `bytes`: the font's identity in process-wide caches.
    pub(crate) hash: u64,
    pub(crate) metrics: FontMetrics,
    /// Flate-compressed font program (`FontFile2`, or the CID-keyed CFF
    /// for `FontFile3`), computed on first use and
    /// shared by every later render.
    compressed: OnceLock<Arc<[u8]>>,
}

impl FontData {
    fn parse(id: &str, bytes: Arc<[u8]>) -> Result<Self, InkoxideError> {
        let err = |msg: &str| InkoxideError::Font(format!("font `{id}`: {msg}"));
        let bytes = match woff::decode(&bytes).map_err(|m| err(&m))? {
            Some(sfnt) => sfnt.into(),
            None => bytes,
        };
        let face = ttf_parser::Face::parse(&bytes, 0)
            .map_err(|e| err(&format!("not a valid TrueType/OpenType font ({e})")))?;
        let tables = face.tables();
        if face.is_variable() || tables.cff2.is_some() {
            return Err(err(
                "variable fonts are not supported; register a static instance",
            ));
        }
        let cff = tables.glyf.is_none();
        if cff && (tables.cff.is_none() || crate::pdf::cff::full_program(&bytes).is_none()) {
            return Err(err(
                "font has neither TrueType (glyf) nor embeddable CFF outlines",
            ));
        }
        if tables
            .cmap
            .is_none_or(|c| !c.subtables.into_iter().any(|s| s.is_unicode()))
        {
            return Err(err("font has no Unicode cmap"));
        }
        if rustybuzz::Face::from_slice(&bytes, 0).is_none() {
            return Err(err("font cannot be loaded for shaping"));
        }

        let upem = f32::from(face.units_per_em());
        let bbox = face.global_bounding_box();
        let ascender = f32::from(face.ascender());
        let post_script_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME && n.is_unicode())
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| id.to_string());
        let underline = face.underline_metrics();
        let strikeout = face.strikeout_metrics();
        let metrics = FontMetrics {
            units_per_em: upem,
            ascender,
            descender: f32::from(face.descender()),
            line_gap: f32::from(face.line_gap()),
            cap_height: face.capital_height().map_or(ascender * 0.7, f32::from),
            italic_angle: face.italic_angle(),
            bbox: [
                f32::from(bbox.x_min),
                f32::from(bbox.y_min),
                f32::from(bbox.x_max),
                f32::from(bbox.y_max),
            ],
            weight: face.weight().to_number(),
            is_italic: face.is_italic(),
            is_fixed_pitch: face.is_monospaced(),
            // OS/2 sFamilyClass (offset 30): IBM classes 1..=7 are serifs.
            is_serif: face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"OS/2"))
                .and_then(|t| t.get(30))
                .is_some_and(|class| (1..=7).contains(class)),
            post_script_name: sanitize_name(&post_script_name),
            underline_position: underline.map_or(-upem * 0.1, |m| f32::from(m.position)),
            underline_thickness: underline.map_or(upem * 0.05, |m| f32::from(m.thickness)),
            strikeout_position: strikeout.map_or(upem * 0.3, |m| f32::from(m.position)),
            strikeout_thickness: strikeout.map_or(upem * 0.05, |m| f32::from(m.thickness)),
            cff,
        };
        Ok(Self {
            hash: xxhash_rust::xxh3::xxh3_64(&bytes),
            bytes,
            metrics,
            compressed: OnceLock::new(),
        })
    }

    pub(crate) fn compressed(&self) -> Result<Arc<[u8]>, InkoxideError> {
        if let Some(c) = self.compressed.get() {
            return Ok(c.clone());
        }
        let data: Arc<[u8]> = if self.metrics.cff {
            // Checked at registration.
            let program = crate::pdf::cff::full_program(&self.bytes)
                .ok_or_else(|| InkoxideError::Internal("CFF conversion failed".into()))?;
            crate::pdf::deflate(&program, 6)?.into()
        } else {
            crate::pdf::deflate(&self.bytes, 6)?.into()
        };
        Ok(self.compressed.get_or_init(|| data).clone())
    }
}

/// PDF names may not contain whitespace or delimiters.
fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c))
        .collect();
    if s.is_empty() {
        "Font".into()
    } else {
        s
    }
}

/// Fonts available to a render, keyed by the id documents reference.
///
/// Cheap to clone: faces are shared behind `Arc`.
#[derive(Debug, Clone, Default)]
pub struct FontStore {
    fonts: BTreeMap<String, Arc<FontData>>,
}

impl FontStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) font `id` from TrueType or CFF OpenType bytes,
    /// plain or in a WOFF or WOFF2 file.
    ///
    /// # Errors
    ///
    /// Fails for bytes that are not a font, for WOFF and WOFF2 files that
    /// cannot be decoded, and for unsupported fonts (variable fonts).
    pub fn register(
        &mut self,
        id: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<(), InkoxideError> {
        let id = id.into();
        let data = FontData::parse(&id, bytes.into())?;
        self.fonts.insert(id, Arc::new(data));
        Ok(())
    }

    /// Removes font `id`; returns whether it was registered.
    pub fn unregister(&mut self, id: &str) -> bool {
        self.fonts.remove(id).is_some()
    }

    /// Whether font `id` is registered.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.fonts.contains_key(id)
    }

    /// Number of registered fonts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Whether no font is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Arc<FontData>> {
        self.fonts.get(id)
    }
}

/// Index of a font within a render's [`FontSet`] (document declaration
/// order); also its PDF resource number.
pub(crate) type FontKey = u16;

pub(crate) struct LoadedFont<'a> {
    pub data: &'a FontData,
    pub face: rustybuzz::Face<'a>,
}

impl LoadedFont<'_> {
    /// Scale from font units to points at `size`.
    pub fn scale(&self, size: f32) -> f32 {
        size / self.data.metrics.units_per_em
    }

    pub fn glyph(&self, c: char) -> Option<u16> {
        self.face.glyph_index(c).map(|g| g.0)
    }

    /// Horizontal advance of glyph `gid` in font units.
    pub fn advance(&self, gid: u16) -> f32 {
        f32::from(
            self.face
                .glyph_hor_advance(ttf_parser::GlyphId(gid))
                .unwrap_or(0),
        )
    }
}

/// The fonts of one document, ready for shaping.
pub(crate) struct FontSet<'a> {
    pub fonts: Vec<LoadedFont<'a>>,
    by_id: BTreeMap<&'a str, FontKey>,
    pub fallbacks: Vec<FontKey>,
}

impl<'a> FontSet<'a> {
    /// Assumes the document passed validation (every font is registered).
    pub fn load(doc: &'a Document, store: &'a FontStore) -> Result<Self, InkoxideError> {
        let mut fonts = Vec::with_capacity(doc.fonts.len());
        let mut by_id = BTreeMap::new();
        for decl in &doc.fonts {
            let data = store.get(&decl.id).ok_or_else(|| {
                InkoxideError::Internal(format!("font `{}` vanished after validation", decl.id))
            })?;
            let face = rustybuzz::Face::from_slice(&data.bytes, 0).ok_or_else(|| {
                InkoxideError::Font(format!("font `{}` cannot be loaded", decl.id))
            })?;
            by_id.insert(decl.id.as_str(), fonts.len() as FontKey);
            fonts.push(LoadedFont { data, face });
        }
        let fallbacks = doc
            .fallback_fonts
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect();
        Ok(Self {
            fonts,
            by_id,
            fallbacks,
        })
    }

    pub fn key(&self, id: &str) -> Option<FontKey> {
        self.by_id.get(id).copied()
    }

    pub fn get(&self, key: FontKey) -> &LoadedFont<'a> {
        &self.fonts[key as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../fixtures/fonts/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn registers_truetype_and_reads_metrics() {
        let mut s = FontStore::new();
        s.register("Inter", fixture("Inter-Regular.ttf")).unwrap();
        let m = &s.get("Inter").unwrap().metrics;
        assert_eq!(m.units_per_em, 2048.0);
        assert!(m.ascender > 0.0 && m.descender < 0.0);
        assert_eq!(m.post_script_name, "Inter-Regular");
        assert_eq!(m.weight, 400);
    }

    #[test]
    fn registers_cff() {
        let mut s = FontStore::new();
        s.register("Bold", fixture("InterDisplay-Bold.otf"))
            .unwrap();
        let m = &s.get("Bold").unwrap().metrics;
        assert!(m.cff);
        assert_eq!(m.weight, 700);
        assert!(!s.get("Bold").unwrap().compressed().unwrap().is_empty());
    }

    /// Drawing commands with their coordinates.
    type Outline = Vec<(char, Vec<f32>)>;

    /// Every glyph's advance and outline.
    fn glyphs(bytes: &[u8]) -> Vec<(Option<u16>, Outline)> {
        struct Path(Outline);
        impl ttf_parser::OutlineBuilder for Path {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.push(('M', vec![x, y]));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.push(('L', vec![x, y]));
            }
            fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
                self.0.push(('Q', vec![x1, y1, x, y]));
            }
            fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
                self.0.push(('C', vec![x1, y1, x2, y2, x, y]));
            }
            fn close(&mut self) {
                self.0.push(('Z', vec![]));
            }
        }
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        (0..face.number_of_glyphs())
            .map(|g| {
                let gid = ttf_parser::GlyphId(g);
                let mut path = Path(Vec::new());
                face.outline_glyph(gid, &mut path);
                (face.glyph_hor_advance(gid), path.0)
            })
            .collect()
    }

    /// Registers `plain` and its web font `web`; they must be the same font.
    fn assert_same_font(plain: &[u8], web: Vec<u8>) {
        let mut s = FontStore::new();
        s.register("plain", plain.to_vec()).unwrap();
        s.register("web", web).unwrap();
        let (a, b) = (s.get("plain").unwrap(), s.get("web").unwrap());
        assert_eq!(format!("{:?}", a.metrics), format!("{:?}", b.metrics));
        assert_eq!(glyphs(&a.bytes), glyphs(&b.bytes));
        assert!(!b.compressed().unwrap().is_empty());
    }

    #[test]
    fn registers_woff() {
        let ttf = fixture("RobotoMono-Regular.ttf");
        assert_same_font(&ttf, fixture("woff/RobotoMono-Regular.woff"));
        // WOFF keeps every table byte for byte.
        let mut s = FontStore::new();
        s.register("web", fixture("woff/RobotoMono-Regular.woff"))
            .unwrap();
        let (plain, web) = (
            ttf_parser::RawFace::parse(&ttf, 0).unwrap(),
            ttf_parser::RawFace::parse(&s.get("web").unwrap().bytes, 0).unwrap(),
        );
        assert_eq!(plain.table_records.len(), web.table_records.len());
        for t in plain.table_records {
            assert_eq!(plain.table(t.tag), web.table(t.tag), "{}", t.tag);
        }
    }

    #[test]
    fn registers_woff2() {
        // glyf and loca are rebuilt from the WOFF2 transform.
        assert_same_font(
            &fixture("RobotoMono-Regular.ttf"),
            fixture("woff/RobotoMono-Regular.woff2"),
        );
    }

    #[test]
    fn registers_cff_in_woff() {
        let otf = fixture("InterDisplay-Bold.otf");
        let web = woff::woff_from_sfnt(&otf, true, None);
        assert_same_font(&otf, web.clone());
        let mut s = FontStore::new();
        s.register("Bold", web).unwrap();
        assert!(s.get("Bold").unwrap().metrics.cff);
    }

    #[test]
    fn renders_woff2_deterministically() {
        let mut fonts = FontStore::new();
        fonts
            .register("Mono", fixture("woff/RobotoMono-Regular.woff2"))
            .unwrap();
        let doc = br#"{"version":1,"fonts":[{"id":"Mono","family":"Roboto Mono"}],
            "pages":[{"size":{"width":300,"height":200},"children":[
              {"type":"text","runs":[{"text":"WOFF2 fonts, decoded.","font":"Mono","size":14}]}]}]}"#;
        let images = crate::ImageStore::new();
        for subset in [true, false] {
            let opts = crate::RenderOptions::default().with_subset_fonts(subset);
            let render = || crate::render(doc, crate::InputFormat::Json, &opts, &fonts, &images);
            let pdf = render().unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            assert_eq!(pdf, render().unwrap());
        }
    }

    #[test]
    fn rejects_garbage() {
        let mut s = FontStore::new();
        let e = s.register("x", vec![0u8; 100]).unwrap_err();
        assert_eq!(e.code(), "FONT_ERROR");
        let mut woff2 = fixture("woff/RobotoMono-Regular.woff2");
        woff2.truncate(woff2.len() / 2);
        let e = s.register("x", woff2).unwrap_err();
        assert_eq!(e.code(), "FONT_ERROR");
        assert_eq!(
            e.to_string(),
            "font error: font `x`: not a valid WOFF2 font"
        );
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("My Font (Bold)"), "MyFontBold");
        assert_eq!(sanitize_name("   "), "Font");
    }
}
