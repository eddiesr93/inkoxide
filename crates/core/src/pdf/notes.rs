//! Icon annotations: one per painted icon op. A text note is drawn as a
//! sticky note, a speech bubble, a key, a question mark, a new paragraph,
//! a paragraph sign or a caret, and a file attachment as a push pin, a
//! paper clip, a graph or a tag, each by an appearance stream of its own
//! (PDF/A-2b requires one, and it looks the same in every viewer).

use pdf_writer::types::{AnnotationFlags, AnnotationIcon, AnnotationType};
use pdf_writer::{Content, Finish, Name, Pdf, Rect as PdfRect, Ref, Str};

use super::attach::FileRefs;
use super::tag::{self, AnnotKind, AnnotTags};
use super::{
    annot_rect, content_bytes, path, round_pt, set_fill, EncodedPage, Out, Profile, Writer,
};
use crate::geom::Rect;
use crate::icons::{IconKind, ICON};
use crate::paint::rounded_rect;
use crate::schema::{Attachment, Color, FileIcon, Note, NoteIcon, ATTACHMENT_BLUE, NOTE_YELLOW};
use crate::InkoxideError;

/// Writes the icon annotations of every page, in page order; appends
/// their refs to `annots` (per page).
pub(super) fn write_icons(
    w: &mut Writer,
    (pages, page_refs): (&[EncodedPage], &[Ref]),
    annots: &mut [Vec<Ref>],
    files: &FileRefs<'_>,
    mut tags: Option<&mut AnnotTags>,
    profile: Profile,
) -> Result<(), InkoxideError> {
    let Writer { pdf, alloc, out } = w;
    for (i, page) in pages.iter().enumerate() {
        for icon in &page.icons {
            let r = alloc.next();
            let ap = alloc.next();
            let key = tags
                .as_deref_mut()
                .map(|t| t.add(r, i, icon.owner, AnnotKind::Other));
            // Inside the page, where viewers show it.
            let x = icon.x.min(page.width - ICON).max(0.0);
            let y = icon.y.min(page.height - ICON).max(0.0);
            let rect = annot_rect(Rect::new(x, y, ICON, ICON), page.height);
            match &icon.kind {
                IconKind::Note(note) => {
                    write_note(pdf, out, (r, ap), note, (rect, page_refs[i]), key, profile)?;
                }
                IconKind::Attachment(a) => {
                    // Validation checked that the file is declared.
                    let Some(&(spec, file)) = files.by_name.get(a.file.as_str()) else {
                        continue;
                    };
                    let contents = a
                        .contents
                        .as_deref()
                        .or(file.description.as_deref())
                        .unwrap_or(&file.name);
                    let annot = FileAnnot {
                        refs: (r, ap),
                        spec,
                        contents,
                        rect,
                        page: page_refs[i],
                        struct_parent: key,
                    };
                    write_attachment(pdf, out, &annot, a, profile)?;
                }
            }
            annots[i].push(r);
        }
    }
    Ok(())
}

/// Writes `note` as annotation `r` at `rect`, with appearance `ap`.
fn write_note(
    pdf: &mut Pdf,
    out: &mut Out,
    (r, ap): (Ref, Ref),
    note: &Note,
    (rect, page_ref): (PdfRect, Ref),
    struct_parent: Option<i32>,
    profile: Profile,
) -> Result<(), InkoxideError> {
    let color = note.color.unwrap_or(NOTE_YELLOW);
    let contents = out.text(&note.contents);
    let author = note.author.as_deref().map(|a| out.text(a));
    let mut a = pdf.annotation(r);
    a.subtype(AnnotationType::Text).rect(rect).page(page_ref);
    a.pair(Name(b"Contents"), Str(&contents));
    a.color_rgb(round_pt(color[0]), round_pt(color[1]), round_pt(color[2]))
        .flags(AnnotationFlags::PRINT | AnnotationFlags::NO_ZOOM | AnnotationFlags::NO_ROTATE)
        .icon(match note.icon {
            NoteIcon::Note => AnnotationIcon::Note,
            NoteIcon::Comment => AnnotationIcon::Comment,
            NoteIcon::Key => AnnotationIcon::Key,
            NoteIcon::Help => AnnotationIcon::Help,
            NoteIcon::NewParagraph => AnnotationIcon::NewParagraph,
            NoteIcon::Paragraph => AnnotationIcon::Paragraph,
            NoteIcon::Insert => AnnotationIcon::Insert,
        });
    if let Some(author) = &author {
        a.pair(Name(b"T"), Str(author));
    }
    if note.open {
        a.pair(Name(b"Open"), true);
    }
    tag::struct_parent(&mut a, struct_parent);
    a.appearance().normal().stream(ap);
    a.finish();
    let content = icon(note.icon, &color, profile)?;
    out.form_xobject(pdf, ap, &content, |x| {
        x.bbox(PdfRect::new(0.0, 0.0, ICON, ICON));
    });
    Ok(())
}

