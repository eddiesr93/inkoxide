//! Display list → PDF bytes, through `pdf-writer`.

pub(crate) mod crypt;
mod font;
mod forms;
mod image;
mod notes;
mod out;
mod pdfa;
mod svg;

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, LineCapStyle, PageLayout, PageMode,
    TextRenderingMode,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, Str};
use rayon::prelude::*;

use crate::date::Timestamp;
use crate::fonts::{FontKey, FontSet};
use crate::images::{ImageKey, ImageSet};
use crate::paint::{apply, FillRule, InvisibleGlyph, Op, PageContent, PathSeg, Rect, TextOp};
use crate::pipeline::{Dest, Marks};
use crate::schema::{self, Color, Meta};
use crate::text::UnicodeMap;
use crate::InkoxideError;

pub(crate) use image::transparency as image_transparency;
use out::Out;
pub(crate) use pdfa::Profile;

/// Maximum kids per page-tree node (keeps viewers fast on long documents).
const PAGE_TREE_FANOUT: usize = 50;

/// A glyph closer than this (points) to where the previous glyph's advance
/// puts it needs no `TJ` adjustment.
const KERN_EPS: f32 = 1e-3;

/// A painted page reduced to what the writer needs: its compressed
/// content stream and the resources it names. Pages are encoded as soon as
/// they are painted (in parallel), so the display lists never pile up.
pub(crate) struct EncodedPage {
    pub width: f32,
    pub height: f32,
    /// Content stream, Flate-compressed when `compressed`.
    pub stream: Vec<u8>,
    pub compressed: bool,
    /// Glyphs used per font.
    pub glyphs: BTreeMap<FontKey, BTreeSet<u16>>,
    /// Opacity keys (see `alpha_key`).
    pub opacities: BTreeSet<u16>,
    pub images: BTreeSet<ImageKey>,
    /// Link areas (page coordinates, top-left origin) and their targets.
    pub links: Vec<(Rect, Box<str>)>,
    /// Form field widgets.
    pub fields: Vec<crate::forms::FieldOp>,
    /// Icon annotations (text notes), at the corner the transform in
    /// effect puts them.
    pub icons: Vec<crate::icons::IconOp>,
}

pub(crate) struct DocumentOut<'a> {
    pub pages: &'a [EncodedPage],
    pub meta: &'a Meta,
    pub creation_date: Option<Timestamp>,
    pub fonts: &'a FontSet<'a>,
    pub unicode: &'a UnicodeMap,
    pub images: &'a ImageSet,
    pub marks: &'a Marks<'a>,
}

/// Dependency firewall around the PDF writer.
pub(crate) trait PdfSink {
    /// Encodes one page; called from worker threads.
    fn encode_page(
        &self,
        page: &PageContent,
        fonts: &FontSet<'_>,
    ) -> Result<EncodedPage, InkoxideError>;
    fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError>;
}

pub(crate) struct PdfWriterSink {
    /// Flate level for content streams; `None` writes them uncompressed.
    pub compression: Option<u32>,
    /// Embed only the glyphs used.
    pub subset_fonts: bool,
    /// Plain PDF or PDF/A.
    pub profile: Profile,
    /// Encrypt the file with these passwords and permissions.
    pub encryption: Option<crypt::Params>,
    /// Prepare font programs on the current rayon pool; otherwise on the
    /// calling thread.
    pub parallel: bool,
}

/// Largest magnitude of a number the writer emits (see [`number`]).
///
/// PDF readers parse reals up to about ±3.4e38 but integers only within 32
/// bits (PDF 1.7, Annex C), and `pdf-writer` writes a value of 1e12 or more
/// as digits without a decimal point: an integer far past 2^31 − 1, which
/// readers reject or read as null (qpdf: "overflow/underflow converting
/// … to 64-bit integer"). Below 1e12 it writes a real, or an integer when
/// the value is one within 32 bits. 1e11 stays clear of that edge and
/// above anything a document draws: lengths are at most 1e6 pt, and the
/// largest scale an SVG within the image limits needs is 1e10 (its
/// largest size over its smallest `viewBox`). 1e11 pt is some 35,000 km,
/// so clamping there only moves what had already overflowed any page.
pub(crate) const MAX_NUMBER: f32 = 1e11;

/// `v` as a number every PDF reader parses: 0 when it is infinite or NaN,
/// and within ±[`MAX_NUMBER`]. The backstop for every number this module
/// writes; numbers that cannot exceed it by construction (colours,
/// opacities, font metrics in font units over at least 16 units per em,
/// gradient stops and periods) are written as they are.
///
/// PDF has no syntax for infinity or NaN; `pdf-writer` would write `inf` or
/// `NaN`, which makes the content stream invalid, and viewers reject the
/// page. Validation bounds every number in a document, but bounded values
/// can still compound past `f32` (a percentage of a percentage, a few
/// dozen levels deep, or one node's scales multiplied), and SVG images
/// bring numbers of their own (`scale(1e30)`). So this is not a
/// `debug_assert`: a valid document must not panic a debug build (the
/// fuzz targets and the debug Node binding run with assertions on). Zero
/// or the bound keeps the file valid; only the shape whose geometry
/// overflowed is misplaced. Other values pass unchanged, tiny ones too,
/// so output is byte-identical, and the check is a few compares.
#[inline]
pub(crate) fn number(v: f32) -> f32 {
    /// 2^31: `pdf-writer` writes ±2^31 as 32-bit integers (2^31 − 1 for
    /// 2^31), and in a debug build panics registering −2^31. The `f32`
    /// next to it, toward zero, is an integer within range.
    const I32_EDGE: f32 = 2_147_483_648.0;
    if !v.is_finite() {
        0.0
    } else if v.abs() == I32_EDGE {
        v.signum() * 2_147_483_520.0
    } else {
        v.clamp(-MAX_NUMBER, MAX_NUMBER)
    }
}

