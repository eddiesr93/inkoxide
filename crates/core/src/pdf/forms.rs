//! `AcroForm` output: one field per name, one widget annotation
//! per painted field op, and an appearance stream for every widget state
//! (no `NeedAppearances`, so PDF/A-2b accepts the form).

use std::collections::BTreeMap;

use pdf_writer::types::{AnnotationFlags, FieldFlags, FieldType};
use pdf_writer::writers::Field as FieldWriter;
use pdf_writer::{Content, Finish, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};

use super::{font_name, q, set_fill, show_text, EncodedPage, RefAlloc};
use crate::fonts::{FontKey, FontSet};
use crate::forms::FieldOp;
use crate::paint::{polygon, PathSeg, KAPPA};
use crate::schema::{FieldKind, BLACK};

/// Longest PDF name, in bytes (ISO 32000-1, Annex C; PDF/A-2 holds files
/// to it).
const MAX_NAME: usize = 127;

/// What the catalog's `/AcroForm` needs.
pub(super) struct Form {
    pub fields: Vec<Ref>,
    pub fonts: Vec<FontKey>,
}

/// Writes every field; appends widget refs to `annots` (per page).
/// `None` when the document has no fields.
pub(super) fn write_fields(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    ctx: &Ctx<'_, '_, '_>,
    annots: &mut [Vec<Ref>],
) -> Option<Form> {
    // Widgets grouped by field name, in order of first appearance.
    let mut order: Vec<&str> = Vec::new();
    let mut groups: BTreeMap<&str, Vec<(usize, &FieldOp)>> = BTreeMap::new();
    for (i, page) in ctx.pages.iter().enumerate() {
        for f in &page.fields {
            let name = f.field.name.as_str();
            let g = groups.entry(name).or_default();
            if g.is_empty() {
                order.push(name);
            }
            g.push((i, f));
        }
    }
    if order.is_empty() {
        return None;
    }
    let mut form = Form {
        fields: Vec::new(),
        fonts: Vec::new(),
    };
    for name in order {
        let widgets = &groups[name];
        let states = on_states(widgets);
        let first = widgets[0].1;
        if let Some(k) = first.font {
            if ctx.font_refs.contains_key(&k) && !form.fonts.contains(&k) {
                form.fonts.push(k);
            }
        }
        let field_ref = alloc.next();
        form.fields.push(field_ref);
        if widgets.len() == 1 && first.field.kind != FieldKind::Radio {
            // Field and widget merged in one dictionary.
            let aps = ctx.appearances(pdf, alloc, first, &states.names[0]);
            let mut f = pdf.form_field(field_ref);
            field_entries(&mut f, name, widgets, &states);
            let mut annot = f.into_annotation();
            ctx.widget(&mut annot, widgets[0], &aps);
            annot.finish();
            annots[widgets[0].0].push(field_ref);
            continue;
        }
        let mut kids = Vec::with_capacity(widgets.len());
        for (&(page, op), on) in widgets.iter().zip(&states.names) {
            let aps = ctx.appearances(pdf, alloc, op, on);
            let kid = alloc.next();
            let mut f = pdf.form_field(kid);
            f.parent(field_ref);
            let mut annot = f.into_annotation();
            ctx.widget(&mut annot, (page, op), &aps);
            annot.finish();
            annots[page].push(kid);
            kids.push(kid);
        }
        let mut f = pdf.form_field(field_ref);
        field_entries(&mut f, name, widgets, &states);
        f.children(kids);
        f.finish();
    }
    Some(form)
}

/// The on-state names of a field's widgets, in order.
struct OnStates {
    names: Vec<String>,
    /// The names are the widgets' positions, and `/Opt` holds their
    /// values.
    positions: bool,
}

/// A checkbox is on in state `Yes`, a radio button in the state named
/// after its value. A value that cannot name a state (`Off` is the off
/// state, and a name has at most [`MAX_NAME`] bytes) makes every button of
/// its group use its position instead, with the values in `/Opt`.
fn on_states(widgets: &[(usize, &FieldOp)]) -> OnStates {
    let value = |op: &FieldOp| op.field.value.clone().unwrap_or_default();
    if widgets[0].1.field.kind != FieldKind::Radio {
        return OnStates {
            names: vec!["Yes".to_string(); widgets.len()],
            positions: false,
        };
    }
    let positions = widgets.iter().any(|(_, w)| {
        let v = value(w);
        v == "Off" || v.len() > MAX_NAME
    });
    OnStates {
        names: if positions {
            (0..widgets.len()).map(|i| i.to_string()).collect()
        } else {
            widgets.iter().map(|(_, w)| value(w)).collect()
        },
        positions,
    }
}

