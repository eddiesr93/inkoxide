//! Font registry, metrics, the per-render font set and the font programs
//! PDFs embed, whole or subset.
//!
//! Fonts are registered explicitly (no system font discovery: determinism).
//! Registration decodes WOFF and WOFF2 files to the font they wrap, then
//! parses and validates the font once; per render, the fonts a document
//! declares are parsed again for glyph lookups and advances (`FontSet`).
//!
//! The font-parsing library (ttf-parser) and the subsetter stay in this
//! module: the rest of the engine sees glyph ids, advances, [`FontMetrics`]
//! and font programs. Shaping faces belong to `text`, which builds them
//! from [`FontData`]'s bytes.

mod cff;
mod color;
mod layout;
mod woff;

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

pub(crate) use color::ColorGlyph;

use crate::schema::{Document, FontDecl, FontStyle};
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
    /// Height of lowercase letters (OS/2 `sxHeight`, else the top of `x`).
    pub x_height: f32,
    /// How far subscripts sit below the baseline (OS/2 `ySubscriptYOffset`).
    pub subscript_offset: f32,
    /// Em size of subscripts (OS/2 `ySubscriptYSize`).
    pub subscript_size: f32,
    /// How far superscripts sit above the baseline (OS/2 `ySuperscriptYOffset`).
    pub superscript_offset: f32,
    /// Em size of superscripts (OS/2 `ySuperscriptYSize`).
    pub superscript_size: f32,
    /// CFF outlines (embedded as `CIDFontType0C`) rather than TrueType.
    pub cff: bool,
    /// Colour glyphs the engine draws: `COLR` layers or PNG bitmaps
    /// (`CBDT` or `sbix`). See `color.rs`.
    pub color: bool,
    /// Colour glyphs and no outlines: the font is never embedded, and its
    /// glyphs without colour are not drawn.
    pub color_only: bool,
}

/// Defaults of the script metrics, in ems, for fonts without them: close
/// to what browsers use.
const SUBSCRIPT_OFFSET: f32 = 0.2;
const SUPERSCRIPT_OFFSET: f32 = 0.34;
const SCRIPT_SIZE: f32 = 0.65;

impl FontMetrics {
    /// How far a superscript's baseline is raised, as a fraction of the
    /// superscript's own size: the OS/2 offset over the OS/2 size.
    pub fn superscript_shift(&self) -> f32 {
        script_shift(
            self.superscript_offset,
            self.superscript_size,
            SUPERSCRIPT_OFFSET,
        )
    }

    /// How far a subscript's baseline is lowered, as a fraction of the
    /// subscript's own size.
    pub fn subscript_shift(&self) -> f32 {
        script_shift(self.subscript_offset, self.subscript_size, SUBSCRIPT_OFFSET)
    }
}

/// `offset` over `size`, or the defaults' ratio where that is out of
/// (0, 1] (a malformed or unusual OS/2 table).
fn script_shift(offset: f32, size: f32, default_offset: f32) -> f32 {
    Some(offset / size)
        .filter(|r| *r > 0.0 && *r <= 1.0)
        .unwrap_or(default_offset / SCRIPT_SIZE)
}

/// The OpenType tag of `bytes`, such as a feature's (`tnum`): the type
/// the shaper takes, built here so that only this module names ttf-parser.
pub(crate) fn opentype_tag(bytes: [u8; 4]) -> ttf_parser::Tag {
    ttf_parser::Tag::from_bytes(&bytes)
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
    /// Compressed whole font program (see [`FontData::compressed_program`]),
    /// computed on first use and shared by every later render.
    compressed: OnceLock<Arc<[u8]>>,
}

impl FontData {
    /// Whether the font has colour glyphs the engine draws: `COLR` layers
    /// or PNG bitmaps (`CBDT` or `sbix`).
    pub(crate) fn has_color(&self) -> bool {
        self.metrics.color
    }

