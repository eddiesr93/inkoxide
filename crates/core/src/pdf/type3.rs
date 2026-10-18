//! Colour glyphs as Type 3 fonts: each glyph's procedure (`d0`, so that it
//! keeps its colours) draws its image over the box it fills, in font
//! units. A `ToUnicode` map gives each code the text it was first drawn
//! with, and tagged output maps a code drawn without text to the
//! combining grapheme joiner, as it does the other fonts' glyphs.
//!
//! Code 0 draws nothing and is half an em wide. Text extractors cannot
//! tell the size of a Type 3 font's glyphs without drawing them, so some
//! (Poppler) take the width of the first glyph for half an em; without it,
//! an emoji is read as text twice its size, and set apart from its line.

use std::collections::BTreeMap;

use pdf_writer::types::{FontFlags, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Rect, Ref};

use super::font::{identity, system_info, IDENTITY};
use super::{content_bytes, deflate, image_name, round_to, Profile, Writer};
use crate::flate;
use crate::fonts::LoadedFont;
use crate::images::{image_matrix, ImageKey, ImageSet};
use crate::text::Type3Font;
use crate::InkoxideError;

/// What a Type 3 font is written with.
pub(super) struct Type3<'a, 'f> {
    pub glyphs: &'a Type3Font,
    /// Its font, whose glyphs it draws.
    pub font: &'a LoadedFont<'f>,
    pub index: u16,
    pub images: &'a ImageSet,
    pub image_refs: &'a BTreeMap<ImageKey, Ref>,
    /// The text of each code drawn with text.
    pub text: &'a BTreeMap<u8, String>,
    pub tagged: bool,
}

/// Writes Type 3 font `t` and its glyph procedures; returns the font's
/// reference.
pub(super) fn write_type3(
    w: &mut Writer,
    t: &Type3<'_, '_>,
    profile: Profile,
) -> Result<Ref, InkoxideError> {
    let Writer {
        pdf, alloc, out, ..
    } = w;
    let font_ref = alloc.next();
    let descriptor = alloc.next();
    let cmap_ref = alloc.next();
    // Code 0, then the glyphs.
    let procs: Vec<Ref> = (0..=t.glyphs.glyphs.len()).map(|_| alloc.next()).collect();
    let m = t.font.metrics();
    let upem = m.units_per_em;
    // Glyph space is font units.
    let bbox = t
        .glyphs
        .glyphs
        .iter()
        .fold([0.0f32, 0.0, 0.0, 0.0], |[a, b, c, d], g| {
            let [x0, y0, x1, y1] = g.bbox;
            [a.min(x0), b.min(y0), c.max(x1), d.max(y1)]
        });
    let bbox = Rect::new(
        bbox[0].floor(),
        bbox[1].floor(),
        bbox[2].ceil(),
        bbox[3].ceil(),
    );
    let units = |v: f32| round_to(v, 1e3);
    let name = format!("{}-Color{}", m.post_script_name, t.index);
    let last = u8::try_from(t.glyphs.glyphs.len()).unwrap_or(u8::MAX);
    let half = units(upem / 2.0);

    let mut font = pdf.type3_font(font_ref);
    font.name(Name(name.as_bytes()))
        .bbox(bbox)
        .matrix([1.0 / upem, 0.0, 0.0, 1.0 / upem, 0.0, 0.0])
        .first_char(0)
        .last_char(last)
        .widths(std::iter::once(half).chain(t.glyphs.glyphs.iter().map(|g| units(g.advance))))
        .font_descriptor(descriptor)
        .to_unicode(cmap_ref);
    {
        let mut char_procs = font.char_procs();
        for (code, r) in procs.iter().enumerate() {
            char_procs.pair(Name(format!("g{code}").as_bytes()), *r);
        }
    }
    font.encoding_custom().differences().consecutive(
        0,
        (0..procs.len())
            .map(|code| format!("g{code}"))
            .collect::<Vec<_>>()
            .iter()
            .map(|n| Name(n.as_bytes())),
    );
    {
        let mut res = font.resources();
        let mut x = res.x_objects();
        for g in &t.glyphs.glyphs {
            x.pair(Name(image_name(g.image).as_bytes()), t.image_refs[&g.image]);
        }
    }
    font.finish();

    pdf.font_descriptor(descriptor)
        .name(Name(name.as_bytes()))
        .flags(FontFlags::SYMBOLIC)
        .bbox(bbox)
        .italic_angle(0.0)
        .ascent(units(m.ascender))
        .descent(units(m.descender))
        .cap_height(units(m.cap_height))
        .stem_v(0.0);

    let mut empty = Content::new();
    empty.start_color_glyph(half);
    let data = deflate(&content_bytes(empty, profile)?, flate::LEVEL)?;
    out.stream(pdf, procs[0], &data, |s| {
        s.filter(Filter::FlateDecode);
    });
    for (g, r) in t.glyphs.glyphs.iter().zip(&procs[1..]) {
        let [x0, y0, x1, y1] = g.bbox;
        let (w, h) = (x1 - x0, y1 - y0);
        // The image's matrix in a y-down box, turned y up.
        let orientation = t.images.get(g.image).orientation;
        let [a, b, c, d, e, f] = image_matrix(orientation, crate::geom::Rect::new(x0, -y1, w, h));
        let mut content = Content::new();
        content.start_color_glyph(units(g.advance));
        content.save_state();
        content.transform([a, -b, c, -d, e, -f].map(units));
        content.x_object(Name(image_name(g.image).as_bytes()));
        content.restore_state();
        let data = deflate(&content_bytes(content, profile)?, flate::LEVEL)?;
        out.stream(pdf, *r, &data, |s| {
            s.filter(Filter::FlateDecode);
        });
    }

    let mut cmap = UnicodeCmap::<u8>::new(Name(b"Custom"), IDENTITY);
    for code in 1..=last {
        match t.text.get(&code).filter(|s| !s.is_empty()) {
            Some(text) => cmap.pair_with_multiple(code, text.chars()),
            None if t.tagged => cmap.pair(code, '\u{34F}'),
            None => {}
        }
    }
    let info = identity(out);
    out.cmap(pdf, cmap_ref, &cmap.finish(), |c| {
        c.name(Name(b"Custom"));
        system_info(c.insert(Name(b"CIDSystemInfo")), &info);
    });
    Ok(font_ref)
}