/// Rounds to 1/1000 pt so output has at most three decimals; bounded and
/// never infinite or NaN (see [`number`]).
#[inline]
pub(crate) fn q(v: f32) -> f32 {
    round_to(v, 1000.0)
}

/// Rounds to `1 / per`, through [`number`]. Past about 3e35 points the
/// scaling overflows, but `f32` has no fraction left to round there.
#[inline]
fn round_to(v: f32, per: f32) -> f32 {
    let r = (v * per).round() / per;
    number(if r.is_finite() { r } else { v })
}

/// The bytes of content stream `c`, whose numbers went through
/// [`number`] (checked in debug builds), within the limits of `profile`.
pub(crate) fn content_bytes(c: Content, profile: Profile) -> Result<Vec<u8>, InkoxideError> {
    let buf = c.finish();
    debug_assert!(
        buf.limits().real() <= MAX_NUMBER,
        "a number past MAX_NUMBER: {}",
        buf.limits().real()
    );
    profile.check_limits(buf.limits())?;
    let data = buf.into_vec();
    profile.check_nesting(&data)?;
    Ok(data)
}

/// Hands out object numbers in order, from 1.
struct RefAlloc(i32);

impl RefAlloc {
    fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

impl PdfSink for PdfWriterSink {
    fn encode_page(
        &self,
        page: &PageContent,
        fonts: &FontSet<'_>,
    ) -> Result<EncodedPage, InkoxideError> {
        let mut glyphs: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
        let mut opacities = BTreeSet::new();
        let mut images = BTreeSet::new();
        let mut links = Vec::new();
        let mut fields = Vec::new();
        let mut icons = Vec::new();
        // Links and widgets are areas on the page: they cover where the
        // transform in effect puts them.
        let mut ctm: Option<[f32; 6]> = None;
        let mut saved = Vec::new();
        for op in &page.ops {
            match op {
                Op::Save => saved.push(ctm),
                Op::Restore => ctm = saved.pop().flatten(),
                Op::Transform(m) => ctm = Some(ctm.map_or(*m, |c| crate::paint::mul(c, *m))),
                // A font without outlines is never embedded (see
                // `content_stream`).
                Op::Text(t) if fonts.get(t.font).data.metrics.color_only => {}
                Op::Text(t) => {
                    glyphs
                        .entry(t.font)
                        .or_default()
                        .extend(t.glyphs.iter().map(|g| g.id));
                }
                Op::SetOpacity(a) => {
                    opacities.insert(alpha_key(*a));
                }
                Op::Image { key, .. } => {
                    images.insert(*key);
                }
                Op::InvisibleText(g) => {
                    glyphs.entry(g.font).or_default().insert(g.glyph);
                }
                Op::Link { rect, target } => {
                    links.push((ctm.map_or(*rect, |m| bounds(m, *rect)), target.clone()));
                }
                Op::Field(f) => {
                    if let Some(font) = f.font {
                        let used = glyphs.entry(font).or_default();
                        for t in &f.text {
                            used.extend(t.glyphs.iter().map(|g| g.id));
                        }
                        used.extend(&f.option_glyphs);
                    }
                    let mut f = (**f).clone();
                    f.ctm = ctm;
                    fields.push(f);
                }
                // An icon stays upright and the same size: only its corner
                // moves.
                Op::Icon(i) => {
                    let mut i = (**i).clone();
                    if let Some(m) = ctm {
                        (i.x, i.y) = crate::paint::apply(m, (i.x, i.y));
                    }
                    icons.push(i);
                }
                _ => {}
            }
        }
        let raw = content_stream(page, fonts, self.profile)?;
        let stream = match self.compression {
            Some(level) => deflate(&raw, level)?,
            None => raw,
        };
        Ok(EncodedPage {
            width: page.width,
            height: page.height,
            stream,
            compressed: self.compression.is_some(),
            glyphs,
            opacities,
            images,
            links,
            fields,
            icons,
        })
    }

    fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError> {
        let mut pdf = Pdf::new();
        if let Some((major, minor)) = self.profile.version() {
            pdf.set_version(major, minor);
        }
        let mut alloc = RefAlloc(0);
        let (mut out, encrypt) = match &self.encryption {
            Some(params) => {
                let d = crypt::digest(doc.pages.iter().map(|p| p.stream.as_slice()), doc.meta);
                let (crypt, dict) = crypt::Crypt::new(params, &d);
                (Out::encrypted(crypt), Some(dict))
            }
            None => (Out::plain(), None),
        };
        let catalog = alloc.next();
        let info = alloc.next();

        // Page and content stream refs first, in document order, so
        // numbering is stable.
        let (page_refs, content_refs): (Vec<Ref>, Vec<Ref>) = doc
            .pages
            .iter()
            .map(|_| (alloc.next(), alloc.next()))
            .unzip();
        let tree_root = write_page_tree(&mut pdf, &mut alloc, &page_refs);

        // Shared graphics states, one per distinct opacity, in value order.
        let mut opacities: BTreeMap<u16, Ref> = BTreeMap::new();
        for page in doc.pages {
            for &key in &page.opacities {
                opacities.entry(key).or_insert(Ref::new(1));
            }
        }
        if opacities.keys().any(|&key| key < alpha_key(1.0)) {
            self.profile
                .allow_transparency("an opacity or a colour's alpha")?;
        }
        for (key, r) in &mut opacities {
            *r = alloc.next();
            let a = key_alpha(*key);
            pdf.ext_graphics(*r).non_stroking_alpha(a).stroking_alpha(a);
        }

        // Fonts: embed each font that is actually painted, with the glyphs
        // used across the whole document.
        let mut used: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
        let mut editable = BTreeSet::new();
        for page in doc.pages {
            for (key, glyphs) in &page.glyphs {
                used.entry(*key).or_default().extend(glyphs);
            }
            editable.extend(
                page.fields
                    .iter()
                    .filter(|f| f.editable)
                    .filter_map(|f| f.font),
            );
        }
        for key in editable {
            let font = doc.fonts.get(key);
            used.entry(key)
                .or_default()
                .extend(crate::forms::edit_glyphs(font));
        }
        // The glyphs of SVG text laid out for the images painted: every
        // text, drawn or not, so a superset of what the forms show.
        let painted: BTreeSet<ImageKey> =
            doc.pages.iter().flat_map(|p| &p.images).copied().collect();
        for &key in &painted {
            if let crate::images::ImageBody::Svg {
                probed: Some(probed),
                ..
            } = &doc.images.get(key).body
            {
                for g in probed.texts.iter().flat_map(|t| &t.glyphs) {
                    used.entry(g.font).or_default().insert(g.id);
                }
            }
        }
        // Subsetting and compression are independent per font.
        let program = |(key, glyphs): (&FontKey, &BTreeSet<u16>)| {
            font::program(doc.fonts.get(*key), glyphs, self.subset_fonts)
        };
        let programs: Vec<font::Program> = if self.parallel {
            used.par_iter().map(program).collect::<Result<_, _>>()?
        } else {
            used.iter().map(program).collect::<Result<_, _>>()?
        };
        let mut font_refs: BTreeMap<FontKey, Ref> = BTreeMap::new();
        for ((key, glyphs), program) in used.iter().zip(&programs) {
            let embedded = font::Embedded {
                font: doc.fonts.get(*key),
                program,
                used: glyphs,
                unicode: doc.unicode.fonts.get(key),
            };
            let r = font::write_font(&mut pdf, &mut alloc, &mut out, &embedded, self.profile)?;
            font_refs.insert(*key, r);
        }

        // Images: one XObject per unique image actually painted.
        let mut image_refs: BTreeMap<ImageKey, Ref> = BTreeMap::new();
        for page in doc.pages {
            for &key in &page.images {
                image_refs.entry(key).or_insert(Ref::new(1));
            }
        }
        for (key, r) in &mut image_refs {
            let fonts = svg::TextFonts {
                fonts: doc.fonts,
                refs: &font_refs,
            };
            *r = image::write_image(
                &mut pdf,
                &mut alloc,
                &mut out,
                doc.images.get(*key),
                self.profile,
                Some(fonts),
            )?;
        }

        // Link annotations, numbered in page order.
        let mut annots: Vec<Vec<Ref>> = Vec::with_capacity(doc.pages.len());
        for page in doc.pages {
            let mut refs = Vec::new();
            for link in &page.links {
                let r = alloc.next();
                if write_link(
                    &mut pdf,
                    &mut out,
                    r,
                    page.height,
                    link,
                    (doc.marks, &page_refs),
                    self.profile,
                ) {
                    refs.push(r);
                }
            }
            annots.push(refs);
        }
        notes::write_icons(
            &mut pdf,
            &mut alloc,
            &mut out,
            (doc.pages, &page_refs),
            &mut annots,
            self.profile,
        )?;
        let fields = forms::Ctx {
            pages: doc.pages,
            page_refs: &page_refs,
            font_refs: &font_refs,
            fonts: doc.fonts,
            profile: self.profile,
        };
        let form = forms::write_fields(&mut pdf, &mut alloc, &mut out, &fields, &mut annots)?;
        let outline = write_outline(
            &mut pdf,
            &mut alloc,
            &mut out,
            &doc.marks.outline,
            &page_refs,
        );

        for (i, ((page, page_ref), content_ref)) in doc
            .pages
            .iter()
            .zip(&page_refs)
            .zip(&content_refs)
            .enumerate()
        {
            let parent = tree_root.leaf_parents[i / PAGE_TREE_FANOUT];
            let mut p = pdf.page(*page_ref);
            p.parent(parent)
                .media_box(PdfRect::new(0.0, 0.0, q(page.width), q(page.height)))
                .contents(*content_ref);
            let mut res = p.resources();
            if !page.opacities.is_empty() {
                let mut gs = res.ext_g_states();
                for key in &page.opacities {
                    gs.pair(Name(gs_name(*key).as_bytes()), opacities[key]);
                }
            }
            if !page.glyphs.is_empty() {
                let mut f = res.fonts();
                for key in page.glyphs.keys() {
                    f.pair(Name(font_name(*key).as_bytes()), font_refs[key]);
                }
            }
            if !page.images.is_empty() {
                let mut x = res.x_objects();
                for key in &page.images {
                    x.pair(Name(image_name(*key).as_bytes()), image_refs[key]);
                }
            }
            res.finish();
            if !annots[i].is_empty() {
                p.annotations(annots[i].iter().copied());
            }
            p.finish();

            out.stream(&mut pdf, *content_ref, &page.stream, |stream| {
                if page.compressed {
                    stream.filter(Filter::FlateDecode);
                }
            });
        }

        let lang = doc.meta.lang.as_deref().map(|l| out.text(l));
        let mut cat = pdf.catalog(catalog);
        cat.pages(tree_root.root);
        if let Some(form) = &form {
            let mut acro = cat.form();
            acro.fields(form.fields.iter().copied());
            if !form.fonts.is_empty() {
                let mut dr = acro.default_resources();
                let mut f = dr.fonts();
                for key in &form.fonts {
                    f.pair(Name(font_name(*key).as_bytes()), font_refs[key]);
                }
            }
            acro.finish();
        }
        if let Some(root) = outline {
            cat.outlines(root);
        }
        // By default a document with bookmarks opens with them shown.
        let mode = match doc.meta.page_mode {
            None if outline.is_some() => Some(PageMode::UseOutlines),
            None => None,
            Some(schema::PageMode::UseNone) => Some(PageMode::UseNone),
            Some(schema::PageMode::UseOutlines) => Some(PageMode::UseOutlines),
            Some(schema::PageMode::UseThumbs) => Some(PageMode::UseThumbs),
            Some(schema::PageMode::FullScreen) => Some(PageMode::FullScreen),
        };
        if let Some(mode) = mode {
            cat.page_mode(mode);
        }
        if let Some(layout) = doc.meta.page_layout {
            cat.page_layout(match layout {
                schema::PageLayout::SinglePage => PageLayout::SinglePage,
                schema::PageLayout::OneColumn => PageLayout::OneColumn,
                schema::PageLayout::TwoColumnLeft => PageLayout::TwoColumnLeft,
                schema::PageLayout::TwoColumnRight => PageLayout::TwoColumnRight,
                schema::PageLayout::TwoPageLeft => PageLayout::TwoPageLeft,
                schema::PageLayout::TwoPageRight => PageLayout::TwoPageRight,
            });
        }
        if let Some(lang) = &lang {
            cat.pair(Name(b"Lang"), Str(lang));
        }
        if encrypt.is_some() {
            // Revision 6 of the security handler is extension level 8 of
            // PDF 1.7.
            cat.insert(Name(b"Extensions"))
                .dict()
                .insert(Name(b"ADBE"))
                .dict()
                .pair(Name(b"BaseVersion"), Name(b"1.7"))
                .pair(Name(b"ExtensionLevel"), 8);
        }
        let producer = doc
            .meta
            .producer
            .as_deref()
            .unwrap_or(concat!("Inkoxide ", env!("CARGO_PKG_VERSION")));
        if self.profile.is_pdf_a() {
            let metadata = alloc.next();
            let profile = alloc.next();
            cat.metadata(metadata);
            let (condition, info) = (out.text("sRGB"), out.text("sRGB IEC61966-2.1"));
            // `output_intents()`, with strings from `out`.
            cat.insert(Name(b"OutputIntents"))
                .array()
                .push()
                .dict()
                .pair(Name(b"Type"), Name(b"OutputIntent"))
                .pair(Name(b"S"), Name(b"GTS_PDFA1"))
                .pair(Name(b"OutputConditionIdentifier"), Str(&condition))
                .pair(Name(b"Info"), Str(&info))
                .pair(Name(b"DestOutputProfile"), profile);
            cat.finish();
            // XMP must stay uncompressed (ISO 19005-2, 6.6.2.1).
            let xmp = pdfa::xmp(doc.meta, producer, doc.creation_date, self.profile);
            out.metadata(&mut pdf, metadata, &xmp, |_| {});
            let icc = deflate(&pdfa::srgb_icc(), 9)?;
            out.icc_profile(&mut pdf, profile, &icc, |p| {
                p.n(3)
                    .range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
                    .filter(Filter::FlateDecode);
            });
            // Deterministic: a hash of everything that makes the document.
            let mut h = xxhash_rust::xxh3::Xxh3::new();
            for page in doc.pages {
                h.update(&page.stream);
            }
            h.update(&xmp);
            let id = h.digest128().to_be_bytes().to_vec();
            pdf.set_file_id((id.clone(), id));
        } else {
            cat.finish();
        }

        // PDF/A: the same cleaned values as the XMP packet.
        let text = |v: &str| {
            if self.profile.is_pdf_a() {
                pdfa::clean(v)
            } else {
                v.to_string()
            }
        };
        let m = doc.meta;
        let mut entries: Vec<(&[u8], Vec<u8>)> = Vec::new();
        for (key, value) in [
            (&b"Title"[..], &m.title),
            (b"Author", &m.author),
            (b"Subject", &m.subject),
            (b"Keywords", &m.keywords),
            (b"Creator", &m.creator),
        ] {
            if let Some(v) = value {
                entries.push((key, out.text(&text(v))));
            }
        }
        entries.push((b"Producer", out.text(&text(producer))));
        if let Some(t) = doc.creation_date {
            entries.push((b"CreationDate", out.bytes(pdf_date(t).as_bytes())));
        }
        let mut inf = pdf.document_info(info);
        for (key, value) in &entries {
            inf.pair(Name(key), Str(value));
        }
        inf.finish();

        // Content streams are checked as they are finished; this covers
        // the objects (matrices, boxes, rectangles, coordinates).
        debug_assert!(
            pdf.limits().real() <= MAX_NUMBER,
            "a number past MAX_NUMBER: {}",
            pdf.limits().real()
        );
        self.profile.check_limits(pdf.limits())?;
        let Some(dict) = encrypt else {
            return Ok(pdf.finish());
        };
        // Last, so every other object keeps its number.
        let r = alloc.next();
        dict.write(&mut pdf, r);
        pdf.set_file_id((dict.id.to_vec(), dict.id.to_vec()));
        crypt::splice_encrypt(pdf.finish(), r)
            .ok_or_else(|| InkoxideError::Pdf("the file has no trailer to encrypt".into()))
    }
}

/// The upright rectangle that `rect` covers once transformed by `m`.
fn bounds(m: [f32; 6], rect: Rect) -> Rect {
    let Rect { x, y, w, h } = rect;
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].map(|p| apply(m, p));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (px, py) in corners {
        (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Writes one link annotation as object `r`; `false` (nothing written)
/// when an anchor has no destination.
fn write_link(
    pdf: &mut Pdf,
    out: &mut Out,
    r: Ref,
    page_h: f32,
    (rect, target): &(Rect, Box<str>),
    (marks, pages): (&Marks<'_>, &[Ref]),
    profile: Profile,
) -> bool {
    let dest = match target.strip_prefix('#') {
        Some(id) => match marks.anchors.get(id) {
            Some(d) => Some(*d),
            None => return false,
        },
        None => None,
    };
    let uri = dest.is_none().then(|| out.bytes(&uri_bytes(target)));
    let mut a = pdf.annotation(r);
    a.subtype(AnnotationType::Link)
        .rect(annot_rect(*rect, page_h))
        .border(0.0, 0.0, 0.0, None);
    if profile.is_pdf_a() {
        a.flags(AnnotationFlags::PRINT);
    }
    let mut action = a.action();
    match dest {
        Some(d) => {
            action.action_type(ActionType::GoTo);
            xyz(action.destination(), d, pages);
        }
        None => {
            action
                .action_type(ActionType::Uri)
                .uri(Str(uri.as_deref().unwrap_or_default()));
        }
    }
    true
}

/// An annotation's `/Rect`: `r` (page coordinates, top-left origin) in
/// PDF user space, whose origin is the bottom left of a page `page_h` high.
fn annot_rect(r: Rect, page_h: f32) -> PdfRect {
    PdfRect::new(q(r.x), q(page_h - r.y - r.h), q(r.x + r.w), q(page_h - r.y))
}

/// `[page /XYZ left top 0]`: keeps the viewer's zoom.
fn xyz(d: pdf_writer::writers::Destination<'_>, dest: Dest, pages: &[Ref]) {
    d.page(pages[dest.page as usize])
        .xyz(q(dest.x), q(dest.y), None);
}

/// PDF URIs are 7-bit ASCII: percent-encode everything else (and spaces).
fn uri_bytes(uri: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(uri.len());
    for &b in uri.as_bytes() {
        if b.is_ascii_graphic() {
            out.push(b);
        } else {
            out.extend_from_slice(format!("%{b:02X}").as_bytes());
        }
    }
    out
}

/// Writes the document outline; returns its root. Each entry nests under
/// the nearest earlier entry of a lower level; all entries start open.
fn write_outline(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    out: &mut Out,
    entries: &[crate::pipeline::OutlineEntry<'_>],
    page_refs: &[Ref],
) -> Option<Ref> {
    if entries.is_empty() {
        return None;
    }
    let root = alloc.next();
    let refs: Vec<Ref> = entries.iter().map(|_| alloc.next()).collect();
    // parent[i]: index of the parent entry (None: the outline root).
    let n = entries.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut stack: Vec<usize> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        while stack.last().is_some_and(|&j| entries[j].level >= e.level) {
            stack.pop();
        }
        parent[i] = stack.last().copied();
        stack.push(i);
    }
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut top = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => kids[*p].push(i),
            None => top.push(i),
        }
    }
    // Descendants of each entry (entries are in pre-order).
    let mut count = vec![0i32; n];
    for i in (0..n).rev() {
        if let Some(p) = parent[i] {
            count[p] += count[i] + 1;
        }
    }
    let (mut prev, mut next) = (vec![None; n], vec![None; n]);
    for sib in kids.iter().chain([&top]) {
        for w in sib.windows(2) {
            next[w[0]] = Some(w[1]);
            prev[w[1]] = Some(w[0]);
        }
    }
    for (i, e) in entries.iter().enumerate() {
        let title = out.text(e.title);
        let mut item = pdf.outline_item(refs[i]);
        item.pair(Name(b"Title"), Str(&title));
        item.parent(parent[i].map_or(root, |p| refs[p]));
        if let Some(p) = prev[i] {
            item.prev(refs[p]);
        }
        if let Some(x) = next[i] {
            item.next(refs[x]);
        }
        if let (Some(&f), Some(&l)) = (kids[i].first(), kids[i].last()) {
            item.first(refs[f]).last(refs[l]).count(count[i]);
        }
        xyz(item.dest(), e.dest, page_refs);
    }
    let mut o = pdf.outline(root);
    if let (Some(&f), Some(&l)) = (top.first(), top.last()) {
        o.first(refs[f]).last(refs[l]);
    }
    o.count(n as i32);
    Some(root)
}

/// A PDF date string (`D:YYYYMMDDHHmmSS` and `Z` or `±HH'mm`). Written by
/// hand: `pdf_writer::Date` takes the offset's sign from its hours, so an
/// offset under an hour west of UTC (`-00:30`) would come out east of it.
fn pdf_date(t: Timestamp) -> String {
    let (d, off) = (t.local, t.offset_minutes);
    let zone = if off == 0 {
        "Z".to_string()
    } else {
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.unsigned_abs();
        format!("{sign}{:02}'{:02}", a / 60, a % 60)
    };
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}{zone}",
        d.year, d.month, d.day, d.hour, d.minute, d.second
    )
}

fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>, InkoxideError> {
    crate::flate::deflate(data, level)
        .map_err(|e| InkoxideError::Pdf(format!("compression failed: {e}")))
}

/// Page tree: flat when small, otherwise intermediate nodes of at most
/// [`PAGE_TREE_FANOUT`] kids.
struct PageTree {
    root: Ref,
    /// Parent of pages `[i * FANOUT, (i + 1) * FANOUT)`.
    leaf_parents: Vec<Ref>,
}

fn write_page_tree(pdf: &mut Pdf, alloc: &mut RefAlloc, pages: &[Ref]) -> PageTree {
    let root = alloc.next();
    if pages.len() <= PAGE_TREE_FANOUT {
        pdf.pages(root)
            .kids(pages.iter().copied())
            .count(pages.len() as i32);
        return PageTree {
            root,
            leaf_parents: vec![root],
        };
    }

    // Build bottom-up: level 0 groups pages; each next level groups nodes.
    // `level` holds (node ref, kids, page count).
    let mut level: Vec<(Ref, Vec<Ref>, usize)> = pages
        .chunks(PAGE_TREE_FANOUT)
        .map(|c| (alloc.next(), c.to_vec(), c.len()))
        .collect();
    let leaf_parents: Vec<Ref> = level.iter().map(|n| n.0).collect();
    let mut parent_of = std::collections::HashMap::<Ref, Ref>::new();
    let mut nodes: Vec<(Ref, Vec<Ref>, usize)> = Vec::new();
    while level.len() > PAGE_TREE_FANOUT {
        let next: Vec<(Ref, Vec<Ref>, usize)> = level
            .chunks(PAGE_TREE_FANOUT)
            .map(|c| {
                let r = alloc.next();
                (
                    r,
                    c.iter().map(|n| n.0).collect(),
                    c.iter().map(|n| n.2).sum(),
                )
            })
            .collect();
        for (r, kids, _) in &next {
            for k in kids {
                parent_of.insert(*k, *r);
            }
        }
        nodes.extend(level);
        level = next;
    }
    for n in &level {
        parent_of.insert(n.0, root);
    }
    nodes.extend(level.iter().cloned());
    let total = pages.len() as i32;
    pdf.pages(root).kids(level.iter().map(|n| n.0)).count(total);
    for (r, kids, count) in nodes {
        let parent = parent_of.get(&r).copied().unwrap_or(root);
        pdf.pages(r).parent(parent).kids(kids).count(count as i32);
    }
    PageTree { root, leaf_parents }
}

