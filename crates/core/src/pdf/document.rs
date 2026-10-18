//! The document around the pages: the page tree, resources, catalog
//! and information dictionary.

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{PageLayout, PageMode};
use pdf_writer::{Filter, Finish, Name, Rect as PdfRect, Ref, Str};

use crate::date::Timestamp;
use crate::fonts::FontKey;
use crate::images::ImageKey;
use crate::schema::{self};
use crate::InkoxideError;

use super::{
    alpha_key, attach, deflate, font, font_name, forms, gs_name, image, image_name, key_alpha,
    notes, pdfa, placement_name, round_pt, svg, svg_text_uses, tag, text_ink, type3, type3_name,
    write_link, write_outline, DocumentOut, FontPrograms, Out, PdfWriterSink, Writer, MAX_NUMBER,
};

/// Where an SVG is placed: each page and placement on it.
type Uses = Vec<(usize, usize)>;

/// Maximum kids per page-tree node (keeps viewers fast on long documents).
pub(super) const PAGE_TREE_FANOUT: usize = 50;

/// The shared resources of the pages: graphics states by opacity key,
/// fonts, images, and the Type 3 fonts of colour glyphs by index.
struct Resources {
    opacities: BTreeMap<u16, Ref>,
    fonts: BTreeMap<FontKey, Ref>,
    images: BTreeMap<ImageKey, Ref>,
    type3: BTreeMap<u16, Ref>,
    /// The forms of the SVGs placed with their parts tagged, by page and
    /// place ([`super::EncodedPage::placements`]).
    placements: Vec<Vec<Ref>>,
}

impl PdfWriterSink {
    /// Writes every object of the document, its strings and streams
    /// through `out`, and hands back the writer: the file before its
    /// trailer and the numbers it allocated. With the same `doc` and
    /// `fonts`, every call numbers the objects the same.
    pub(super) fn emit(
        &self,
        doc: &DocumentOut<'_>,
        fonts: &FontPrograms,
        out: Out,
    ) -> Result<Writer, InkoxideError> {
        let mut w = Writer::new(out);
        if let Some((major, minor)) = self.profile.version() {
            w.pdf.set_version(major, minor);
        }
        // Structure elements go into object streams, compressed as the
        // content streams are.
        if let Some(level) = self.compression.filter(|_| doc.plan.is_some()) {
            if self.profile.object_streams() {
                w.pack(level);
            }
        }
        let catalog = w.alloc.next();
        let info = w.alloc.next();

        // Page and content stream refs first, in document order, so
        // numbering is stable.
        let (page_refs, content_refs): (Vec<Ref>, Vec<Ref>) = doc
            .pages
            .iter()
            .map(|_| (w.alloc.next(), w.alloc.next()))
            .unzip();
        let tree = write_page_tree(&mut w, &page_refs);

        let opacities = self.write_opacities(&mut w, doc)?;
        let (font_refs, vertical) = self.write_fonts(&mut w, doc, fonts)?;
        let image_refs = self.write_images(&mut w, doc, (&font_refs, &vertical))?;
        let mut tags = doc.plan.map(|_| tag::AnnotTags::new(doc.pages.len()));
        let placements = match tags.as_mut() {
            Some(t) => {
                self.write_placements(&mut w, doc, (&font_refs, &vertical), &image_refs, t)?
            }
            None => Vec::new(),
        };
        let type3 = self.write_type3(&mut w, doc, &image_refs)?;
        let res = Resources {
            opacities,
            fonts: font_refs,
            images: image_refs,
            type3,
            placements,
        };
        let files = attach::write_files(&mut w, doc.files, self.profile.pdf_2());
        let (annots, form) =
            self.write_annotations(&mut w, doc, &page_refs, &res.fonts, &files, tags.as_mut())?;
        // PDF/UA-2: each bookmark goes to its element too.
        let outline_sd: Vec<Option<Ref>> = match (doc.plan, tags.as_mut()) {
            (Some(plan), Some(t)) if self.profile.pdf_2() => doc
                .marks
                .outline
                .iter()
                .map(|e| plan.bookmark(e.node).map(|id| t.dest(id, &mut w.alloc)))
                .collect(),
            _ => Vec::new(),
        };
        let outline = write_outline(&mut w, &doc.marks.outline, &page_refs, &outline_sd);
        write_pages(
            &mut w,
            doc,
            (&page_refs, &content_refs),
            &tree,
            &res,
            &annots,
        );

        let producer = doc
            .meta
            .producer
            .as_deref()
            .unwrap_or(concat!("Inkoxide ", env!("CARGO_PKG_VERSION")));
        let struct_root = tags.as_ref().map(|_| w.alloc.next());
        let nav = Nav {
            pages: tree.root,
            form: form.as_ref(),
            outline,
            files: &files.specs,
            struct_root,
        };
        self.write_catalog(&mut w, doc, catalog, &nav, &res.fonts, producer)?;
        self.write_info(&mut w, doc, info, producer);
        if let (Some(plan), Some(root), Some(tags)) = (doc.plan, struct_root, &tags) {
            tag::write_tree(
                &mut w,
                plan,
                root,
                doc.pages,
                &page_refs,
                tags,
                self.profile.pdf_2(),
            );
        }

        // Content streams are checked as they are finished; this covers
        // the objects (matrices, boxes, rectangles, coordinates).
        debug_assert!(
            w.pdf.limits().real() <= MAX_NUMBER,
            "a number past MAX_NUMBER: {}",
            w.pdf.limits().real()
        );
        self.profile.check_limits(w.pdf.limits())?;
        Ok(w)
    }