    /// Whether the font has outlines. One without (colour bitmaps only) is
    /// never embedded, and its glyphs without colour are not drawn.
    pub(crate) fn has_outlines(&self) -> bool {
        !self.metrics.color_only
    }

    fn parse(id: &str, bytes: Arc<[u8]>) -> Result<Self, InkoxideError> {
        let err = |msg: &str| InkoxideError::Font(format!("font `{id}`: {msg}"));
        let bytes = match woff::decode(&bytes).map_err(|m| err(&m))? {
            Some(sfnt) => sfnt.into(),
            None => bytes,
        };
        // `Face::parse` would take the first face of a collection, and the
        // PDF needs the program of one font, not the whole file.
        if bytes.starts_with(b"ttcf") {
            return Err(err(
                "font collections (.ttc) are not supported; register each font as its own \
                 .ttf or .otf file",
            ));
        }
        let face = ttf_parser::Face::parse(&bytes, 0)
            .map_err(|e| err(&format!("not a valid TrueType/OpenType font ({e})")))?;
        let tables = face.tables();
        if face.is_variable() || tables.cff2.is_some() {
            return Err(err(
                "variable fonts are not supported; register a static instance",
            ));
        }
        // Parsed only with their companion tables (`CBLC`, `CPAL`).
        let bitmaps = tables.cbdt.is_some() || tables.sbix.is_some();
        let color = bitmaps || tables.colr.is_some();
        let color_only = tables.glyf.is_none() && tables.cff.is_none() && bitmaps;
        let cff = tables.glyf.is_none() && !color_only;
        if cff && cff::reals_fit(&bytes) == Some(false) {
            return Err(err(
                "the CFF table is malformed (a number in a DICT is out of range)",
            ));
        }
        if cff && (tables.cff.is_none() || cff::full_program(&bytes).is_none()) {
            return Err(err(
                "font has neither TrueType (glyf) nor embeddable CFF outlines, nor colour \
                 bitmaps (CBDT or sbix)",
            ));
        }
        if tables
            .cmap
            .is_none_or(|c| !c.subtables.into_iter().any(|s| s.is_unicode()))
        {
            return Err(err("font has no Unicode cmap"));
        }
        if let Some(table) = layout::unresolved_coverage(&face) {
            return Err(err(&format!(
                "the {table} table is malformed (a coverage offset does not point to a \
                 coverage table)"
            )));
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
        let x_height = face
            .x_height()
            .filter(|&h| h > 0)
            .map(f32::from)
            .or_else(|| {
                let x = face.glyph_index('x')?;
                face.glyph_bounding_box(x).map(|b| f32::from(b.y_max))
            })
            .filter(|&h| h > 0.0)
            .unwrap_or(upem * 0.5);
        // Fonts without an OS/2 table, or with sizes of 0, get values close
        // to what browsers use.
        let script = |m: Option<ttf_parser::ScriptMetrics>, offset: f32| {
            m.filter(|m| m.y_size > 0)
                .map_or((upem * offset, upem * SCRIPT_SIZE), |m| {
                    (f32::from(m.y_offset), f32::from(m.y_size))
                })
        };
        let (subscript_offset, subscript_size) = script(face.subscript_metrics(), SUBSCRIPT_OFFSET);
        let (superscript_offset, superscript_size) =
            script(face.superscript_metrics(), SUPERSCRIPT_OFFSET);
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
            x_height,
            subscript_offset,
            subscript_size,
            superscript_offset,
            superscript_size,
            cff,
            color,
            color_only,
        };
        Ok(Self {
            hash: xxhash_rust::xxh3::xxh3_64(&bytes),
            bytes,
            metrics,
            compressed: OnceLock::new(),
        })
    }

