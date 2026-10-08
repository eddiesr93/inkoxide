//! Display list → PDF bytes, through `pdf-writer`.

pub(crate) mod cff;
mod font;
mod image;
mod pdfa;
mod svg;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, LineCapStyle, OutputIntentSubtype, PageMode,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};
use rayon::prelude::*;

use crate::date::Timestamp;
use crate::fonts::{FontKey, FontSet};
use crate::images::{ImageKey, ImageSet};
use crate::paint::{FillRule, Op, PageContent, PathSeg, Rect, TextOp};
use crate::pipeline::{Dest, Marks};
use crate::schema::{Color, Meta};
use crate::text::UnicodeMap;
use crate::InkoxideError;

/// Maximum kids per page-tree node (keeps viewers fast on long documents).
const PAGE_TREE_FANOUT: usize = 50;

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

/// Dependency firewall around the PDF writer (§5.1).
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
    /// Embed only the glyphs used (ADR-032).
    pub subset_fonts: bool,
    /// Write PDF/A-2b (ADR-039).
    pub pdf_a: bool,
}

/// Rounds to 1/1000 pt so output has at most three decimals.
#[inline]
pub(crate) fn q(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

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
        for op in &page.ops {
            match op {
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
                Op::Link { rect, target } => links.push((*rect, target.clone())),
                _ => {}
            }
        }
        let raw = content_stream(page, fonts, self.pdf_a);
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
        })
    }

    fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError> {
        let mut pdf = Pdf::new();
        let mut alloc = RefAlloc(0);
        let catalog = alloc.next();
        let info = alloc.next();

        // Page refs first, in document order, so numbering is stable.
        let page_refs: Vec<(Ref, Ref)> = doc
            .pages
            .iter()
            .map(|_| (alloc.next(), alloc.next()))
            .collect();
        let tree_root = write_page_tree(
            &mut pdf,
            &mut alloc,
            &page_refs.iter().map(|p| p.0).collect::<Vec<_>>(),
        );

        // Shared graphics states, one per distinct opacity, in value order.
        let mut opacities: BTreeMap<u16, Ref> = BTreeMap::new();
        for page in doc.pages {
            for &key in &page.opacities {
                opacities.entry(key).or_insert(Ref::new(1));
            }
        }
        for (key, r) in opacities.iter_mut() {
            *r = alloc.next();
            let a = f32::from(*key) / 1000.0;
            pdf.ext_graphics(*r).non_stroking_alpha(a).stroking_alpha(a);
        }

        // Fonts: embed each font that is actually painted, with the glyphs
        // used across the whole document.
        let mut used: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
        for page in doc.pages {
            for (key, glyphs) in &page.glyphs {
                used.entry(*key).or_default().extend(glyphs);
            }
        }
        // Subsetting and compression are independent per font.
        let programs: Vec<font::Program> = used
            .par_iter()
            .map(|(key, glyphs)| font::program(doc.fonts.get(*key), glyphs, self.subset_fonts))
            .collect::<Result<_, _>>()?;
        let mut font_refs: BTreeMap<FontKey, Ref> = BTreeMap::new();
        for ((key, glyphs), program) in used.iter().zip(&programs) {
            let r = font::write_font(
                &mut pdf,
                &mut || alloc.next(),
                doc.fonts.get(*key),
                program,
                glyphs,
                doc.unicode.fonts.get(key),
            )?;
            font_refs.insert(*key, r);
        }

        // Images: one XObject per unique image actually painted.
        let mut image_refs: BTreeMap<ImageKey, Ref> = BTreeMap::new();
        for page in doc.pages {
            for &key in &page.images {
                image_refs.entry(key).or_insert(Ref::new(1));
            }
        }
        for (key, r) in image_refs.iter_mut() {
            *r = image::write_image(
                &mut pdf,
                &mut || alloc.next(),
                doc.images.get(*key),
                self.pdf_a,
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
                    r,
                    page.height,
                    link,
                    doc.marks,
                    &page_refs,
                    self.pdf_a,
                ) {
                    refs.push(r);
                }
            }
            annots.push(refs);
        }
        let outline = write_outline(&mut pdf, &mut alloc, &doc.marks.outline, &page_refs);

        for (i, (page, (page_ref, content_ref))) in doc.pages.iter().zip(&page_refs).enumerate() {
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

            let mut stream = pdf.stream(*content_ref, &page.stream);
            if page.compressed {
                stream.filter(Filter::FlateDecode);
            }
        }

        let mut cat = pdf.catalog(catalog);
        cat.pages(tree_root.root);
        if let Some(root) = outline {
            cat.outlines(root);
            cat.page_mode(PageMode::UseOutlines);
        }
        if let Some(lang) = &doc.meta.lang {
            cat.lang(TextStr(lang));
        }
        let producer = concat!("Inkoxide ", env!("CARGO_PKG_VERSION"));
        if self.pdf_a {
            let metadata = alloc.next();
            let profile = alloc.next();
            cat.metadata(metadata);
            cat.output_intents()
                .push()
                .subtype(OutputIntentSubtype::PDFA)
                .output_condition_identifier(TextStr("sRGB"))
                .info(TextStr("sRGB IEC61966-2.1"))
                .dest_output_profile(profile);
            cat.finish();
            // XMP must stay uncompressed (ISO 19005-2, 6.6.2.1).
            let xmp = pdfa::xmp(doc.meta, producer, doc.creation_date);
            pdf.metadata(metadata, &xmp);
            let icc = deflate(&pdfa::srgb_icc(), 9)?;
            pdf.icc_profile(profile, &icc)
                .n(3)
                .range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
                .filter(Filter::FlateDecode);
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
            if self.pdf_a {
                pdfa::clean(v)
            } else {
                v.to_string()
            }
        };
        let mut inf = pdf.document_info(info);
        let m = doc.meta;
        if let Some(v) = &m.title {
            inf.title(TextStr(&text(v)));
        }
        if let Some(v) = &m.author {
            inf.author(TextStr(&text(v)));
        }
        if let Some(v) = &m.subject {
            inf.subject(TextStr(&text(v)));
        }
        if let Some(v) = &m.keywords {
            inf.keywords(TextStr(&text(v)));
        }
        if let Some(v) = &m.creator {
            inf.creator(TextStr(&text(v)));
        }
        inf.producer(TextStr(producer));
        if let Some(t) = doc.creation_date {
            inf.creation_date(pdf_date(t));
        }
        inf.finish();

        Ok(pdf.finish())
    }
}