/// Opacity quantised to 1/1000: the key of its shared graphics state.
fn alpha_key(a: f32) -> u16 {
    (a.clamp(0.0, 1.0) * 1000.0).round() as u16
}

/// The opacity [`alpha_key`] quantised to `key`.
fn key_alpha(key: u16) -> f32 {
    f32::from(key) / 1000.0
}

fn gs_name(key: u16) -> String {
    format!("A{key}")
}

fn image_name(key: ImageKey) -> String {
    format!("Im{key}")
}

fn font_name(key: FontKey) -> String {
    format!("F{key}")
}

/// Text ops: one `TJ` per stretch of glyphs on the baseline, with kerning
/// adjustments wherever our positions differ from the font's advances.
/// With `skip_notdef` (PDF/A), `.notdef` glyphs are left out and the next
/// glyph keeps its position.
fn show_text(c: &mut Content, t: &TextOp, fonts: &FontSet<'_>, skip_notdef: bool) {
    let font = fonts.get(t.font);
    let scale = font.scale(t.size);
    let size = q(t.size);
    set_fill(c, &t.color);
    c.begin_text();
    c.set_font(Name(font_name(t.font).as_bytes()), size);
    let glyphs = &t.glyphs;
    let mut i = 0;
    while i < glyphs.len() {
        let g0 = glyphs[i];
        if skip_notdef && g0.id == 0 {
            i += 1;
            continue;
        }
        // Text space is y-up; the page is flipped, so flip back.
        c.set_text_matrix([1.0, 0.0, 0.0, -1.0, q(t.x + g0.x), q(t.y - g0.y_offset)]);
        if g0.y_offset != 0.0 {
            c.show(Str(&g0.id.to_be_bytes()));
            i += 1;
            continue;
        }
        let mut shown = c.show_positioned();
        let mut items = shown.items();
        let mut buf: Vec<u8> = Vec::new();
        let mut pen = g0.x;
        while i < glyphs.len() && glyphs[i].y_offset == 0.0 {
            let g = glyphs[i];
            if skip_notdef && g.id == 0 {
                i += 1;
                continue;
            }
            let delta = g.x - pen;
            if delta.abs() > KERN_EPS {
                if !buf.is_empty() {
                    items.show(Str(&buf));
                    buf.clear();
                }
                // TJ adjustments are in thousandths of text space, inverted.
                items.adjust(q(-delta * 1000.0 / t.size));
            }
            buf.extend_from_slice(&g.id.to_be_bytes());
            pen = g.x + font.advance(g.id) * scale;
            i += 1;
        }
        if !buf.is_empty() {
            items.show(Str(&buf));
        }
    }
    c.end_text();
}