    /// The whole font program (`FontFile2`, or the CID-keyed CFF for
    /// `FontFile3`), passed through `compress` on first use; later calls,
    /// in this render or another, return that first result.
    pub(crate) fn compressed_program(
        &self,
        compress: impl FnOnce(&[u8]) -> Result<Vec<u8>, InkoxideError>,
    ) -> Result<Arc<[u8]>, InkoxideError> {
        if let Some(c) = self.compressed.get() {
            return Ok(c.clone());
        }
        let data: Arc<[u8]> = if self.metrics.cff {
            // Checked at registration.
            let program = cff::full_program(&self.bytes)
                .ok_or_else(|| InkoxideError::Internal("CFF conversion failed".into()))?;
            compress(&program)?.into()
        } else {
            compress(&self.bytes)?.into()
        };
        Ok(self.compressed.get_or_init(|| data).clone())
    }
}

/// A font program cut down to some glyphs, uncompressed. Its CIDs are
/// the original glyph ids.
pub(crate) struct Subset {
    /// The `FontFile2` program, or the CID-keyed CFF for `FontFile3`.
    pub program: Vec<u8>,
    /// TrueType: the `CIDToGIDMap`, the new glyph id of each original one
    /// up to the largest used (two bytes each, big-endian). `None` for CFF,
    /// whose charset maps the CIDs.
    pub cid_to_gid: Option<Vec<u8>>,
}

impl FontData {
    /// The program with only `glyphs` (sorted); `None` when the subsetter
    /// rejects the font.
    pub(crate) fn subset(&self, glyphs: &[u16]) -> Option<Subset> {
        if self.metrics.cff {
            return Some(Subset {
                program: cff::subset_program(&self.bytes, glyphs)?,
                cid_to_gid: None,
            });
        }
        let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(glyphs);
        let program = subsetter::subset(&self.bytes, 0, &remapper).ok()?;
        let max = glyphs.last().copied().unwrap_or(0);
        let mut map = vec![0u8; 2 * (usize::from(max) + 1)];
        for &g in glyphs {
            let new = remapper.get(g).unwrap_or(0);
            map[2 * usize::from(g)..][..2].copy_from_slice(&new.to_be_bytes());
        }
        Some(Subset {
            program,
            cid_to_gid: Some(map),
        })
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
    /// cannot be decoded, and for fonts that cannot be used:
    ///
    /// - font collections (`.ttc`): register each font on its own;
    /// - variable fonts (and CFF2 outlines): register a static instance;
    /// - fonts with neither TrueType (`glyf`) nor CFF outlines that can be
    ///   embedded, nor colour bitmaps (`CBDT` or `sbix`): a font with
    ///   colour bitmaps and no outlines is accepted, never embedded, and
    ///   draws only its colour glyphs;
    /// - fonts without a Unicode `cmap` (text is mapped to glyphs through
    ///   it);
    /// - fonts whose `GSUB` or `GPOS` lookups point to coverage tables
    ///   that are missing or malformed (they would fail shaping).
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

/// A registered font parsed for one render: its `cmap` and advances.
pub(crate) struct LoadedFont<'a> {
    /// The id the document declares it under.
    pub id: &'a str,
    pub data: &'a FontData,
    face: ttf_parser::Face<'a>,
}

impl LoadedFont<'_> {
    /// See [`FontData::has_color`].
    pub fn has_color(&self) -> bool {
        self.data.has_color()
    }

    /// See [`FontData::has_outlines`].
    pub fn has_outlines(&self) -> bool {
        self.data.has_outlines()
    }

    /// Scale from font units to points at `size`.
    pub fn scale(&self, size: f32) -> f32 {
        size / self.data.metrics.units_per_em
    }

    pub fn glyph(&self, c: char) -> Option<u16> {
        self.face.glyph_index(c).map(|g| g.0)
    }

    /// Number of glyphs in the font: its glyph ids are `0..glyph_count()`.
    pub fn glyph_count(&self) -> u16 {
        self.face.number_of_glyphs()
    }

    /// Horizontal advance of glyph `gid` in font units.
    pub fn advance(&self, gid: u16) -> f32 {
        f32::from(
            self.face
                .glyph_hor_advance(ttf_parser::GlyphId(gid))
                .unwrap_or(0),
        )
    }