/// The field-level entries: type, name, flags, value, default appearance.
fn field_entries(
    f: &mut FieldWriter<'_>,
    name: &str,
    widgets: &[(usize, &FieldOp)],
    states: &OnStates,
) {
    let op = widgets[0].1;
    let spec = &op.field;
    f.partial_name(TextStr(name));
    let mut flags = FieldFlags::empty();
    if spec.read_only {
        flags |= FieldFlags::READ_ONLY;
    }
    if spec.required {
        flags |= FieldFlags::REQUIRED;
    }
    match spec.kind {
        FieldKind::Text => {
            f.field_type(FieldType::Text);
            if spec.multiline {
                flags |= FieldFlags::MULTILINE;
            }
            if spec.password {
                flags |= FieldFlags::PASSWORD;
            }
            if let Some(max) = spec.max_length {
                f.text_max_len(i32::try_from(max).unwrap_or(i32::MAX));
            }
            if let Some(v) = &spec.value {
                if !spec.password {
                    f.text_value(TextStr(v));
                }
            }
        }
        FieldKind::Select => {
            f.field_type(FieldType::Choice);
            flags |= FieldFlags::COMBO;
            f.choice_options()
                .options(spec.options.iter().map(|o| TextStr(o)));
            f.choice_value(spec.value.as_deref().map(TextStr));
        }
        FieldKind::Checkbox => {
            f.field_type(FieldType::Button);
            f.checkbox_value(if spec.checked {
                pdf_writer::types::CheckBoxState::Yes
            } else {
                pdf_writer::types::CheckBoxState::Off
            });
        }
        FieldKind::Radio => {
            f.field_type(FieldType::Button);
            flags |= FieldFlags::RADIO | FieldFlags::NO_TOGGLE_TO_OFF;
            let on = widgets
                .iter()
                .zip(&states.names)
                .find(|((_, w), _)| w.field.checked)
                .map_or("Off", |(_, s)| s.as_str());
            f.radio_value(Name(on.as_bytes()));
            if states.positions {
                f.button_options(
                    widgets
                        .iter()
                        .map(|(_, w)| TextStr(w.field.value.as_deref().unwrap_or(""))),
                );
            }
        }
    }
    if !flags.is_empty() {
        f.field_flags(flags);
    }
    if matches!(spec.kind, FieldKind::Text | FieldKind::Select) {
        f.vartext_default_appearance(Str(default_appearance(op).as_bytes()));
    }
}

/// `/DA`: font, size and colour viewers use for edited text.
fn default_appearance(op: &FieldOp) -> String {
    let c = op.field.color.unwrap_or(BLACK);
    let font = op.font.map_or_else(|| "F0".to_string(), font_name);
    format!(
        "/{font} {} Tf {} {} {} rg",
        q(op.field.size),
        q(c[0]),
        q(c[1]),
        q(c[2])
    )
}

/// What the fields' widgets and appearance streams refer to.
pub(super) struct Ctx<'r, 'f, 'a> {
    pub pages: &'r [EncodedPage],
    pub page_refs: &'r [Ref],
    pub font_refs: &'r BTreeMap<FontKey, Ref>,
    pub fonts: &'f FontSet<'a>,
    /// Writing PDF/A: leave `.notdef` glyphs out of appearance streams, as
    /// page content does.
    pub pdf_a: bool,
}

/// Appearance streams of one widget: `(state, stream)`; the state is
/// `None` for text and select widgets (a single appearance).
type Appearances = Vec<(Option<String>, Ref)>;