fn set_fill(c: &mut Content, color: &Color) {
    c.set_fill_rgb(q(color[0]), q(color[1]), q(color[2]));
}

fn path(c: &mut Content, segs: &[PathSeg]) {
    for s in segs {
        match *s {
            PathSeg::MoveTo(x, y) => {
                c.move_to(q(x), q(y));
            }
            PathSeg::LineTo(x, y) => {
                c.line_to(q(x), q(y));
            }
            PathSeg::CubicTo(x1, y1, x2, y2, x3, y3) => {
                c.cubic_to(q(x1), q(y1), q(x2), q(y2), q(x3), q(y3));
            }
            PathSeg::Close => {
                c.close_path();
            }
        }
    }
}

/// Serialises one page's display list. Pure function of the page: safe to
/// run per page in parallel.
pub(crate) fn content_stream(
    page: &PageContent,
    fonts: &FontSet<'_>,
    profile: Profile,
) -> Result<Vec<u8>, InkoxideError> {
    let mut c = Content::new();
    // Flip to a top-left origin once; everything below is in layout space.
    c.transform([1.0, 0.0, 0.0, -1.0, 0.0, q(page.height)]);
    for op in &page.ops {
        match op {
            Op::Save => {
                c.save_state();
            }
            Op::Restore => {
                c.restore_state();
            }
            Op::Transform(m) => {
                c.transform(m.map(|v| round_to(v, 1e5)));
            }
            Op::ClipRect(r) => {
                c.rect(q(r.x), q(r.y), q(r.w), q(r.h));
                c.clip_nonzero();
                c.end_path();
            }
            Op::ClipPath { path: p, rule } => {
                path(&mut c, &p.segs);
                match rule {
                    FillRule::NonZero => c.clip_nonzero(),
                    FillRule::EvenOdd => c.clip_even_odd(),
                };
                c.end_path();
            }
            Op::FillRect { rect, color } => {
                set_fill(&mut c, color);
                c.rect(q(rect.x), q(rect.y), q(rect.w), q(rect.h));
                c.fill_nonzero();
            }
            Op::FillPath {
                path: p,
                color,
                rule,
            } => {
                set_fill(&mut c, color);
                path(&mut c, &p.segs);
                match rule {
                    FillRule::NonZero => c.fill_nonzero(),
                    FillRule::EvenOdd => c.fill_even_odd(),
                };
            }
            Op::StrokePath {
                path: p,
                color,
                width,
                dash,
            } => {
                c.save_state();
                c.set_stroke_rgb(q(color[0]), q(color[1]), q(color[2]));
                c.set_line_width(q(*width));
                if let Some(d) = dash {
                    c.set_dash_pattern(d.pattern.map(q), 0.0);
                    if d.round_cap {
                        c.set_line_cap(LineCapStyle::RoundCap);
                    }
                }
                path(&mut c, &p.segs);
                c.stroke();
                c.restore_state();
            }
            Op::Link { .. } | Op::Field(_) | Op::Icon(_) => {}
            Op::SetOpacity(a) => {
                c.set_parameters(Name(gs_name(alpha_key(*a)).as_bytes()));
            }
            // Its colour glyphs are images; the others have no ink.
            Op::Text(t) if fonts.get(t.font).data.metrics.color_only => {}
            Op::Text(t) => show_text(&mut c, t, fonts, profile.is_pdf_a()),
            Op::Image { key, matrix } => {
                c.save_state();
                c.transform(matrix.map(q));
                c.x_object(Name(image_name(*key).as_bytes()));
                c.restore_state();
            }
            Op::Span { actual_text } => {
                c.begin_marked_content_with_properties(Name(b"Span"))
                    .properties()
                    .pair(Name(b"ActualText"), Str(&out::text_bytes(actual_text)));
            }
            Op::EndSpan => {
                c.end_marked_content();
            }
            Op::InvisibleText(g) => invisible_glyph(&mut c, g, fonts, profile),
        }
    }
    content_bytes(c, profile)
}

