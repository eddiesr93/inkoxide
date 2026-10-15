//! Icon annotations: one per painted icon op. A text note is drawn as a
//! sticky-note icon by an appearance stream of its own (PDF/A-2b requires
//! one, and it looks the same in every viewer).

use pdf_writer::types::{AnnotationFlags, AnnotationIcon, AnnotationType};
use pdf_writer::{Content, Finish, Name, Pdf, Rect as PdfRect, Ref, Str};

use super::{annot_rect, content_bytes, path, q, set_fill, EncodedPage, Out, Profile, RefAlloc};
use crate::icons::{IconKind, ICON};
use crate::paint::{rounded_rect, Rect};
use crate::schema::{Color, Note, NOTE_YELLOW};
use crate::InkoxideError;

/// Writes the icon annotations of every page, in page order; appends
/// their refs to `annots` (per page).
pub(super) fn write_icons(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    out: &mut Out,
    (pages, page_refs): (&[EncodedPage], &[Ref]),
    annots: &mut [Vec<Ref>],
    profile: Profile,
) -> Result<(), InkoxideError> {
    for (i, page) in pages.iter().enumerate() {
        for icon in &page.icons {
            let r = alloc.next();
            let ap = alloc.next();
            // Inside the page, where viewers show it.
            let x = icon.x.min(page.width - ICON).max(0.0);
            let y = icon.y.min(page.height - ICON).max(0.0);
            let rect = annot_rect(Rect::new(x, y, ICON, ICON), page.height);
            match &icon.kind {
                IconKind::Note(note) => {
                    write_note(pdf, out, (r, ap), note, (rect, page_refs[i]), profile)?;
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
    profile: Profile,
) -> Result<(), InkoxideError> {
    let color = note.color.unwrap_or(NOTE_YELLOW);
    let contents = out.text(&note.contents);
    let author = note.author.as_deref().map(|a| out.text(a));
    let mut a = pdf.annotation(r);
    a.subtype(AnnotationType::Text).rect(rect).page(page_ref);
    a.pair(Name(b"Contents"), Str(&contents));
    a.color_rgb(q(color[0]), q(color[1]), q(color[2]))
        .flags(AnnotationFlags::PRINT | AnnotationFlags::NO_ZOOM | AnnotationFlags::NO_ROTATE)
        .icon(AnnotationIcon::Note);
    if let Some(author) = &author {
        a.pair(Name(b"T"), Str(author));
    }
    if note.open {
        a.pair(Name(b"Open"), true);
    }
    a.appearance().normal().stream(ap);
    a.finish();
    let content = icon(&color, profile)?;
    out.form_xobject(pdf, ap, &content, |x| {
        x.bbox(PdfRect::new(0.0, 0.0, ICON, ICON));
    });
    Ok(())
}

/// A sticky note in `color`: a rounded square with a darker outline and
/// three lines of "text".
fn icon(color: &Color, profile: Profile) -> Result<Vec<u8>, InkoxideError> {
    let dark = color.map(|v| q(v * 0.45));
    let mut c = Content::new();
    set_fill(&mut c, color);
    c.set_stroke_rgb(dark[0], dark[1], dark[2]);
    c.set_line_width(1.0);
    let body = rounded_rect(
        Rect::new(1.0, 1.0, ICON - 2.0, ICON - 2.0),
        &[(3.0, 3.0); 4],
    );
    path(&mut c, &body.segs);
    c.fill_nonzero_and_stroke();
    c.set_line_width(1.25);
    for y in [13.5, 10.0, 6.5] {
        c.move_to(5.0, y);
        c.line_to(ICON - 5.0, y);
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
    fn pdf_a_keeps_the_appearance_and_the_print_flag() {
        let pdf = render("", &note(r#"{"contents":"x"}"#), 1, true);
        assert!(pdf.contains("/F 28") && pdf.contains("/AP <<"), "{pdf}");
    }
}