/// Where a file attachment annotation goes and what it opens.
struct FileAnnot<'a> {
    /// The annotation and its appearance.
    refs: (Ref, Ref),
    /// The file specification it opens.
    spec: Ref,
    contents: &'a str,
    rect: PdfRect,
    page: Ref,
    /// Its key in the parent tree, in tagged output.
    struct_parent: Option<i32>,
}

/// Writes attachment icon `a` as file attachment annotation `annot`.
fn write_attachment(
    pdf: &mut Pdf,
    out: &mut Out,
    annot: &FileAnnot<'_>,
    a: &Attachment,
    profile: Profile,
) -> Result<(), InkoxideError> {
    let (r, ap) = annot.refs;
    let color = a.color.unwrap_or(ATTACHMENT_BLUE);
    let contents = out.text(annot.contents);
    let mut w = pdf.annotation(r);
    w.subtype(AnnotationType::FileAttachment)
        .rect(annot.rect)
        .page(annot.page);
    w.pair(Name(b"Contents"), Str(&contents));
    w.color_rgb(round_pt(color[0]), round_pt(color[1]), round_pt(color[2]))
        .flags(AnnotationFlags::PRINT | AnnotationFlags::NO_ZOOM | AnnotationFlags::NO_ROTATE)
        .icon(match a.icon {
            FileIcon::PushPin => AnnotationIcon::PushPin,
            FileIcon::Paperclip => AnnotationIcon::Paperclip,
            FileIcon::Graph => AnnotationIcon::Graph,
            FileIcon::Tag => AnnotationIcon::Tag,
        });
    w.pair(Name(b"FS"), annot.spec);
    w.insert(Name(b"AF")).array().item(annot.spec);
    tag::struct_parent(&mut w, annot.struct_parent);
    w.appearance().normal().stream(ap);
    w.finish();
    let content = file_icon(a.icon, &color, profile)?;
    out.form_xobject(pdf, ap, &content, |x| {
        x.bbox(PdfRect::new(0.0, 0.0, ICON, ICON));
    });
    Ok(())
}

/// The body of an icon in `color`: a rounded square with a darker
/// outline, and the stroke colour set to the outline's for what is drawn
/// on it.
fn body(c: &mut Content, color: &Color) {
    let dark = color.map(|v| round_pt(v * 0.45));
    set_fill(c, color);
    c.set_stroke_rgb(dark[0], dark[1], dark[2]);
    c.set_line_width(1.0);
    let body = rounded_rect(
        Rect::new(1.0, 1.0, ICON - 2.0, ICON - 2.0),
        &[(3.0, 3.0); 4],
    );
    path(c, &body.segs);
    c.fill_nonzero_and_stroke();
}