/// An invisible glyph (`3 Tr`), horizontally scaled (`Tz`) to its width,
/// in its own save level: text state outlives `ET`. Where `profile`
/// bounds real numbers (PDF/A-1), a scaling past the bound is moved into
/// the font size, which draws the same width.
fn invisible_glyph(c: &mut Content, g: &InvisibleGlyph, fonts: &FontSet<'_>, profile: Profile) {
    let font = fonts.get(g.font);
    let advance = font.advance(g.glyph) * font.scale(g.size);
    let mut scaling = 100.0 * g.width / advance;
    if !(scaling.is_finite() && scaling > 0.0) {
        return;
    }
    let mut size = g.size;
    let max = profile.max_real();
    if scaling > max {
        size *= scaling / max;
        scaling = max;
    }
    c.save_state();
    c.begin_text();
    c.set_font(Name(font_name(g.font).as_bytes()), q(size));
    c.set_text_rendering_mode(TextRenderingMode::Invisible);
    c.set_horizontal_scaling(q(scaling));
    c.set_text_matrix([1.0, 0.0, 0.0, -1.0, q(g.x), q(g.y)]);
    c.show(Str(&g.glyph.to_be_bytes()));
    c.end_text();
    c.restore_state();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{Dash, Path};

    #[test]
    fn numbers_are_rounded_bounded_and_never_non_finite() {
        assert_eq!(q(1.234_56), 1.235);
        assert_eq!(q(-0.000_4), -0.0);
        // Tiny values keep their precision where they are not rounded.
        assert_eq!(number(1e-7), 1e-7);
        assert_eq!(number(-1e-30), -1e-30);
        // Large values pass unchanged up to the bound, and stop there,
        // whether or not they are too large to scale by 1000.
        for v in [1e6, 1e10, MAX_NUMBER] {
            assert_eq!(number(v), v);
            assert_eq!(number(-v), -v);
            assert_eq!(q(v), v);
        }
        for v in [1.5e11, 1e12, 1e30, f32::MAX] {
            assert_eq!(number(v), MAX_NUMBER);
            assert_eq!(number(-v), -MAX_NUMBER);
            assert_eq!(q(v), MAX_NUMBER);
            assert_eq!(q(-v), -MAX_NUMBER);
            assert_eq!(round_to(v, 1e5), MAX_NUMBER);
        }
        // ±2^31 would be written as a 32-bit integer: -2^31 is past the
        // range pdf-writer can register.
        assert!(number(-2_147_483_648.0) > -2_147_483_648.0);
        assert!(number(2_147_483_648.0) < 2_147_483_648.0);
        for v in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(number(v), 0.0);
            assert_eq!(q(v), 0.0);
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
        let span =
            format!("/Span <<\n  /ActualText <{hex}>\n>> BDC\n1 0 0 rg\n20 10 30 30 re\nf\n");
        assert!(text.contains(&span), "{span} in:\n{text}");
        // The invisible glyph: in its own save level, in mode 3, scaled to
        // 30 pt from the space's advance.
        let font = fonts.get(fonts.key("Inter-Regular").unwrap());
        let space = font.glyph(' ').unwrap();
        let scaling = q(100.0 * 30.0 / (font.advance(space) * font.scale(10.0)));
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
        let pages = [sink.encode_page(&page, &fonts).unwrap()];
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
                images: &images,
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
            return; // pdftotext is not installed
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
        let encoded = sink.encode_page(&page, &fonts).unwrap();
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
        // About 1.8 million percent at 1 pt.
        assert!(operand(Profile::default(), " Tz") > 1e6);
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
            let out = String::from_utf8(content_stream(&page, &fonts, Profile::default()).unwrap())
                .unwrap();
            assert!(!out.contains("Tr"), "{width}: {out}");
        }
    }
}