    /// Shared graphics states, one per distinct opacity, in value order.
    fn write_opacities(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
    ) -> Result<BTreeMap<u16, Ref>, InkoxideError> {
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
            *r = w.alloc.next();
            let a = key_alpha(*key);
            w.pdf
                .ext_graphics(*r)
                .non_stroking_alpha(a)
                .stroking_alpha(a);
        }
        Ok(opacities)
    }

    /// The fonts, each with the glyphs the document uses, and the
    /// vertical fonts of those SVG text draws upright.
    fn write_fonts(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        fonts: &FontPrograms,
    ) -> Result<(BTreeMap<FontKey, Ref>, svg::VerticalFonts), InkoxideError> {
        let mut font_refs: BTreeMap<FontKey, Ref> = BTreeMap::new();
        let mut vertical = svg::VerticalFonts::new();
        for ((key, glyphs), program) in fonts.used.iter().zip(&fonts.programs) {
            // Glyphs laid out keep their text; field glyphs add theirs.
            let mut unicode = doc.unicode.font_with(*key, &fonts.field_text);
            // Tagged output maps every glyph (ISO 14289-1, 7.21.7): a glyph
            // after the first of its cluster (a mark the font draws apart)
            // gets the combining grapheme joiner, and is drawn in a span of
            // no text (`show_text`), so that extraction reads the cluster
            // once.
            if doc.plan.is_some() {
                let all = unicode.get_or_insert_with(Default::default).to_mut();
                for &g in glyphs {
                    let text = all.entry(g).or_default();
                    if text.is_empty() {
                        text.push('\u{34F}');
                    }
                }
            }
            let embedded = font::Embedded {
                font: doc.fonts.get(*key),
                program,
                used: glyphs,
                unicode: unicode.as_deref(),
                vertical: fonts.vertical.contains(key),
            };
            let refs = font::write_font(w, &embedded, self.profile)?;
            font_refs.insert(*key, refs.horizontal);
            if let Some(v) = refs.vertical {
                vertical.insert(*key, v);
            }
        }
        Ok((font_refs, vertical))
    }

    /// Images: one `XObject` per unique image actually painted, those SVG
    /// text draws first, for the forms of the SVGs to name them.
    fn write_images(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        (font_refs, vertical): (&BTreeMap<FontKey, Ref>, &svg::VerticalFonts),
    ) -> Result<BTreeMap<ImageKey, Ref>, InkoxideError> {
        let mut image_refs: BTreeMap<ImageKey, Ref> = BTreeMap::new();
        let text_images: BTreeSet<ImageKey> =
            svg_text_uses(doc).flat_map(|uses| uses.images).collect();
        // And those the Type 3 fonts of colour glyphs draw.
        let type3 = doc.colors.type3();
        let glyph_images = type3_used(doc)
            .flat_map(|i| &type3[usize::from(i)].glyphs)
            .map(|g| g.image);
        let on_pages: BTreeSet<ImageKey> = doc
            .pages
            .iter()
            .flat_map(|p| p.images.iter().copied())
            .chain(glyph_images)
            .filter(|k| !text_images.contains(k))
            .collect();
        for key in text_images.into_iter().chain(on_pages) {
            let res = svg::TextResources {
                ink: text_ink(doc),
                refs: font_refs,
                vertical,
                image_refs: &image_refs,
            };
            let r = image::write_image(w, doc.images.get(key), self.profile, Some(res))?;
            image_refs.insert(key, r);
        }
        Ok(image_refs)
    }

    /// The forms of the SVGs placed with their parts tagged, by page and
    /// place: one for each placement, each with a `/StructParents` key of
    /// `tags`, and the content written once for each SVG and tag of the
    /// sequences of the element it is in.
    fn write_placements(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        (font_refs, vertical): (&BTreeMap<FontKey, Ref>, &svg::VerticalFonts),
        image_refs: &BTreeMap<ImageKey, Ref>,
        tags: &mut tag::AnnotTags,
    ) -> Result<Vec<Vec<Ref>>, InkoxideError> {
        let Some(plan) = doc.plan else {
            return Ok(Vec::new());
        };
        // Each SVG and the kind of its element, and where it is placed.
        let mut groups: BTreeMap<(ImageKey, &'static [u8]), Uses> = BTreeMap::new();
        for (i, page) in doc.pages.iter().enumerate() {
            for (n, p) in page.placements.iter().enumerate() {
                let outer = plan.elems[p.owner as usize].kind.name();
                groups.entry((p.key, outer)).or_default().push((i, n));
            }
        }
        let mut refs: Vec<Vec<Option<Ref>>> = doc
            .pages
            .iter()
            .map(|p| vec![None; p.placements.len()])
            .collect();
        for ((key, outer), uses) in groups {
            let keys: Vec<i32> = uses.iter().map(|_| tags.form_key()).collect();
            let res = svg::TextResources {
                ink: text_ink(doc),
                refs: font_refs,
                vertical,
                image_refs,
            };
            let img = doc.images.get(key);
            let Some((forms, form_tags)) =
                image::write_tagged_svg(w, img, self.profile, Some(res), (outer, &keys))?
            else {
                // Not an SVG with parts after all: drawn as itself.
                let r = image::write_image(w, img, self.profile, Some(res))?;
                for &(page, n) in &uses {
                    refs[page][n] = Some(r);
                }
                continue;
            };
            let form_tags = std::rc::Rc::new(form_tags);
            for ((&(page, n), &key), r) in uses.iter().zip(&keys).zip(forms) {
                let p = doc.pages[page].placements[n];
                refs[page][n] = Some(r);
                tags.forms.push(tag::FormEntry {
                    key,
                    r,
                    page,
                    after: p.after,
                    owner: p.owner,
                    tags: form_tags.clone(),
                });
            }
        }
        Ok(refs
            .into_iter()
            .map(|page| page.into_iter().flatten().collect())
            .collect())
    }

    /// The Type 3 fonts of colour glyphs the pages use, by index.
    fn write_type3(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        image_refs: &BTreeMap<ImageKey, Ref>,
    ) -> Result<BTreeMap<u16, Ref>, InkoxideError> {
        let mut refs = BTreeMap::new();
        for index in type3_used(doc) {
            let glyphs = &doc.colors.type3()[usize::from(index)];
            // The text of each code, from the first page that draws it.
            let mut text = BTreeMap::new();
            for page in doc.pages {
                let codes = page.type3_text.range((index, 0)..=(index, u8::MAX));
                for (&(_, code), t) in codes {
                    text.entry(code).or_insert_with(|| t.clone());
                }
            }
            let t = type3::Type3 {
                glyphs,
                font: doc.fonts.get(glyphs.font),
                index,
                images: doc.images,
                image_refs,
                text: &text,
                tagged: doc.plan.is_some(),
            };
            refs.insert(index, type3::write_type3(w, &t, self.profile)?);
        }
        Ok(refs)
    }

    /// The annotations of each page, in page order: links, then icons,
    /// then form field widgets; and the form, when there are fields.
    fn write_annotations(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        page_refs: &[Ref],
        font_refs: &BTreeMap<FontKey, Ref>,
        files: &attach::FileRefs<'_>,
        mut tags: Option<&mut tag::AnnotTags>,
    ) -> Result<(Vec<Vec<Ref>>, Option<forms::Form>), InkoxideError> {
        let mut annots: Vec<Vec<Ref>> = Vec::with_capacity(doc.pages.len());
        for (i, page) in doc.pages.iter().enumerate() {
            let mut refs = Vec::new();
            for link in &page.links {
                let r = w.alloc.next();
                // Recorded only when written: an anchor may have no
                // destination.
                let key = tags.as_deref().map(tag::AnnotTags::peek);
                // PDF/UA-2: a link within the document goes to the
                // element of its target too (ISO 14289-2, 8.8).
                let sd = match (doc.plan, tags.as_deref_mut()) {
                    (Some(plan), Some(t)) if self.profile.pdf_2() => link
                        .target
                        .strip_prefix('#')
                        .and_then(|id| plan.anchor(id))
                        .map(|id| t.dest(id, &mut w.alloc)),
                    _ => None,
                };
                if write_link(
                    w,
                    r,
                    page.height,
                    link,
                    (doc.marks, page_refs),
                    self.profile,
                    (key, sd),
                ) {
                    if let Some(t) = tags.as_deref_mut() {
                        t.add(r, i, link.owner, tag::AnnotKind::Link);
                    }
                    refs.push(r);
                }
            }
            annots.push(refs);
        }
        notes::write_icons(
            w,
            (doc.pages, page_refs),
            &mut annots,
            files,
            tags.as_deref_mut(),
            self.profile,
        )?;
        let fields = forms::Ctx {
            pages: doc.pages,
            page_refs,
            font_refs,
            fonts: doc.fonts,
            profile: self.profile,
        };
        let form = forms::write_fields(w, &fields, &mut annots, tags)?;
        Ok((annots, form))
    }

    /// The catalog, and under PDF/A its XMP metadata, output intent and
    /// file identifier.
    fn write_catalog(
        &self,
        w: &mut Writer,
        doc: &DocumentOut<'_>,
        catalog: Ref,
        nav: &Nav<'_>,
        font_refs: &BTreeMap<FontKey, Ref>,
        producer: &str,
    ) -> Result<(), InkoxideError> {
        let lang = doc.meta.lang.as_deref().map(|l| w.out.text(l));
        let encrypted = w.out.is_encrypted();
        let pdf_a = self
            .profile
            .level()
            .map(|level| (level, w.alloc.next(), w.alloc.next()));
        let intent = pdf_a.map(|_| (w.out.text("sRGB"), w.out.text("sRGB IEC61966-2.1")));
        // PDF/UA without PDF/A: metadata of its own (ISO 14289-1, 7.1;
        // ISO 14289-2, 8.6).
        let ua_metadata = (pdf_a.is_none() && self.profile.ua()).then(|| w.alloc.next());
        let embedded = attach::name_tree(doc.files, nav.files, |key| w.out.bytes(key));
        let mut cat = w.pdf.catalog(catalog);
        cat.pages(nav.pages);
        if let Some(root) = nav.struct_root {
            cat.pair(Name(b"StructTreeRoot"), root);
            cat.insert(Name(b"MarkInfo"))
                .dict()
                .pair(Name(b"Marked"), true);
            // Viewers title the window with the document's title.
            if doc.meta.title.is_some() {
                cat.insert(Name(b"ViewerPreferences"))
                    .dict()
                    .pair(Name(b"DisplayDocTitle"), true);
            }
        }
        if !embedded.is_empty() {
            let mut names = cat.insert(Name(b"Names")).dict();
            let mut tree = names.insert(Name(b"EmbeddedFiles")).dict();
            let mut entries = tree.insert(Name(b"Names")).array();
            for (key, r) in &embedded {
                entries.item(Str(key));
                entries.item(*r);
            }
            entries.finish();
            tree.finish();
            names.finish();
            // Associated files (ISO 19005-3, 6.8).
            if self.profile.level().is_some_and(|l| l.part() == 3) {
                cat.insert(Name(b"AF"))
                    .array()
                    .items(nav.files.iter().copied());
            }
        }
        if let Some(form) = nav.form {
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
        if let Some(root) = nav.outline {
            cat.outlines(root);
        }
        if let Some(mode) = page_mode(doc.meta.page_mode, nav.outline.is_some()) {
            cat.page_mode(mode);
        }
        if let Some(layout) = doc.meta.page_layout {
            cat.page_layout(page_layout(layout));
        }
        if let Some(lang) = &lang {
            cat.pair(Name(b"Lang"), Str(lang));
        }
        if encrypted && !self.profile.pdf_2() {
            // Revision 6 of the security handler is extension level 8 of
            // PDF 1.7, and part of PDF 2.0.
            cat.insert(Name(b"Extensions"))
                .dict()
                .insert(Name(b"ADBE"))
                .dict()
                .pair(Name(b"BaseVersion"), Name(b"1.7"))
                .pair(Name(b"ExtensionLevel"), 8);
        }
        if let Some(m) = ua_metadata {
            cat.metadata(m);
        }
        let (Some((level, metadata, profile)), Some((condition, info))) = (pdf_a, intent) else {
            cat.finish();
            if let Some(m) = ua_metadata {
                let xmp = pdfa::xmp(
                    doc.meta,
                    producer,
                    doc.creation_date,
                    None,
                    self.profile.ua_part(),
                );
                w.out.metadata(&mut w.pdf, m, &xmp, |_| {});
            }
            return Ok(());
        };
        cat.metadata(metadata);
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
        let xmp = pdfa::xmp(
            doc.meta,
            producer,
            doc.creation_date,
            Some(level),
            self.profile.ua_part(),
        );
        w.out.metadata(&mut w.pdf, metadata, &xmp, |_| {});
        let icc = deflate(&pdfa::srgb_icc(), 9)?;
        w.out.icc_profile(&mut w.pdf, profile, &icc, |p| {
            p.n(3)
                .range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
                .filter(Filter::FlateDecode);
        });
        // Deterministic: a hash of the page content streams and the XMP
        // metadata, which mirrors the document information.
        let mut h = xxhash_rust::xxh3::Xxh3::new();
        for page in doc.pages {
            h.update(&page.stream);
        }
        h.update(&xmp);
        // Two documents that differ only in their files, or in their
        // names and types, differ in `/ID`.
        for f in doc.files {
            h.update(&f.hash.to_le_bytes());
            for part in [f.name.as_bytes(), f.mime_type.as_bytes()] {
                // As u64: a `usize` has 4 bytes in WebAssembly, and
                // the browser writes the same bytes as Node.
                h.update(&(part.len() as u64).to_le_bytes());
                h.update(part);
            }
        }
        let id = h.digest128().to_be_bytes().to_vec();
        w.pdf.set_file_id((id.clone(), id));
        Ok(())
    }

    /// The document information dictionary; under PDF/A, the same cleaned
    /// values as the XMP packet.
    fn write_info(&self, w: &mut Writer, doc: &DocumentOut<'_>, info: Ref, producer: &str) {
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
                entries.push((key, w.out.text(&text(v))));
            }
        }
        entries.push((b"Producer", w.out.text(&text(producer))));
        if let Some(t) = doc.creation_date {
            entries.push((b"CreationDate", w.out.bytes(pdf_date(t).as_bytes())));
        }
        let mut inf = w.pdf.document_info(info);
        for (key, value) in &entries {
            inf.pair(Name(key), Str(value));
        }
        inf.finish();
    }
}

