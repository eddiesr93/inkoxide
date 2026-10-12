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
use crate::paint::PathSeg;
use crate::schema::{FieldKind, BLACK};

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
            let aps = ctx.appearances(pdf, alloc, first);
            let mut f = pdf.form_field(field_ref);
            field_entries(&mut f, name, widgets);
            let mut annot = f.into_annotation();
            ctx.widget(&mut annot, widgets[0], &aps);
            annot.finish();
            annots[widgets[0].0].push(field_ref);
            continue;
        }
        let mut kids = Vec::with_capacity(widgets.len());
        for &(page, op) in widgets {
            let aps = ctx.appearances(pdf, alloc, op);
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
        field_entries(&mut f, name, widgets);
        f.children(kids);
        f.finish();
    }
    Some(form)
}

/// The field-level entries: type, name, flags, value, default appearance.
fn field_entries(f: &mut FieldWriter<'_>, name: &str, widgets: &[(usize, &FieldOp)]) {
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
                .find(|(_, w)| w.field.checked)
                .and_then(|(_, w)| w.field.value.as_deref());
            f.radio_value(Name(on.unwrap_or("Off").as_bytes()));
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
    pub page_refs: &'r [(Ref, Ref)],
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
    fn appearances(&self, pdf: &mut Pdf, alloc: &mut RefAlloc, op: &FieldOp) -> Appearances {
        let (w, h) = (op.rect.w, op.rect.h);
        let mut out = Vec::new();
        let mut form = |pdf: &mut Pdf, content: Vec<u8>, font: Option<FontKey>| -> Ref {
            let r = alloc.next();
            let mut x = pdf.form_xobject(r, &content);
            x.bbox(PdfRect::new(0.0, 0.0, q(w), q(h)));
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
                let on_name = match op.field.kind {
                    FieldKind::Radio => op.field.value.clone().unwrap_or_default(),
                    _ => "Yes".to_string(),
                };
                let mut c = Content::new();
                set_fill(&mut c, &op.field.color.unwrap_or(BLACK));
                let segs = if op.field.kind == FieldKind::Radio {
                    dot(w, h)
                } else {
                    check(w, h)
                };
                super::path(&mut c, &segs);
                c.fill_nonzero();
                out.push((Some(on_name), form(pdf, c.finish().into_vec(), None)));
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
        let r = op.rect;
        let page_h = self.pages[page].height;
        a.rect(PdfRect::new(
            q(r.x),
            q(page_h - r.y - r.h),
            q(r.x + r.w),
            q(page_h - r.y),
        ))
        .page(self.page_refs[page].0)
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

/// A check mark filling a `w`×`h` box (y-up), as a closed polygon.
fn check(w: f32, h: f32) -> Vec<PathSeg> {
    let s = w.min(h);
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
    let mut segs = vec![PathSeg::MoveTo(pts[0].0, pts[0].1)];
    segs.extend(pts[1..].iter().map(|&(x, y)| PathSeg::LineTo(x, y)));
    segs.push(PathSeg::Close);
    segs
}

/// A centred dot for a selected radio button.
fn dot(w: f32, h: f32) -> Vec<PathSeg> {
    let r = w.min(h) * 0.28;
    let k = 0.552_284_8 * r;
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