    /// The box of glyph `gid`'s outline, `[x_min, y_min, x_max, y_max]` in
    /// font units with y up; `None` for a glyph with no outline (a space).
    pub fn glyph_bbox(&self, gid: u16) -> Option<[f32; 4]> {
        let b = self.face.glyph_bounding_box(ttf_parser::GlyphId(gid))?;
        Some([
            f32::from(b.x_min),
            f32::from(b.y_min),
            f32::from(b.x_max),
            f32::from(b.y_max),
        ])
    }
}

/// CSS font matching (CSS Fonts 4 §5.2) over `faces`, the family, weight
/// and style of each face in order: the first family of `families` that
/// has a face (else the first face's family), then the faces of `style`
/// (else every face of the family), then the nearest weight. Family names
/// match ignoring ASCII case, as in CSS. The same algorithm as `FontRegistry` in
/// `@inkoxide/react`; `fixtures/fonts/matching.json` holds the cases both
/// are tested with. Returns the index of the face.
fn match_face(
    faces: &[(&str, u16, FontStyle)],
    families: &[&str],
    weight: u16,
    style: FontStyle,
) -> Option<usize> {
    let first = faces.first()?;
    let family = families
        .iter()
        .find(|f| faces.iter().any(|x| x.0.eq_ignore_ascii_case(f)))
        .map_or(first.0, |f| *f);
    let in_family = || (0..faces.len()).filter(|&i| faces[i].0.eq_ignore_ascii_case(family));
    let same_style: Vec<usize> = in_family().filter(|&i| faces[i].2 == style).collect();
    let pool = if same_style.is_empty() {
        in_family().collect()
    } else {
        same_style
    };
    nearest_weight(&pool, |i| faces[i].1, weight)
}

/// CSS Fonts 4 §5.2 weight matching: the face of `pool` with weight `w`,
/// else, nearest first, for 400 to 500 the heavier ones up to 500, then the
/// lighter ones, then the heavier ones; below 400 the lighter, then the
/// heavier; above 500 the heavier, then the lighter. Of equal weights, the
/// first in `pool` wins when exact or heavier, and the last when lighter,
/// as in `FontRegistry`.
fn nearest_weight(pool: &[usize], weight_of: impl Fn(usize) -> u16, w: u16) -> Option<usize> {
    if let Some(&exact) = pool.iter().find(|&&i| weight_of(i) == w) {
        return Some(exact);
    }
    let mut asc = pool.to_vec();
    asc.sort_by_key(|&i| weight_of(i));
    let mut above = asc.iter().copied().filter(|&i| weight_of(i) > w);
    let mut below = asc.iter().copied().rev().filter(|&i| weight_of(i) < w);
    if (400..=500).contains(&w) {
        let up_to_500 = asc
            .iter()
            .copied()
            .find(|&i| weight_of(i) > w && weight_of(i) <= 500);
        up_to_500.or_else(|| below.next()).or_else(|| above.next())
    } else if w < 400 {
        below.next().or_else(|| above.next())
    } else {
        above.next().or_else(|| below.next())
    }
}