impl Ctx<'_, '_, '_> {
    /// The appearance streams of widget `op`, whose on state is `on`
    /// (checkboxes and radio buttons).
    fn appearances(
        &self,
        pdf: &mut Pdf,
        alloc: &mut RefAlloc,
        op: &FieldOp,
        on: &str,
    ) -> Appearances {
        let (w, h) = (op.rect.w, op.rect.h);
        // A transformed widget: its appearance turns and scales with it
        // (widget space is y-up, layout space y-down), and the viewer fits
        // the result to the widget's `/Rect`, which bounds it.
        let matrix = op
            .ctm
            .map(|[a, b, c, d, ..]| [a, -b, -c, d, 0.0, 0.0].map(q))
            .filter(|m| *m != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let mut out = Vec::new();
        let mut form = |pdf: &mut Pdf, content: Vec<u8>, font: Option<FontKey>| -> Ref {
            let r = alloc.next();
            let mut x = pdf.form_xobject(r, &content);
            x.bbox(PdfRect::new(0.0, 0.0, q(w), q(h)));
            if let Some(m) = matrix {
                x.matrix(m);
            }
            if let Some(key) = font {
                if let Some(&fr) = self.font_refs.get(&key) {
                    x.resources()
                        .fonts()
                        .pair(Name(font_name(key).as_bytes()), fr);
                }
            }
            x.finish();
            r
        };
        match op.field.kind {
            FieldKind::Text | FieldKind::Select => {
                let mut c = Content::new();
                c.begin_marked_content(Name(b"Tx"));
                c.save_state();
                c.rect(1.0, 1.0, q((w - 2.0).max(0.0)), q((h - 2.0).max(0.0)));
                c.clip_nonzero();
                c.end_path();
                // Widget space is y-up; the text ops are top-left based.
                c.transform([1.0, 0.0, 0.0, -1.0, 0.0, q(h)]);
                for t in &op.text {
                    show_text(&mut c, t, self.fonts, self.pdf_a);
                }
                c.restore_state();
                c.end_marked_content();
                out.push((None, form(pdf, c.finish().into_vec(), op.font)));
            }
            FieldKind::Checkbox | FieldKind::Radio => {
                let mut c = Content::new();
                set_fill(&mut c, &op.field.color.unwrap_or(BLACK));
                let segs = if op.field.kind == FieldKind::Radio {
                    dot(w, h, op.field.size)
                } else {
                    check(w, h, op.field.size)
                };
                super::path(&mut c, &segs);
                c.fill_nonzero();
                out.push((Some(on.to_string()), form(pdf, c.finish().into_vec(), None)));
                out.push((Some("Off".into()), form(pdf, Vec::new(), None)));
            }
        }
        out
    }

    fn widget(
        &self,
        a: &mut pdf_writer::writers::Annotation<'_>,
        (page, op): (usize, &FieldOp),
        aps: &Appearances,
    ) {
        let r = op.ctm.map_or(op.rect, |m| super::bounds(m, op.rect));
        a.rect(super::annot_rect(r, self.pages[page].height))
            .page(self.page_refs[page])
            .flags(if op.field.read_only {
                AnnotationFlags::PRINT | AnnotationFlags::READ_ONLY
            } else {
                AnnotationFlags::PRINT
            });
        if let [(None, s)] = aps.as_slice() {
            a.appearance().normal().stream(*s);
        } else {
            let mut ap = a.appearance();
            let mut n = ap.normal().streams();
            for (state, s) in aps {
                if let Some(state) = state {
                    n.pair(Name(state.as_bytes()), *s);
                }
            }
        }
        if let Some((Some(on), _)) = aps.first() {
            let checked = op.field.checked;
            a.appearance_state(Name(if checked { on.as_bytes() } else { b"Off" }));
        }
    }
}

/// The side of the square a checkbox or radio mark of `size` is drawn in:
/// that of a widget of the intrinsic size (`size` + 2 pt), but no larger
/// than the `w`×`h` widget.
fn mark_side(w: f32, h: f32, size: f32) -> f32 {
    (size + crate::forms::PAD).min(w).min(h)
}

/// A check mark of `size`, centred in a `w`×`h` box (y-up), as a closed
/// polygon.
fn check(w: f32, h: f32, size: f32) -> Vec<PathSeg> {
    let s = mark_side(w, h, size);
    let (ox, oy) = ((w - s) / 2.0, (h - s) / 2.0);
    let p = |x: f32, y: f32| (ox + x * s, oy + y * s);
    let pts = [
        p(0.15, 0.52),
        p(0.27, 0.62),
        p(0.42, 0.42),
        p(0.75, 0.82),
        p(0.87, 0.72),
        p(0.42, 0.18),
    ];
    polygon(&pts).segs
}

