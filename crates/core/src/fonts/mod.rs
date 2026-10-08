//! Font registry, metrics and the per-render font set.
//!
//! Fonts are registered explicitly (no system font discovery: determinism).
//! Registration parses and validates the font once; per render, the fonts a
//! document declares are wrapped in shaping faces (`FontSet`).

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
    pub num_glyphs: u16,
    /// CFF outlines (embedded as `CIDFontType0C`) rather than TrueType.
    pub cff: bool,
}

/// A registered font face.
#[derive(Debug)]
pub struct FontData {
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
            num_glyphs: face.number_of_glyphs(),
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
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) font `id` from TrueType or CFF OpenType bytes.
    /// Fails with a clear error for unsupported fonts (variable fonts).
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

    pub fn contains(&self, id: &str) -> bool {
        self.fonts.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

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

    #[test]
    fn rejects_garbage() {
        let mut s = FontStore::new();
        let e = s.register("x", vec![0u8; 100]).unwrap_err();
        assert_eq!(e.code(), "FONT_ERROR");
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("My Font (Bold)"), "MyFontBold");
        assert_eq!(sanitize_name("   "), "Font");
    }
}