/// Writes one link annotation as object `r`; `false` (nothing written)
/// when an anchor has no destination.
fn write_link(
    pdf: &mut Pdf,
    r: Ref,
    page_h: f32,
    (rect, target): &(Rect, Box<str>),
    marks: &Marks<'_>,
    pages: &[(Ref, Ref)],
    pdf_a: bool,
) -> bool {
    let dest = match target.strip_prefix('#') {
        Some(id) => match marks.anchors.get(id) {
            Some(d) => Some(*d),
            None => return false,
        },
        None => None,
    };
    let mut a = pdf.annotation(r);
    a.subtype(AnnotationType::Link)
        .rect(PdfRect::new(
            q(rect.x),
            q(page_h - rect.y - rect.h),
            q(rect.x + rect.w),
            q(page_h - rect.y),
        ))
        .border(0.0, 0.0, 0.0, None);
    if pdf_a {
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
                .uri(Str(uri_bytes(target).as_slice()));
        }
    }
    true
}

/// `[page /XYZ left top 0]`: keeps the viewer's zoom.
fn xyz(d: pdf_writer::writers::Destination<'_>, dest: Dest, pages: &[(Ref, Ref)]) {
    let (page_ref, _) = pages[dest.page as usize];
    d.page(page_ref).xyz(q(dest.x), q(dest.y), None);
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
    entries: &[crate::pipeline::OutlineEntry<'_>],
    page_refs: &[(Ref, Ref)],
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
        let mut item = pdf.outline_item(refs[i]);
        item.title(TextStr(e.title))
            .parent(parent[i].map_or(root, |p| refs[p]));
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

fn pdf_date(t: Timestamp) -> pdf_writer::Date {
    let off = t.offset_minutes;
    let date = pdf_writer::Date::new(t.year)
        .month(t.month)
        .day(t.day)
        .hour(t.hour)
        .minute(t.minute)
        .second(t.second);
    if off == 0 {
        date.utc_offset_hour(0).utc_offset_minute(0)
    } else {
        date.utc_offset_hour((off / 60) as i8)
            .utc_offset_minute((off.unsigned_abs() % 60) as u8)
    }
}

pub(crate) fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>, InkoxideError> {
    let mut enc = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 3 + 64),
        Compression::new(level),
    );
    enc.write_all(data)
        .and_then(|_| enc.finish())
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
    let mut parent_of: std::collections::HashMap<Ref, Ref> = Default::default();
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
            if delta.abs() > 1e-3 {
                if !buf.is_empty() {
                    items.show(Str(&buf));
                    buf.clear();
                }
                // TJ adjustments are in thousandths of text space, inverted.
                items.adjust(((-delta * 1000.0 / t.size) * 1000.0).round() / 1000.0);
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
pub(crate) fn content_stream(page: &PageContent, fonts: &FontSet<'_>, pdf_a: bool) -> Vec<u8> {
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
                c.transform(m.map(|v| (v * 1e5).round() / 1e5));
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
            Op::Link { .. } => {}
            Op::SetOpacity(a) => {
                c.set_parameters(Name(gs_name(alpha_key(*a)).as_bytes()));
            }
            Op::Text(t) => show_text(&mut c, t, fonts, pdf_a),
            Op::Image { key, matrix } => {
                c.save_state();
                c.transform(matrix.map(q));
                c.x_object(Name(image_name(*key).as_bytes()));
                c.restore_state();
            }
        }
    }
    c.finish().into_vec()
}