/// What the catalog points to besides the page tree root `pages`.
struct Nav<'f> {
    pages: Ref,
    form: Option<&'f forms::Form>,
    outline: Option<Ref>,
    /// The file specifications, in declaration order.
    files: &'f [Ref],
    /// The structure tree of tagged output.
    struct_root: Option<Ref>,
}

/// The pages, each naming the shared resources it uses, with its
/// annotations, and their content streams.
fn write_pages(
    w: &mut Writer,
    doc: &DocumentOut<'_>,
    (page_refs, content_refs): (&[Ref], &[Ref]),
    tree: &PageTree,
    res: &Resources,
    annots: &[Vec<Ref>],
) {
    for (i, ((page, page_ref), content_ref)) in doc
        .pages
        .iter()
        .zip(page_refs)
        .zip(content_refs)
        .enumerate()
    {
        let parent = tree.leaf_parents[i / PAGE_TREE_FANOUT];
        let mut p = w.pdf.page(*page_ref);
        p.parent(parent)
            .media_box(PdfRect::new(
                0.0,
                0.0,
                round_pt(page.width),
                round_pt(page.height),
            ))
            .contents(*content_ref);
        let mut r = p.resources();
        if !page.opacities.is_empty() {
            let mut gs = r.ext_g_states();
            for key in &page.opacities {
                gs.pair(Name(gs_name(*key).as_bytes()), res.opacities[key]);
            }
        }
        if !page.glyphs.is_empty() || !page.type3.is_empty() {
            let mut f = r.fonts();
            for key in page.glyphs.keys() {
                f.pair(Name(font_name(*key).as_bytes()), res.fonts[key]);
            }
            for index in &page.type3 {
                f.pair(Name(type3_name(*index).as_bytes()), res.type3[index]);
            }
        }
        let placed = res.placements.get(i).map_or(&[][..], Vec::as_slice);
        if !page.images.is_empty() || !placed.is_empty() {
            let mut x = r.x_objects();
            for key in &page.images {
                x.pair(Name(image_name(*key).as_bytes()), res.images[key]);
            }
            for (n, form) in placed.iter().enumerate() {
                x.pair(Name(placement_name(n).as_bytes()), *form);
            }
        }
        r.finish();
        if !annots[i].is_empty() {
            p.annotations(annots[i].iter().copied());
        }
        if page.mcids.is_some() {
            p.pair(Name(b"StructParents"), i32::try_from(i).unwrap_or(i32::MAX));
            if !annots[i].is_empty() {
                // Annotations are visited in the order of the structure.
                p.pair(Name(b"Tabs"), Name(b"S"));
            }
        }
        p.finish();

        w.out
            .stream(&mut w.pdf, *content_ref, &page.stream, |stream| {
                if page.compressed {
                    stream.filter(Filter::FlateDecode);
                }
            });
    }
}