#[cfg(test)]
mod pdf_a_1_tests {
    use crate::{FontStore, ImageStore, InputFormat, PdfALevel, RenderOptions};

    /// Renders a 300 × 300 pt page holding `children`, with `meta`, at
    /// `level`.
    fn render(
        children: &str,
        meta: &str,
        level: PdfALevel,
    ) -> Result<Vec<u8>, crate::InkoxideError> {
        let doc = format!(
            r#"{{"version":1,"meta":{{{meta}}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
               "pages":[{{"size":{{"width":300,"height":300}},"children":[{children}]}}]}}"#
        );
        let fonts: FontStore = crate::test_fixtures::inter();
        let options = RenderOptions::default().with_pdf_a_level(level);
        crate::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options,
            &fonts,
            &ImageStore::new(),
        )
    }

    #[test]
    fn pdf_a_1b_fails_past_its_limits() {
        let nested = |n: usize| {
            let open = r#"{"type":"box","style":{"overflow":"hidden"},"children":["#.repeat(n);
            format!("{open}{}", "]}".repeat(n))
        };
        let title = format!(r#""title":"{}""#, "a".repeat(70_000));
        // A word of 4200 letters, each spaced: a `TJ` array of an entry
        // per letter and per adjustment.
        let spaced = format!(
            r#"{{"type":"text","style":{{"whiteSpace":"nowrap"}},"runs":[{{"text":"{}","font":"Inter-Regular","size":2,"letterSpacing":1}}]}}"#,
            "x".repeat(4200)
        );
        let cases = [
            (
                r#"{"type":"box","style":{"width":10,"height":10,"transform":[{"translate":[40000.5,0]}]}}"#.to_string(),
                "",
                "a real number past ±32767",
            ),
            (nested(30), "", "graphics states nested more than 28 deep"),
            (String::new(), title.as_str(), "a string longer than 65535 bytes"),
            (spaced, "", "an array of more than 8191 entries"),
        ];
        for (child, meta, what) in cases {
            let Err(e) = render(&child, meta, PdfALevel::A1b) else {
                panic!("not refused: {what}");
            };
            assert_eq!(e.code(), "PDF_ERROR", "{e}");
            assert!(e.to_string().contains(what), "{e}");
            render(&child, meta, PdfALevel::A2b).unwrap();
        }
        // Within the limits.
        render(&nested(20), "", PdfALevel::A1b).unwrap();
        render(
            r#"{"type":"box","style":{"width":10,"height":10,"transform":[{"translate":[32000.5,0]}]}}"#,
            "",
            PdfALevel::A1b,
        )
        .unwrap();
    }
}