/// The fonts of one document, parsed once per render.
pub(crate) struct FontSet<'a> {
    pub fonts: Vec<LoadedFont<'a>>,
    by_id: BTreeMap<&'a str, FontKey>,
    pub fallbacks: Vec<FontKey>,
    /// The declarations, by key: family, weight and style for `select`.
    decls: &'a [FontDecl],
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
            let face = ttf_parser::Face::parse(&data.bytes, 0)
                .map_err(|_| InkoxideError::Font(format!("font `{}` cannot be loaded", decl.id)))?;
            by_id.insert(decl.id.as_str(), fonts.len() as FontKey);
            fonts.push(LoadedFont {
                id: &decl.id,
                data,
                face,
            });
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
            decls: &doc.fonts,
        })
    }

    pub fn key(&self, id: &str) -> Option<FontKey> {
        self.by_id.get(id).copied()
    }

    /// The declared font CSS matching picks for `families` (in order of
    /// preference), `weight` and `style`, by the family, weight and style
    /// the document declares for each font; `None` when it declares none.
    pub fn select(&self, families: &[&str], weight: u16, style: FontStyle) -> Option<FontKey> {
        let faces: Vec<_> = self
            .decls
            .iter()
            .map(|d| (d.family.as_str(), d.weight, d.style))
            .collect();
        FontKey::try_from(match_face(&faces, families, weight, style)?).ok()
    }

    pub fn get(&self, key: FontKey) -> &LoadedFont<'a> {
        &self.fonts[key as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        crate::test_fixtures::fixture(&format!("fonts/{name}"))
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
        let bold = s.get("Bold").unwrap();
        assert!(bold.metrics.cff);
        assert_eq!(bold.metrics.weight, 700);
        let program = bold.compressed_program(|p| Ok(p.to_vec())).unwrap();
        assert!(!program.is_empty());
    }

    #[test]
    fn registers_colour_fonts_with_and_without_outlines() {
        let mut s = FontStore::new();
        for (id, file, color_only) in [
            ("colr", "EmojiTest-COLR.ttf", false),
            ("cbdt", "EmojiTest-CBDT.ttf", true),
            ("sbix", "EmojiTest-sbix.ttf", true),
        ] {
            s.register(id, fixture(file)).unwrap();
            let m = &s.get(id).unwrap().metrics;
            assert!(m.color, "{id}");
            assert_eq!(m.color_only, color_only, "{id}");
            assert!(!m.cff, "{id}");
        }
        s.register("inter", fixture("Inter-Regular.ttf")).unwrap();
        assert!(!s.get("inter").unwrap().metrics.color);
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
        assert!(!b.compressed_program(|p| Ok(p.to_vec())).unwrap().is_empty());
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

    /// `font` wrapped in a TrueType collection of one face.
    fn collection(font: &[u8]) -> Vec<u8> {
        const HEADER: u32 = 16;
        let mut out = b"ttcf".to_vec();
        out.extend([0, 1, 0, 0]);
        out.extend(1u32.to_be_bytes());
        out.extend(HEADER.to_be_bytes());
        let mut face = font.to_vec();
        // Table offsets count from the start of the file.
        let tables = u16::from_be_bytes([face[4], face[5]]) as usize;
        for i in 0..tables {
            let at = 12 + 16 * i + 8;
            let offset = u32::from_be_bytes(face[at..at + 4].try_into().unwrap());
            face[at..at + 4].copy_from_slice(&(offset + HEADER).to_be_bytes());
        }
        out.extend(face);
        out
    }

    #[test]
    fn rejects_font_collections() {
        let inter = fixture("Inter-Regular.ttf");
        let ttc = collection(&inter);
        assert!(ttf_parser::Face::parse(&ttc, 0).is_ok());
        let e = FontStore::new().register("x", ttc).unwrap_err();
        assert_eq!(e.code(), "FONT_ERROR");
        assert!(e.to_string().contains("font collections (.ttc)"), "{e}");
    }

    /// A fuzz finding, reduced: a CFF font whose Top DICT `FontMatrix`
    /// holds a real that overflows `f32`. Embedding it panicked subsetter
    /// 0.2.6, which writes the real back as `inf`.
    #[test]
    fn rejects_cff_numbers_out_of_range() {
        let matrix_real = [0x1e, 0xa0, 0x00, 0x48, 0x82, 0x8f]; // .00048828
        let font = fixture("InterDisplay-Bold.otf");
        let at = font
            .windows(matrix_real.len())
            .position(|w| w == matrix_real)
            .unwrap();
        for (nibbles, ok) in [
            ([0x1b, 0x99, 0x99, 0x99, 0x9f], false), // 1E9999999: inf
            ([0x1b, 0x00, 0x00, 0x03, 0x1f], false), // 1E31, above 1E30
            ([0x1b, 0x00, 0x00, 0x00, 0x3f], true),  // 1E3
        ] {
            let mut bad = font.clone();
            bad[at + 1..at + 6].copy_from_slice(&nibbles);
            let r = FontStore::new().register("x", bad);
            if ok {
                r.unwrap();
            } else {
                let e = r.unwrap_err();
                assert_eq!(e.code(), "FONT_ERROR");
                assert!(e.to_string().contains("CFF table is malformed"), "{e}");
            }
        }
    }

    /// A fuzz finding, minimised: Noto Sans Hebrew with the lookahead
    /// coverage of a chained context lookup pointing to bytes that are not
    /// a coverage table. Shaping an `i` before a soft hyphen with it panics
    /// rustybuzz 0.20.1.
    const BAD_COVERAGE: &str = "malformed/gsub-lookahead-coverage.ttf";

    #[test]
    fn rejects_coverage_offsets_that_do_not_resolve() {
        let e = FontStore::new()
            .register("x", fixture(BAD_COVERAGE))
            .unwrap_err();
        assert_eq!(e.code(), "FONT_ERROR");
        assert_eq!(
            e.to_string(),
            "font error: font `x`: the GSUB table is malformed (a coverage offset does not \
             point to a coverage table)"
        );

        // Noto Sans Hebrew has one chained context format 3 lookup with a
        // lookahead in each table; a null offset there is rejected too.
        let hebrew = fixture("NotoSansHebrew-Regular.ttf");
        assert!(FontStore::new().register("x", hebrew.clone()).is_ok());
        for (table, lookup) in [(b"GSUB", 3), (b"GPOS", 0)] {
            let mut font = hebrew.clone();
            let at = chain_format3(&font, table, lookup);
            let back = usize::from(u16::from_be_bytes([font[at + 2], font[at + 3]]));
            let input_at = at + 4 + 2 * back;
            let input = usize::from(u16::from_be_bytes([font[input_at], font[input_at + 1]]));
            let ahead_at = input_at + 2 + 2 * input;
            assert_eq!(u16::from_be_bytes([font[ahead_at], font[ahead_at + 1]]), 1);
            font[ahead_at + 2..ahead_at + 4].fill(0);
            let e = FontStore::new().register("x", font).unwrap_err();
            let table = std::str::from_utf8(table).unwrap();
            assert!(e.to_string().contains(&format!("the {table} table")), "{e}");
        }
    }

    /// The offset in `font` of the first subtable of lookup `lookup` in
    /// GSUB or GPOS, which must be a chained context format 3 subtable.
    fn chain_format3(font: &[u8], table: &[u8; 4], lookup: usize) -> usize {
        let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
        let data = raw.table(ttf_parser::Tag::from_bytes(table)).unwrap();
        let start = data.as_ptr() as usize - font.as_ptr() as usize;
        let u16_at = |at: usize| usize::from(u16::from_be_bytes([font[at], font[at + 1]]));
        let lookups = start + u16_at(start + 8);
        let at = lookups + u16_at(lookups + 2 + 2 * lookup);
        let subtable = at + u16_at(at + 6);
        assert_eq!(u16_at(subtable), 3, "not format 3");
        subtable
    }

    /// A font the shaper panics on that registration lets through (here by
    /// skipping it): the render fails with a font error naming it, on one
    /// thread and when paragraphs are shaped in parallel.
    #[test]
    fn a_panic_while_shaping_is_a_font_error() {
        let hebrew = FontData::parse("bad", fixture("NotoSansHebrew-Regular.ttf").into()).unwrap();
        let bytes: Arc<[u8]> = fixture(BAD_COVERAGE).into();
        let mut fonts = FontStore::new();
        fonts.fonts.insert(
            "bad".into(),
            Arc::new(FontData {
                hash: xxhash_rust::xxh3::xxh3_64(&bytes),
                bytes,
                ..hebrew
            }),
        );
        // An `i` before a soft hyphen reaches the broken lookup; enough
        // paragraphs for `TextCache::prepare_all` to shape them in parallel.
        let para = r#"{"type":"text","runs":[{"text":"i\u00ad","font":"bad","size":12}]}"#;
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"bad","family":"Bad"}}],
              "pages":[{{"size":{{"width":300,"height":200}},"children":[{}]}}]}}"#,
            [para; 200].join(",")
        );
        let images = crate::ImageStore::new();
        for threads in [1, 4] {
            let opts = crate::RenderOptions::default().with_threads(threads);
            let e = crate::render(
                doc.as_bytes(),
                crate::InputFormat::Json,
                &opts,
                &fonts,
                &images,
            )
            .unwrap_err();
            assert_eq!(e.code(), "FONT_ERROR", "{e}");
            assert!(
                e.to_string().contains("font `bad`: the shaper panicked"),
                "{e}"
            );
        }
    }

    /// `fixtures/fonts/matching.json`, the cases `FontRegistry` in
    /// `@inkoxide/react` is tested with too.
    #[test]
    fn selects_fonts_like_the_react_registry() {
        let table: serde_json::Value = serde_json::from_slice(&fixture("matching.json")).unwrap();
        let inter = fixture("Inter-Regular.ttf");
        let mut store = FontStore::new();
        for faces in table["faces"].as_object().unwrap().values() {
            for face in faces.as_array().unwrap() {
                store
                    .register(face["id"].as_str().unwrap(), inter.clone())
                    .unwrap();
            }
        }
        let cases = table["cases"].as_array().unwrap();
        assert!(cases.len() > 20);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let faces = &table["faces"][case["faces"].as_str().unwrap()];
            let json = format!(
                r#"{{"version":1,"fonts":{faces},"pages":[{{"size":{{"width":10,"height":10}}}}]}}"#
            );
            let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
            let set = FontSet::load(&doc, &store).unwrap();
            let families: Vec<&str> = case["families"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap())
                .collect();
            let style = match case["style"].as_str().unwrap() {
                "italic" => FontStyle::Italic,
                _ => FontStyle::Normal,
            };
            let weight = u16::try_from(case["weight"].as_u64().unwrap()).unwrap();
            // `exact` is for the registry, which warns when a face falls
            // back.
            let key = set.select(&families, weight, style).unwrap();
            assert_eq!(set.get(key).id, case["id"].as_str().unwrap(), "{name}");
        }

        let doc = crate::validate::parse(
            br#"{"version":1,"pages":[{"size":{"width":10,"height":10}}]}"#,
            crate::InputFormat::Json,
        )
        .unwrap();
        let none = FontSet::load(&doc, &store).unwrap();
        assert_eq!(none.select(&["Inter"], 400, FontStyle::Normal), None);
    }

    #[test]
    fn reads_x_height_and_script_metrics() {
        let mut s = FontStore::new();
        s.register("Inter", fixture("Inter-Regular.ttf")).unwrap();
        s.register("Bold", fixture("InterDisplay-Bold.otf"))
            .unwrap();
        s.register("Mono", fixture("RobotoMono-Regular.ttf"))
            .unwrap();
        for id in ["Inter", "Bold", "Mono"] {
            let m = &s.get(id).unwrap().metrics;
            let em = m.units_per_em;
            assert!((0.4..0.6).contains(&(m.x_height / em)), "{id} {m:?}");
            assert!(m.x_height < m.cap_height, "{id}");
            for size in [m.subscript_size, m.superscript_size] {
                assert!((0.4..0.9).contains(&(size / em)), "{id} {m:?}");
            }
            assert!(
                (0.0..0.5).contains(&(m.subscript_offset / em)),
                "{id} {m:?}"
            );
            assert!(
                (0.2..0.6).contains(&(m.superscript_offset / em)),
                "{id} {m:?}"
            );
        }
        // Inter's OS/2 table.
        let inter = &s.get("Inter").unwrap().metrics;
        assert_eq!(
            [
                inter.x_height,
                inter.subscript_offset,
                inter.subscript_size,
                inter.superscript_offset,
                inter.superscript_size,
            ],
            INTER_OS2
        );
    }

    /// Inter Regular's `sxHeight`, `ySubscriptYOffset`, `ySubscriptYSize`,
    /// `ySuperscriptYOffset` and `ySuperscriptYSize`.
    const INTER_OS2: [f32; 5] = [1118.0, 154.0, 1229.0, 717.0, 1229.0];

    #[test]
    fn falls_back_without_an_os2_table() {
        let mut font = fixture("Inter-Regular.ttf");
        let tables = usize::from(u16::from_be_bytes([font[4], font[5]]));
        let at = (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&at| &font[at..at + 4] == b"OS/2")
            .unwrap();
        font[at..at + 4].copy_from_slice(b"OS/x");
        let mut s = FontStore::new();
        s.register("x", font).unwrap();
        let m = &s.get("x").unwrap().metrics;
        // The top of `x`, which Inter's sxHeight also gives.
        assert_eq!(m.x_height, INTER_OS2[0]);
        assert_eq!(
            [
                m.subscript_offset,
                m.subscript_size,
                m.superscript_offset,
                m.superscript_size
            ],
            [2048.0 * 0.2, 2048.0 * 0.65, 2048.0 * 0.34, 2048.0 * 0.65]
        );
        // In the script's own size, a script at 0.65 em sits as browsers
        // put one: raised 0.34 em, lowered 0.2 em of the text around it.
        assert!((m.superscript_shift() - 0.34 / 0.65).abs() < 1e-6);
        assert!((m.subscript_shift() - 0.2 / 0.65).abs() < 1e-6);
        assert!((m.superscript_shift() - 0.523).abs() < 1e-3);
        assert!((m.subscript_shift() - 0.308).abs() < 1e-3);
        // OS/2 sizes that make no ratio in (0, 1] fall back the same way.
        let odd = FontMetrics {
            superscript_size: 100.0,
            superscript_offset: 300.0,
            subscript_size: 100.0,
            subscript_offset: -5.0,
            ..m.clone()
        };
        assert_eq!(odd.superscript_shift(), m.superscript_shift());
        assert_eq!(odd.subscript_shift(), m.subscript_shift());
        // Inter's own: the OS/2 offset over the OS/2 size.
        let inter = fixture("Inter-Regular.ttf");
        s.register("Inter", inter).unwrap();
        let inter = &s.get("Inter").unwrap().metrics;
        assert_eq!(inter.superscript_shift(), 717.0 / 1229.0);
        assert_eq!(inter.subscript_shift(), 154.0 / 1229.0);
    }

    #[test]
    fn measures_glyph_boxes() {
        let mut s = FontStore::new();
        s.register("Inter", fixture("Inter-Regular.ttf")).unwrap();
        s.register("Bold", fixture("InterDisplay-Bold.otf"))
            .unwrap();
        let doc = crate::validate::parse(
            br#"{"version":1,"fonts":[{"id":"Inter","family":"Inter"},{"id":"Bold","family":"Inter Display","weight":700}],
                "pages":[{"size":{"width":10,"height":10}}]}"#,
            crate::InputFormat::Json,
        )
        .unwrap();
        let set = FontSet::load(&doc, &s).unwrap();
        for font in &set.fonts {
            let x = font.glyph('x').unwrap();
            let [x0, y0, x1, y1] = font.glyph_bbox(x).unwrap();
            assert!(x0 < x1 && y0 <= 0.0 && y0 > -10.0, "{}", font.id);
            assert!(
                (y1 - font.data.metrics.x_height).abs() < 0.02 * font.data.metrics.units_per_em
            );
            assert!(x1 <= font.advance(x) + 1.0, "{}", font.id);
            // A descender reaches below the baseline.
            assert!(font.glyph_bbox(font.glyph('p').unwrap()).unwrap()[1] < -100.0);
            assert_eq!(
                font.glyph_bbox(font.glyph(' ').unwrap()),
                None,
                "{}",
                font.id
            );
        }
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("My Font (Bold)"), "MyFontBold");
        assert_eq!(sanitize_name("   "), "Font");
    }
}