/// The Type 3 fonts of colour glyphs the pages use, by index.
fn type3_used(doc: &DocumentOut<'_>) -> impl Iterator<Item = u16> {
    doc.pages
        .iter()
        .flat_map(|p| p.type3.iter().copied())
        .collect::<BTreeSet<u16>>()
        .into_iter()
}

/// The page mode the document opens in: by default, with the bookmarks
/// shown when it has some.
fn page_mode(mode: Option<schema::PageMode>, outline: bool) -> Option<PageMode> {
    match mode {
        None if outline => Some(PageMode::UseOutlines),
        None => None,
        Some(schema::PageMode::UseNone) => Some(PageMode::UseNone),
        Some(schema::PageMode::UseOutlines) => Some(PageMode::UseOutlines),
        Some(schema::PageMode::UseThumbs) => Some(PageMode::UseThumbs),
        Some(schema::PageMode::FullScreen) => Some(PageMode::FullScreen),
        Some(schema::PageMode::UseAttachments) => Some(PageMode::UseAttachments),
    }
}

fn page_layout(layout: schema::PageLayout) -> PageLayout {
    match layout {
        schema::PageLayout::SinglePage => PageLayout::SinglePage,
        schema::PageLayout::OneColumn => PageLayout::OneColumn,
        schema::PageLayout::TwoColumnLeft => PageLayout::TwoColumnLeft,
        schema::PageLayout::TwoColumnRight => PageLayout::TwoColumnRight,
        schema::PageLayout::TwoPageLeft => PageLayout::TwoPageLeft,
        schema::PageLayout::TwoPageRight => PageLayout::TwoPageRight,
    }
}

/// A PDF date string (`D:YYYYMMDDHHmmSS` and `Z` or `±HH'mm`). Written by
/// hand: `pdf_writer::Date` takes the offset's sign from its hours, so an
/// offset under an hour west of UTC (`-00:30`) would come out east of it.
pub(super) fn pdf_date(t: Timestamp) -> String {
    let (d, zone) = (t.local, t.zone('\''));
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}{zone}",
        d.year, d.month, d.day, d.hour, d.minute, d.second
    )
}

/// Page tree: flat when small, otherwise intermediate nodes of at most
/// [`PAGE_TREE_FANOUT`] kids.
pub(super) struct PageTree {
    root: Ref,
    /// Parent of pages `[i * FANOUT, (i + 1) * FANOUT)`.
    leaf_parents: Vec<Ref>,
}

pub(super) fn write_page_tree(w: &mut Writer, pages: &[Ref]) -> PageTree {
    let Writer { pdf, alloc, .. } = w;
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