/// A note's icon in `color`: the body, and on it three lines of "text",
/// a speech bubble, a key, a question mark, a triangle over a line, a
/// paragraph sign or a caret.
fn icon(icon: NoteIcon, color: &Color, profile: Profile) -> Result<Vec<u8>, InkoxideError> {
    let mut c = Content::new();
    body(&mut c, color);
    c.set_line_width(1.25);
    match icon {
        NoteIcon::Note => {
            for y in [13.5, 10.0, 6.5] {
                c.move_to(5.0, y);
                c.line_to(ICON - 5.0, y);
            }
        }
        NoteIcon::Comment => {
            c.move_to(5.0, 15.0);
            c.line_to(15.0, 15.0);
            c.line_to(15.0, 8.0);
            c.line_to(9.5, 8.0);
            c.line_to(6.5, 4.5);
            c.line_to(7.0, 8.0);
            c.line_to(5.0, 8.0);
            c.close_path();
        }
        NoteIcon::Key => {
            // A ring, a shaft and two teeth.
            let k = 2.5 * crate::paint::KAPPA;
            c.move_to(9.5, 12.5);
            c.cubic_to(9.5, 12.5 + k, 7.0 + k, 15.0, 7.0, 15.0);
            c.cubic_to(7.0 - k, 15.0, 4.5, 12.5 + k, 4.5, 12.5);
            c.cubic_to(4.5, 12.5 - k, 7.0 - k, 10.0, 7.0, 10.0);
            c.cubic_to(7.0 + k, 10.0, 9.5, 12.5 - k, 9.5, 12.5);
            c.move_to(8.8, 10.7);
            c.line_to(15.0, 4.5);
            c.move_to(13.0, 6.5);
            c.line_to(14.75, 8.25);
            c.move_to(11.0, 8.5);
            c.line_to(12.5, 10.0);
        }
        NoteIcon::Help => {
            c.move_to(7.0, 12.5);
            c.cubic_to(7.0, 16.5, 13.0, 16.5, 13.0, 12.5);
            c.cubic_to(13.0, 10.5, 10.0, 10.5, 10.0, 8.0);
            c.move_to(10.0, 6.25);
            c.line_to(10.0, 4.75);
        }
        NoteIcon::NewParagraph => {
            c.move_to(10.0, 15.5);
            c.line_to(6.0, 9.0);
            c.line_to(14.0, 9.0);
            c.close_path();
            c.move_to(6.0, 6.0);
            c.line_to(14.0, 6.0);
        }
        NoteIcon::Paragraph => {
            // A bowl on two stems.
            c.move_to(14.5, 15.0);
            c.line_to(8.5, 15.0);
            c.cubic_to(5.5, 15.0, 5.5, 10.5, 8.5, 10.5);
            c.line_to(11.0, 10.5);
            c.move_to(11.0, 15.0);
            c.line_to(11.0, 4.5);
            c.move_to(13.5, 15.0);
            c.line_to(13.5, 4.5);
        }
        NoteIcon::Insert => {
            c.move_to(5.5, 5.5);
            c.line_to(10.0, 14.5);
            c.line_to(14.5, 5.5);
        }
    }
    c.stroke();
    content_bytes(c, profile)
}

/// An attachment icon in `color`: the body, and a push pin, a paper clip,
/// a bar chart or a tag drawn on it.
fn file_icon(icon: FileIcon, color: &Color, profile: Profile) -> Result<Vec<u8>, InkoxideError> {
    let mut c = Content::new();
    body(&mut c, color);
    c.set_line_width(1.25);
    match icon {
        FileIcon::PushPin => {
            // A round head, a collar and a needle.
            let k = 2.5 * crate::paint::KAPPA;
            c.move_to(12.5, 13.5);
            c.cubic_to(12.5, 13.5 + k, 10.0 + k, 16.0, 10.0, 16.0);
            c.cubic_to(10.0 - k, 16.0, 7.5, 13.5 + k, 7.5, 13.5);
            c.cubic_to(7.5, 13.5 - k, 10.0 - k, 11.0, 10.0, 11.0);
            c.cubic_to(10.0 + k, 11.0, 12.5, 13.5 - k, 12.5, 13.5);
            c.move_to(6.5, 10.0);
            c.line_to(13.5, 10.0);
            c.move_to(10.0, 10.0);
            c.line_to(10.0, 4.0);
        }
        FileIcon::Paperclip => {
            c.move_to(12.0, 7.0);
            c.line_to(12.0, 13.5);
            c.cubic_to(12.0, 16.5, 7.5, 16.5, 7.5, 13.5);
            c.line_to(7.5, 5.5);
            c.cubic_to(7.5, 3.5, 10.5, 3.5, 10.5, 5.5);
            c.line_to(10.5, 12.5);
        }
        FileIcon::Graph => {
            c.move_to(5.0, 15.5);
            c.line_to(5.0, 5.0);
            c.line_to(15.5, 5.0);
            for (x, h) in [(8.0, 4.0), (11.0, 7.0), (14.0, 9.5)] {
                c.move_to(x, 5.0);
                c.line_to(x, 5.0 + h);
            }
        }
        FileIcon::Tag => {
            // A label with a cut corner and a hole.
            c.move_to(5.0, 9.0);
            c.line_to(5.0, 15.0);
            c.line_to(11.0, 15.0);
            c.line_to(15.5, 10.5);
            c.line_to(9.5, 4.5);
            c.close_path();
            c.move_to(8.5, 12.5);
            c.line_to(8.5, 11.5);
        }
    }
    c.stroke();
    content_bytes(c, profile)
}