/// A centred dot of `size` for a selected radio button.
fn dot(w: f32, h: f32, size: f32) -> Vec<PathSeg> {
    let r = mark_side(w, h, size) * 0.28;
    let k = KAPPA * r;
    let (cx, cy) = (w / 2.0, h / 2.0);
    vec![
        PathSeg::MoveTo(cx + r, cy),
        PathSeg::CubicTo(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
        PathSeg::CubicTo(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
        PathSeg::CubicTo(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
        PathSeg::CubicTo(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        PathSeg::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The PDF (uncompressed, as text) of a 200 × 200 pt page holding
    /// `children`.
    fn render(children: &str) -> String {
        let doc = format!(
            r#"{{"version":1,"pages":[{{"size":{{"width":200,"height":200}},"children":[{children}]}}]}}"#
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
        String::from_utf8_lossy(&pdf).into_owned()
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

    /// Width and height of what `segs` cover.
    fn extent(segs: &[PathSeg]) -> (f32, f32) {
        let pts: Vec<(f32, f32)> = segs
            .iter()
            .flat_map(|s| match *s {
                PathSeg::MoveTo(x, y) | PathSeg::LineTo(x, y) | PathSeg::CubicTo(.., x, y) => {
                    vec![(x, y)]
                }
                PathSeg::Close => vec![],
            })
            .collect();
        let span = |f: fn(&(f32, f32)) -> f32| {
            let v = pts.iter().map(f);
            v.clone().fold(f32::MIN, f32::max) - v.fold(f32::MAX, f32::min)
        };
        (span(|p| p.0), span(|p| p.1))
    }

    #[test]
    fn marks_take_their_size_from_the_field_not_the_box() {
        let same = |a: (f32, f32), b: (f32, f32)| {
            assert!(
                (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3,
                "{a:?} {b:?}"
            );
        };
        // A 10 pt mark draws as in its 12 pt intrinsic box, whatever box
        // it is centred in.
        let small = extent(&check(12.0, 12.0, 10.0));
        same(small, (0.72 * 12.0, 0.64 * 12.0));
        same(extent(&check(40.0, 40.0, 10.0)), small);
        same(
            extent(&dot(40.0, 40.0, 10.0)),
            extent(&dot(12.0, 12.0, 10.0)),
        );
        // A larger mark in a smaller box is shrunk to fit it.
        same(extent(&check(12.0, 12.0, 20.0)), small);
    }

    #[test]
    fn checkboxes_and_radio_buttons_keep_their_intrinsic_size() {
        let field = |kind: &str, extra: &str| {
            format!(
                r#"{{"type":"field",{extra}"field":{{"name":"{kind}","kind":"{kind}","value":"v","size":30}}}}"#
            )
        };
        let pdf = render(&field("radio", ""));
        assert_eq!(rects(&pdf), [[0.0, 168.0, 32.0, 200.0]]);
        // In a row, too, and centred where the row centres.
        let row = |align: &str| {
            render(&format!(
                r#"{{"type":"box","style":{{"flexDirection":"row","height":100{align}}},"children":[{}]}}"#,
                field("radio", "")
            ))
        };
        assert_eq!(rects(&row("")), [[0.0, 168.0, 32.0, 200.0]]);
        assert_eq!(
            rects(&row(r#","alignItems":"center""#)),
            [[0.0, 134.0, 32.0, 166.0]]
        );
        // Asked to stretch, it stretches.
        let pdf = render(&field("radio", r#""style":{"alignSelf":"stretch"},"#));
        assert_eq!(rects(&pdf), [[0.0, 168.0, 200.0, 200.0]]);
    }

    #[test]
    fn a_rotated_widget_turns_its_appearance() {
        let pdf = render(
            r#"{"type":"box","style":{"rotate":90,"width":40,"height":20},"children":[
                 {"type":"field","style":{"width":40,"height":20},"field":{"name":"c","kind":"checkbox","checked":true}}]}"#,
        );
        // Turned a quarter clockwise around (20, 10): 20 wide, 40 tall.
        assert_eq!(rects(&pdf), [[10.0, 170.0, 30.0, 210.0]]);
        assert_eq!(pdf.matches("/Matrix [0 -1 1 0 0 0]").count(), 2, "{pdf}");
        // Upright widgets have no matrix.
        let pdf = render(
            r#"{"type":"field","style":{"width":40,"height":20},"field":{"name":"c","kind":"checkbox"}}"#,
        );
        assert!(!pdf.contains("/Matrix"));
    }

    #[test]
    fn radio_values_that_cannot_name_a_state_use_positions() {
        let radio = |value: &str, checked: bool| {
            format!(
                r#"{{"type":"field","field":{{"name":"heating","kind":"radio","value":"{value}","checked":{checked}}}}}"#
            )
        };
        let pdf = render(&format!("{},{}", radio("Off", true), radio("On", false)));
        assert!(pdf.contains("/Opt [(Off) (On)]"), "{pdf}");
        assert!(pdf.contains("/V /0"), "{pdf}");
        assert!(pdf.contains("/AS /0"), "{pdf}");
        // One off state per widget, never the on state of a button too.
        assert_eq!(pdf.matches("/Off ").count(), 2, "{pdf}");
        assert!(!pdf.contains("/V /Off"));
        let long = "x".repeat(MAX_NAME + 1);
        let pdf = render(&format!("{},{}", radio("a", false), radio(&long, true)));
        assert!(pdf.contains("/V /1"), "{pdf}");
        assert!(!pdf.contains(&format!("/{long}")));
        // Values that are names stay the states.
        let pdf = render(&format!("{},{}", radio("gas", true), radio("oil", false)));
        assert!(pdf.contains("/V /gas") && pdf.contains("/oil") && !pdf.contains("/Opt"));
    }
}