#[cfg(test)]
mod tests {
    /// The PDF (uncompressed, as text) of `pages` pages of 200 × 200 pt,
    /// with `header` and `children`.
    fn render(header: &str, children: &str, pages: usize, pdf_a: bool) -> String {
        let header = if header.is_empty() {
            String::new()
        } else {
            format!(r#""header":{header},"#)
        };
        let page =
            format!(r#"{{"size":{{"width":200,"height":200}},{header}"children":[{children}]}}"#);
        let doc = format!(
            r#"{{"version":1,"pages":[{}]}}"#,
            vec![page; pages].join(",")
        );
        let options = crate::RenderOptions::default()
            .with_compress(false)
            .with_pdf_a(pdf_a);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &options,
            &crate::FontStore::new(),
            &crate::ImageStore::new(),
        )
        .unwrap();
        String::from_utf8_lossy(&pdf).into_owned()
    }

    fn note(json: &str) -> String {
        format!(r#"{{"type":"note","note":{json}}}"#)
    }

    /// The `/Rect` of every annotation, in order.
    fn rects(pdf: &str) -> Vec<[f32; 4]> {
        pdf.split("/Rect [")
            .skip(1)
            .map(|s| {
                let v: Vec<f32> = s
                    .split(']')
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .map(|n| n.parse().unwrap())
                    .collect();
                [v[0], v[1], v[2], v[3]]
            })
            .collect()
    }

    #[test]
    fn a_note_is_a_text_annotation_with_an_icon() {
        let pdf = render(
            "",
            &note(r#"{"contents":"Check (this)","author":"Ana","open":true}"#),
            1,
            false,
        );
        for entry in [
            "/Subtype /Text",
            "/Contents (Check (this))",
            "/T (Ana)",
            "/Name /Note",
            "/C [1 0.82 0.2]",
            "/Open true",
            "/F 28",
            "/AP <<",
        ] {
            assert!(pdf.contains(entry), "{entry}: {pdf}");
        }
        assert_eq!(rects(&pdf), [[0.0, 180.0, 20.0, 200.0]]);
        // The icon is in the appearance stream, in the note's colour.
        assert!(pdf.contains("/BBox [0 0 20 20]"), "{pdf}");
        assert!(pdf.contains("1 0.82 0.2 rg"), "{pdf}");
        // Closed and anonymous by default, in its own colour.
        let pdf = render("", &note(r#"{"contents":"x","color":[0,0,1,1]}"#), 1, false);
        assert!(!pdf.contains("/Open") && !pdf.contains("/T ("), "{pdf}");
        assert!(pdf.contains("/C [0 0 1]"), "{pdf}");
    }

    #[test]
    fn notes_take_no_space_and_stay_inside_the_page() {
        // Between two boxes, which touch.
        let pdf = render(
            "",
            &[
                r#"{"type":"box","style":{"height":50}}"#.to_string(),
                note(r#"{"contents":"x"}"#),
                r#"{"type":"box","link":"https://a.example","style":{"height":10}}"#.to_string(),
            ]
            .join(","),
            1,
            false,
        );
        assert_eq!(
            rects(&pdf),
            [[0.0, 140.0, 200.0, 150.0], [0.0, 130.0, 20.0, 150.0]]
        );
        // At the bottom right corner, the icon is moved in.
        let pdf = render(
            "",
            r#"{"type":"note","style":{"position":"absolute","inset":{"left":195,"top":195}},"note":{"contents":"x"}}"#,
            1,
            false,
        );
        assert_eq!(rects(&pdf), [[180.0, 0.0, 200.0, 20.0]]);
    }

    #[test]
    fn a_note_has_the_icon_it_names_drawn_its_own_way() {
        let mut drawn = std::collections::BTreeSet::new();
        for (icon, name) in [
            ("note", "Note"),
            ("comment", "Comment"),
            ("key", "Key"),
            ("help", "Help"),
            ("newParagraph", "NewParagraph"),
            ("paragraph", "Paragraph"),
            ("insert", "Insert"),
        ] {
            let pdf = render(
                "",
                &note(&format!(r#"{{"contents":"x","icon":"{icon}"}}"#)),
                1,
                true,
            );
            assert!(pdf.contains(&format!("/Name /{name}")), "{pdf}");
            let stream = pdf
                .split("stream\n")
                .nth(1)
                .unwrap()
                .split("endstream")
                .next()
                .unwrap();
            drawn.insert(stream.to_string());
        }
        assert_eq!(drawn.len(), 7);
    }

    #[test]
    fn a_note_after_a_break_or_on_the_bottom_edge_is_on_one_page() {
        let full = r#"{"type":"box","style":{"height":200}}"#;
        let count = |pdf: &str| pdf.matches("/Subtype /Text").count();
        // On the bottom edge of the last page.
        let pdf = render(
            "",
            &format!("{full},{}", note(r#"{"contents":"x"}"#)),
            1,
            false,
        );
        assert_eq!(count(&pdf), 1);
        assert_eq!(rects(&pdf), [[0.0, 0.0, 20.0, 20.0]]);
        // At the top of the next page, before more content.
        let pdf = render(
            "",
            &format!("{full},{},{full}", note(r#"{"contents":"x"}"#)),
            1,
            false,
        );
        assert_eq!(count(&pdf), 1);
        assert_eq!(pdf.matches("/Type /Page\n").count(), 2, "{pdf}");
        assert_eq!(rects(&pdf), [[0.0, 180.0, 20.0, 200.0]]);
        // After a forced break.
        let pdf = render(
            "",
            &format!(
                r#"{{"type":"box","style":{{"height":20}}}},{{"type":"pageBreak"}},{},{{"type":"box","style":{{"height":20}}}}"#,
                note(r#"{"contents":"x"}"#)
            ),
            1,
            false,
        );
        assert_eq!(count(&pdf), 1);
    }

    #[test]
    fn a_note_in_a_header_is_on_every_page() {
        let pdf = render(&note(r#"{"contents":"x"}"#), "", 3, false);
        assert_eq!(pdf.matches("/Subtype /Text").count(), 3);
    }

    #[test]
    fn a_transformed_note_moves_its_corner_and_stays_upright() {
        let pdf = render(
            "",
            &format!(
                r#"{{"type":"box","style":{{"width":100,"height":40,"transform":[{{"translate":[50,60]}},{{"rotate":90}}]}},"children":[{}]}}"#,
                note(r#"{"contents":"x"}"#)
            ),
            1,
            false,
        );
        // The top-left corner (0, 0) turns a quarter around (50, 20), then
        // moves by (50, 60): to (120, 30).
        assert_eq!(rects(&pdf), [[120.0, 150.0, 140.0, 170.0]]);
    }

    /// The PDF (uncompressed, as text) of `pages` pages of 200 × 200 pt in
    /// Inter, with header `header` and `children`, and its pages' content
    /// streams.
    fn render_text(header: &str, children: &str, pages: usize) -> (String, Vec<String>) {
        let header = if header.is_empty() {
            String::new()
        } else {
            format!(r#""header":{header},"#)
        };
        let page =
            format!(r#"{{"size":{{"width":200,"height":200}},{header}"children":[{children}]}}"#);
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{}]}}"#,
            vec![page; pages].join(",")
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
        let pdf = String::from_utf8_lossy(&pdf).into_owned();
        let streams = pdf
            .split("stream\n")
            .skip(1)
            .filter_map(|s| s.split("\nendstream").next())
            .filter(|s| s.contains(" Tf"))
            .map(String::from)
            .collect();
        (pdf, streams)
    }

    /// A text node of runs in Inter, 10 pt, `width` wide.
    fn text(width: u32, runs: &[&str]) -> String {
        let runs: Vec<String> = runs
            .iter()
            .map(|r| match r.strip_prefix('@') {
                Some(contents) => format!(
                    r#"{{"note":{{"contents":"{contents}"}},"font":"Inter-Regular","size":10}}"#
                ),
                None => format!(r#"{{"text":"{r}","font":"Inter-Regular","size":10}}"#),
            })
            .collect();
        format!(
            r#"{{"type":"text","style":{{"width":{width}}},"runs":[{}]}}"#,
            runs.join(",")
        )
    }

    #[test]
    fn a_note_in_text_hangs_from_the_line_where_it_is() {
        let (pdf, streams) = render_text("", &text(200, &["ab", "@See", "cd"]), 1);
        assert_eq!(pdf.matches("/Subtype /Text").count(), 1, "{pdf}");
        assert!(pdf.contains("/Contents (See)"), "{pdf}");
        let [[x0, y0, x1, y1]] = rects(&pdf)[..] else {
            panic!("{pdf}");
        };
        // After `ab`, at the line's top.
        assert!(x0 > 5.0 && x0 < 15.0, "{x0}");
        assert_eq!((x1 - x0, y0, y1), (20.0, 180.0, 200.0));
        // It takes no room and draws nothing in the text.
        let (_, plain) = render_text("", &text(200, &["ab", "cd"]), 1);
        assert_eq!(streams, plain);
    }

    #[test]
    fn a_note_in_text_stays_with_its_word() {
        // `aaaa bbbb` does not fit on one line.
        let rect = |runs: &[&str]| {
            let (pdf, _) = render_text("", &text(30, runs), 1);
            rects(&pdf)[0]
        };
        // After a word, it stays at its end on the first line.
        let [x0, y0, ..] = rect(&["aaaa", "@n", " bbbb"]);
        assert!(x0 > 15.0 && y0 == 180.0, "{x0} {y0}");
        // After a space, it goes with the next word, at the start of the
        // second line.
        let [x0, y0, ..] = rect(&["aaaa ", "@n", "bbbb"]);
        assert!(x0 == 0.0 && y0 < 180.0, "{x0} {y0}");
        // The spaces around it collapse to one.
        let (_, two) = render_text("", &text(200, &["a ", "@n", " b"]), 1);
        let (_, one) = render_text("", &text(200, &["a ", "b"]), 1);
        assert_eq!(two, one);
    }

    #[test]
    fn a_note_in_text_is_on_the_page_of_its_line() {
        let count = |pdf: &str| pdf.matches("/Subtype /Text").count();
        // In a header, on every page.
        let (pdf, _) = render_text(&text(200, &["Draft", "@x"]), "", 3);
        assert_eq!(count(&pdf), 3);
        // A line on the second page.
        let full = r#"{"type":"box","style":{"height":195}}"#;
        let (pdf, _) = render_text("", &format!("{full},{}", text(200, &["a", "@x"])), 1);
        assert_eq!(pdf.matches("/Type /Page\n").count(), 2, "{pdf}");
        assert_eq!(count(&pdf), 1);
        assert_eq!(rects(&pdf)[0][3], 200.0);
    }

    #[test]
    fn an_attachment_icon_opens_its_file() {
        let icon = |a: &str| format!(r#"{{"type":"attachment","attachment":{a}}}"#);
        let doc = format!(
            r#"{{"version":1,"attachments":[
                 {{"name":"a.txt","data":"aGk=","description":"Data (raw)"}},
                 {{"name":"b.txt","data":"aGk="}}],
               "pages":[{{"size":{{"width":200,"height":200}},"children":[{},{},{}]}}]}}"#,
            icon(r#"{"file":"a.txt"}"#),
            icon(r#"{"file":"b.txt","icon":"tag","color":[1,0,0,1]}"#),
            icon(r#"{"file":"a.txt","contents":"Open me"}"#),
        );
        let options = crate::RenderOptions::default().with_compress(false);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &options,
            &crate::FontStore::new(),
            &crate::ImageStore::new(),
        )
        .unwrap();
        let pdf = String::from_utf8_lossy(&pdf);
        let annots: Vec<&str> = pdf
            .split("/Subtype /FileAttachment")
            .skip(1)
            .map(|a| a.split("endobj").next().unwrap())
            .collect();
        assert_eq!(annots.len(), 3, "{pdf}");
        // The description, else the file name; blue and a push pin by
        // default; each opens its file specification.
        for (annot, entries) in annots.iter().zip([
            &[
                "/Contents (Data (raw))",
                "/C [0.3 0.55 0.9]",
                "/Name /PushPin",
            ][..],
            &["/Contents (b.txt)", "/C [1 0 0]", "/Name /Tag"],
            &["/Contents (Open me)"],
        ]) {
            for entry in entries.iter().chain(&["/FS ", "/AF [", "/F 28", "/AP <<"]) {
                assert!(annot.contains(entry), "{entry}: {annot}");
            }
        }
        let spec = |annot: &str| {
            annot
                .split("/FS ")
                .nth(1)
                .unwrap()
                .split(" R")
                .next()
                .unwrap()
                .to_string()
        };
        assert_eq!(spec(annots[0]), spec(annots[2]));
        assert_ne!(spec(annots[0]), spec(annots[1]));
    }

    #[test]
    fn pdf_a_keeps_the_appearance_and_the_print_flag() {
        let pdf = render("", &note(r#"{"contents":"x"}"#), 1, true);
        assert!(pdf.contains("/F 28") && pdf.contains("/AP <<"), "{pdf}");
    }
}
