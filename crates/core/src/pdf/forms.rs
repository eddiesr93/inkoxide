//! `AcroForm` output: one field per name, one widget annotation
//! per painted field op, and an appearance stream for every widget state
//! (no `NeedAppearances`, so PDF/A-2b accepts the form).

use std::collections::BTreeMap;

use pdf_writer::types::{ActionType, AnnotationFlags, FieldFlags, FieldType};
use pdf_writer::writers::Field as FieldWriter;
use pdf_writer::{Content, Finish, Name, Pdf, Rect as PdfRect, Ref, Str};

use super::pdfa::limits::NAME as MAX_NAME;
use super::tag::{struct_parent, AnnotKind, AnnotTags};
use super::{
    font_name, round_pt, set_fill, show_text, EncodedPage, Out, Profile, RefAlloc, Writer,
};
use crate::fonts::{FontKey, FontSet};
use crate::forms::{format, FieldOp};
use crate::paint::{polygon, PathSeg, KAPPA};
use crate::schema::{FieldKind, FieldValue, BLACK};
use crate::InkoxideError;

/// What the catalog's `/AcroForm` needs.
pub(super) struct Form {
    pub fields: Vec<Ref>,
    pub fonts: Vec<FontKey>,
}

/// A non-terminal field: the group a dot in field names makes.
struct Group<'n> {
    r: Ref,
    /// Its own part of the name (`/T`).
    part: &'n str,
    parent: Option<Ref>,
    kids: Vec<Ref>,
}

/// The groups of a field tree, made as names reach them.
#[derive(Default)]
struct Groups<'n> {
    list: Vec<Group<'n>>,
    /// Index in `list` by full name (`a.b`).
    by_name: BTreeMap<&'n str, usize>,
}

impl<'n> Groups<'n> {
    /// The group that holds the field named `name` (`None`: a root
    /// field), and the field's own part of the name. Groups not made yet
    /// are made from the top down; new root groups join `roots`.
    fn parent_of(
        &mut self,
        alloc: &mut RefAlloc,
        roots: &mut Vec<Ref>,
        name: &'n str,
    ) -> (Option<usize>, &'n str) {
        let mut parent = None;
        let mut start = 0;
        for (i, _) in name.match_indices('.') {
            let full = &name[..i];
            let g = if let Some(&g) = self.by_name.get(full) {
                g
            } else {
                let g = self.list.len();
                let r = alloc.next();
                self.list.push(Group {
                    r,
                    part: &name[start..i],
                    parent: parent.map(|p: usize| self.list[p].r),
                    kids: Vec::new(),
                });
                match parent {
                    Some(p) => self.list[p].kids.push(r),
                    None => roots.push(r),
                }
                self.by_name.insert(full, g);
                g
            };
            parent = Some(g);
            start = i + 1;
        }
        (parent, &name[start..])
    }
}

/// Writes every field; appends widget refs to `annots` (per page).
/// `None` when the document has no fields.
pub(super) fn write_fields(
    w: &mut Writer,
    ctx: &Ctx<'_, '_, '_>,
    annots: &mut [Vec<Ref>],
    mut tags: Option<&mut AnnotTags>,
) -> Result<Option<Form>, InkoxideError> {
    let Writer { pdf, alloc, out } = w;
    let mut key = |r: Ref, page: usize, op: &FieldOp| {
        tags.as_deref_mut()
            .map(|t| t.add(r, page, op.owner, AnnotKind::Widget))
    };
    // Widgets by field name, in order of first appearance.
    let mut order: Vec<&str> = Vec::new();
    let mut widgets_of: BTreeMap<&str, Vec<(usize, &FieldOp)>> = BTreeMap::new();
    for (i, page) in ctx.pages.iter().enumerate() {
        for f in &page.fields {
            let name = f.field.name.as_str();
            let w = widgets_of.entry(name).or_default();
            if w.is_empty() {
                order.push(name);
            }
            w.push((i, f));
        }
    }
    if order.is_empty() {
        return Ok(None);
    }
    let mut form = Form {
        fields: Vec::new(),
        fonts: Vec::new(),
    };
    let mut tree = Groups::default();
    for full in order {
        let widgets = &widgets_of[full];
        let states = on_states(widgets, out.is_encrypted());
        let first = widgets[0].1;
        if let Some(k) = first.font {
            if ctx.font_refs.contains_key(&k) && !form.fonts.contains(&k) {
                form.fonts.push(k);
            }
        }
        // Only root fields and groups are listed in the form.
        let (group, name) = tree.parent_of(alloc, &mut form.fields, full);
        let parent = group.map(|g| tree.list[g].r);
        let field_ref = alloc.next();
        match group {
            Some(g) => tree.list[g].kids.push(field_ref),
            None => form.fields.push(field_ref),
        }
        if widgets.len() == 1 && first.field.kind != FieldKind::Radio {
            // Field and widget merged in one dictionary.
            let aps = ctx.appearances(pdf, alloc, out, first, &states.names[0])?;
            let mut f = pdf.form_field(field_ref);
            if let Some(p) = parent {
                f.parent(p);
            }
            field_entries(&mut f, out, name, widgets, &states, ctx.profile.scripts());
            let mut annot = f.into_annotation();
            ctx.widget(&mut annot, widgets[0], &aps, states.on[0]);
            struct_parent(&mut annot, key(field_ref, widgets[0].0, first));
            annot.finish();
            annots[widgets[0].0].push(field_ref);
            continue;
        }
        let mut kids = Vec::with_capacity(widgets.len());
        for (i, (&(page, op), on)) in widgets.iter().zip(&states.names).enumerate() {
            let aps = ctx.appearances(pdf, alloc, out, op, on)?;
            let kid = alloc.next();
            let mut f = pdf.form_field(kid);
            f.parent(field_ref);
            let mut annot = f.into_annotation();
            ctx.widget(&mut annot, (page, op), &aps, states.on[i]);
            struct_parent(&mut annot, key(kid, page, op));
            annot.finish();
            annots[page].push(kid);
            kids.push(kid);
        }
        let mut f = pdf.form_field(field_ref);
        if let Some(p) = parent {
            f.parent(p);
        }
        field_entries(&mut f, out, name, widgets, &states, ctx.profile.scripts());
        f.children(kids);
        f.finish();
    }
    for g in &tree.list {
        let part = out.text(g.part);
        let mut f = pdf.form_field(g.r);
        f.pair(Name(b"T"), Str(&part));
        if let Some(p) = g.parent {
            f.parent(p);
        }
        f.children(g.kids.iter().copied());
    }
    Ok(Some(form))
}

/// The on-state names of a field's widgets, in order, and which are on.
struct OnStates {
    names: Vec<String>,
    /// The names are positions, and `/Opt` holds the widgets' values.
    positions: bool,
    /// A radio group's value (`/V`): the on state of its first checked
    /// button, `None` when none is.
    value: Option<String>,
    /// Whether each widget is on; for radio buttons, those whose on
    /// state is the group's value.
    on: Vec<bool>,
}

/// A checkbox is on in state `Yes`, a radio button in the state named
/// after its value. A value that cannot name a state (`Off` is the off
/// state, and a name has at most [`MAX_NAME`] bytes) makes every button of
/// its group use a position instead, with the values in `/Opt`. So does
/// encryption (`hidden`): names are not encrypted, the strings of `/Opt`
/// are. The position is that of the first button with the same value, so
/// that buttons of one value share a state, as they do by name: a button
/// repeated on several pages turns on and off as one.
fn on_states(widgets: &[(usize, &FieldOp)], hidden: bool) -> OnStates {
    fn value(op: &FieldOp) -> &str {
        op.field.value_str().unwrap_or_default()
    }
    if widgets[0].1.field.kind != FieldKind::Radio {
        return OnStates {
            names: vec!["Yes".to_string(); widgets.len()],
            positions: false,
            value: None,
            on: widgets.iter().map(|(_, w)| w.field.checked).collect(),
        };
    }
    let positions = hidden
        || widgets.iter().any(|(_, w)| {
            let v = value(w);
            v == "Off" || v.len() > MAX_NAME
        });
    let names: Vec<String> = if positions {
        widgets
            .iter()
            .map(|(_, w)| {
                let first = widgets.iter().position(|(_, o)| value(o) == value(w));
                first.unwrap_or_default().to_string()
            })
            .collect()
    } else {
        widgets.iter().map(|(_, w)| value(w).to_string()).collect()
    };
    let chosen = widgets
        .iter()
        .zip(&names)
        .find(|((_, w), _)| w.field.checked)
        .map(|(_, s)| s.clone());
    OnStates {
        on: names.iter().map(|n| chosen.as_ref() == Some(n)).collect(),
        names,
        positions,
        value: chosen,
    }
}

/// The field-level entries: type, name, flags, value, default appearance,
/// and with `scripts` the actions of a format. Strings go through `out`.
fn field_entries(
    f: &mut FieldWriter<'_>,
    out: &mut Out,
    name: &str,
    widgets: &[(usize, &FieldOp)],
    states: &OnStates,
    scripts: bool,
) {
    let op = widgets[0].1;
    let spec = &op.field;
    f.pair(Name(b"T"), Str(&out.text(name)));
    // What screen readers and tooltips say for the field.
    if let Some(label) = &spec.label {
        f.pair(Name(b"TU"), Str(&out.text(label)));
    }
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
            if let Some(v) = spec.value_str() {
                if !spec.password {
                    let stored = spec
                        .format
                        .as_deref()
                        .and_then(|fmt| format::stored(fmt, v));
                    f.pair(Name(b"V"), Str(&out.text(stored.as_deref().unwrap_or(v))));
                }
            }
            if let (Some(fmt), true) = (spec.format.as_deref(), scripts) {
                // The keystroke and format actions viewers run.
                let (keystroke, format) = format::scripts(fmt);
                let (keystroke, format) = (out.text(&keystroke), out.text(&format));
                let mut aa = f.additional_actions();
                aa.form_calculate_partial()
                    .action_type(ActionType::JavaScript)
                    .pair(Name(b"JS"), Str(&keystroke));
                aa.form_format()
                    .action_type(ActionType::JavaScript)
                    .pair(Name(b"JS"), Str(&format));
            }
        }
        FieldKind::Select => {
            f.field_type(FieldType::Choice);
            flags |= FieldFlags::COMBO;
            options(f, out, &spec.options);
            match spec.value_str() {
                Some(v) => f.pair(Name(b"V"), Str(&out.text(v))),
                None => f.pair(Name(b"V"), pdf_writer::Null),
            };
        }
        FieldKind::List => {
            f.field_type(FieldType::Choice);
            if spec.multi_select {
                flags |= FieldFlags::MULTI_SELECT;
            }
            options(f, out, &spec.options);
            match &spec.value {
                Some(FieldValue::Many(v)) => {
                    let values: Vec<Vec<u8>> = v.iter().map(|v| out.text(v)).collect();
                    f.insert(Name(b"V"))
                        .array()
                        .items(values.iter().map(|v| Str(v)));
                }
                Some(FieldValue::One(v)) => {
                    f.pair(Name(b"V"), Str(&out.text(v)));
                }
                None => {}
            }
            if !op.selected.is_empty() {
                f.choice_indices(op.selected.iter().map(|&i| index(i)));
            }
            if op.top_index > 0 {
                f.choice_top_index(index(op.top_index));
            }
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
            let on = states.value.as_deref().unwrap_or("Off");
            f.radio_value(Name(on.as_bytes()));
            if states.positions {
                let values: Vec<&str> = widgets
                    .iter()
                    .map(|(_, w)| w.field.value_str().unwrap_or(""))
                    .collect();
                options(f, out, &values);
            }
        }
    }
    if !flags.is_empty() {
        f.field_flags(flags);
    }
    if spec.is_text_like() {
        f.pair(
            Name(b"DA"),
            Str(&out.bytes(default_appearance(op).as_bytes())),
        );
    }
}

/// `/Opt`: the text of each option, through `out`.
fn options(f: &mut FieldWriter<'_>, out: &mut Out, options: &[impl AsRef<str>]) {
    let options: Vec<Vec<u8>> = options.iter().map(|o| out.text(o.as_ref())).collect();
    f.insert(Name(b"Opt"))
        .array()
        .items(options.iter().map(|o| Str(o)));
}

/// An index into `/Opt` (at most 1000 options).
fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(i32::MAX)
}

/// `/DA`: font, size and colour viewers use for edited text.
fn default_appearance(op: &FieldOp) -> String {
    let c = op.field.color.unwrap_or(BLACK);
    let font = op.font.map_or_else(|| "F0".to_string(), font_name);
    format!(
        "/{font} {} Tf {} {} {} rg",
        round_pt(op.field.size),
        round_pt(c[0]),
        round_pt(c[1]),
        round_pt(c[2])
    )
}

/// What the fields' widgets and appearance streams refer to.
pub(super) struct Ctx<'r, 'f, 'a> {
    pub pages: &'r [EncodedPage],
    pub page_refs: &'r [Ref],
    pub font_refs: &'r BTreeMap<FontKey, Ref>,
    pub fonts: &'f FontSet<'a>,
    /// Under PDF/A and PDF/UA, `.notdef` glyphs are left out of appearance
    /// streams, as page content does; under PDF/A fields carry no scripts.
    pub profile: Profile,
}

/// Appearance streams of one widget: `(state, stream)`; the state is
/// `None` for text, select and list widgets (a single appearance).
type Appearances = Vec<(Option<String>, Ref)>;

impl Ctx<'_, '_, '_> {
    /// The appearance streams of widget `op`, whose on state is `on`
    /// (checkboxes and radio buttons).
    fn appearances(
        &self,
        pdf: &mut Pdf,
        alloc: &mut RefAlloc,
        out: &mut Out,
        op: &FieldOp,
        on: &str,
    ) -> Result<Appearances, InkoxideError> {
        let (w, h) = (op.rect.w, op.rect.h);
        // A transformed widget: its appearance turns and scales with it
        // (widget space is y-up, layout space y-down), and the viewer fits
        // the result to the widget's `/Rect`, which bounds it.
        let matrix = op
            .ctm
            .map(|[a, b, c, d, ..]| [a, -b, -c, d, 0.0, 0.0].map(round_pt))
            .filter(|m| *m != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let mut aps = Vec::new();
        let mut form = |pdf: &mut Pdf, content: Vec<u8>, font: Option<FontKey>| -> Ref {
            let r = alloc.next();
            out.form_xobject(pdf, r, &content, |x| {
                x.bbox(PdfRect::new(0.0, 0.0, round_pt(w), round_pt(h)));
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
            });
            r
        };
        match op.field.kind {
            FieldKind::Text | FieldKind::Select | FieldKind::List => {
                let mut c = Content::new();
                c.begin_marked_content(Name(b"Tx"));
                c.save_state();
                c.rect(
                    1.0,
                    1.0,
                    round_pt((w - 2.0).max(0.0)),
                    round_pt((h - 2.0).max(0.0)),
                );
                c.clip_nonzero();
                c.end_path();
                // Widget space is y-up; the text ops are top-left based.
                c.transform([1.0, 0.0, 0.0, -1.0, 0.0, round_pt(h)]);
                if !op.highlights.is_empty() {
                    set_fill(&mut c, &crate::forms::HIGHLIGHT);
                    for r in &op.highlights {
                        c.rect(round_pt(r.x), round_pt(r.y), round_pt(r.w), round_pt(r.h));
                    }
                    c.fill_nonzero();
                }
                for t in &op.text {
                    show_text(&mut c, t, self.fonts, self.profile.skips_notdef(), false);
                }
                c.restore_state();
                c.end_marked_content();
                let content = super::content_bytes(c, self.profile)?;
                aps.push((None, form(pdf, content, op.font)));
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
                let content = super::content_bytes(c, self.profile)?;
                aps.push((Some(on.to_string()), form(pdf, content, None)));
                aps.push((Some("Off".into()), form(pdf, Vec::new(), None)));
            }
        }
        Ok(aps)
    }

    /// Writes widget `op`'s entries; `checked`: a checkbox or radio
    /// button is on.
    fn widget(
        &self,
        a: &mut pdf_writer::writers::Annotation<'_>,
        (page, op): (usize, &FieldOp),
        aps: &Appearances,
        checked: bool,
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
        render_with(children, "", &crate::FontStore::new(), false)
    }

    /// [`render`], with `Inter-Regular` declared.
    fn render_inter(children: &str) -> String {
        render_with(
            children,
            r#""fonts":[{"id":"Inter-Regular","family":"Inter"}],"#,
            &crate::test_fixtures::inter(),
            false,
        )
    }

    fn render_with(children: &str, fonts: &str, store: &crate::FontStore, pdf_a: bool) -> String {
        let doc = format!(
            r#"{{"version":1,{fonts}"pages":[{{"size":{{"width":200,"height":200}},"children":[{children}]}}]}}"#
        );
        let options = crate::RenderOptions::default()
            .with_compress(false)
            .with_pdf_a(pdf_a);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &options,
            store,
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

    /// The text of each object of `pdf`, by number.
    fn objects(pdf: &str) -> BTreeMap<i32, &str> {
        pdf.split("endobj")
            .filter_map(|o| {
                let (head, body) = o.split_once(" 0 obj")?;
                let n = head.rsplit(char::is_whitespace).next()?.parse().ok()?;
                Some((n, body))
            })
            .collect()
    }

    /// The number of the object whose partial name is `t`.
    fn named(objects: &BTreeMap<i32, &str>, t: &str) -> i32 {
        let key = format!("/T ({t})");
        let found: Vec<i32> = objects
            .iter()
            .filter(|(_, o)| o.contains(&key))
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(found.len(), 1, "{t}: {found:?}");
        found[0]
    }

    #[test]
    fn dotted_names_make_a_field_tree() {
        let field = |name: &str, kind: &str| {
            format!(r#"{{"type":"field","field":{{"name":"{name}","kind":"{kind}"}}}}"#)
        };
        let radio = |value: &str| {
            format!(
                r#"{{"type":"field","field":{{"name":"a.d","kind":"radio","value":"{value}"}}}}"#
            )
        };
        let pdf = render(
            &[
                field("a.b.c", "checkbox"),
                field("e", "checkbox"),
                radio("x"),
                radio("y"),
            ]
            .join(","),
        );
        let o = objects(&pdf);
        let [a, b, c, d, e] = ["a", "b", "c", "d", "e"].map(|t| named(&o, t));
        // Only the root field and group are in the form, in order.
        assert!(pdf.contains(&format!("/Fields [{a} 0 R {e} 0 R]")), "{pdf}");
        let parent = |n: i32| {
            o[&n]
                .split("/Parent ")
                .nth(1)
                .map(|p| p.split(' ').next().unwrap().parse::<i32>().unwrap())
        };
        assert_eq!(
            [a, b, c, d, e].map(parent),
            [None, Some(a), Some(b), Some(a), None]
        );
        assert!(
            o[&a].contains(&format!("/Kids [{b} 0 R {d} 0 R]")),
            "{}",
            o[&a]
        );
        assert!(o[&b].contains(&format!("/Kids [{c} 0 R]")), "{}", o[&b]);
        // A group is no field of its own.
        assert!(!o[&a].contains("/FT") && !o[&b].contains("/Subtype"));
        // The radio group holds its two buttons.
        assert_eq!(o[&d].matches(" 0 R").count(), 3, "{}", o[&d]);
        assert!(!pdf.contains("(a.b.c)") && !pdf.contains("(a.d)"));
    }

    #[test]
    fn list_boxes_are_choice_fields_without_the_combo_flag() {
        let list = |extra: &str| {
            render_inter(&format!(
                r#"{{"type":"field","style":{{"height":20}},"field":{{"name":"l","kind":"list","font":"Inter-Regular","options":["a","b","c","d"]{extra}}}}}"#
            ))
        };
        let pdf = list(r#","multiSelect":true,"value":["d","b"]"#);
        assert!(pdf.contains("/FT /Ch"), "{pdf}");
        assert!(pdf.contains("/Opt [(a) (b) (c) (d)]"), "{pdf}");
        assert!(pdf.contains("/V [(d) (b)]"), "{pdf}");
        assert!(pdf.contains("/I [1 3]"), "{pdf}");
        // Multiple selection is bit 22; the top index makes `b` visible.
        assert!(pdf.contains(&format!("/Ff {}", 1 << 21)), "{pdf}");
        assert!(pdf.contains("/TI 1"), "{pdf}");
        let pdf = list(r#","value":"a""#);
        assert!(pdf.contains("/V (a)") && pdf.contains("/I [0]"), "{pdf}");
        // (The font program is binary: look for whole entries.)
        assert!(!pdf.contains("/Ff ") && !pdf.contains("/TI "), "{pdf}");
        let pdf = list("");
        assert!(!pdf.contains("/V (") && !pdf.contains("/I ["), "{pdf}");
        // The highlight is drawn behind the selected row.
        let pdf = list(r#","value":"a""#);
        assert!(pdf.contains("0.6 0.75 0.86 rg"), "{pdf}");
    }

    #[test]
    fn formats_add_their_scripts_except_in_pdf_a() {
        let field = r#"{"type":"field","style":{"height":20},"field":{"name":"d","kind":"text","font":"Inter-Regular","value":"2026-10-14","format":{"type":"date","pattern":"dd mmm yyyy"}}}"#;
        let pdf = render_inter(field);
        assert!(pdf.contains("/V (14 Oct 2026)"), "{pdf}");
        assert!(pdf.contains("/AA <<"), "{pdf}");
        assert!(
            pdf.contains("/K <<") && pdf.contains("/S /JavaScript"),
            "{pdf}"
        );
        assert!(
            pdf.contains(r#"/JS (AFDate_KeystrokeEx("dd mmm yyyy");)"#),
            "{pdf}"
        );
        assert!(
            pdf.contains(r#"/JS (AFDate_FormatEx("dd mmm yyyy");)"#),
            "{pdf}"
        );
        // PDF/A forbids scripts: the formatted value stays.
        let pdf = render_with(
            field,
            r#""fonts":[{"id":"Inter-Regular","family":"Inter"}],"#,
            &crate::test_fixtures::inter(),
            true,
        );
        assert!(pdf.contains("/V (14 Oct 2026)"), "{pdf}");
        assert!(
            !pdf.contains("/AA <<") && !pdf.contains("/S /JavaScript"),
            "{pdf}"
        );
        // Nor at PDF/A-1b.
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],"pages":[{{"size":{{"width":200,"height":200}},"children":[{field}]}}]}}"#
        );
        let options = crate::RenderOptions::default()
            .with_compress(false)
            .with_pdf_a_level(crate::PdfALevel::A1b);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &options,
            &crate::test_fixtures::inter(),
            &crate::ImageStore::new(),
        )
        .unwrap();
        let pdf = String::from_utf8_lossy(&pdf);
        assert!(pdf.contains("/V (14 Oct 2026)"), "{pdf}");
        assert!(
            !pdf.contains("/AA <<") && !pdf.contains("/S /JavaScript"),
            "{pdf}"
        );
        // Numbers keep their value; the viewer formats it.
        let pdf = render_inter(
            r#"{"type":"field","field":{"name":"n","kind":"text","font":"Inter-Regular","value":"-1234.5","format":{"type":"number","currency":"$"}}}"#,
        );
        assert!(pdf.contains("/V (-1234.5)"), "{pdf}");
        assert!(
            pdf.contains(r#"AFNumber_Format(2, 0, 0, 0, "$", true);"#),
            "{pdf}"
        );
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

    #[test]
    fn radio_buttons_of_one_value_share_a_state() {
        let radio = |value: &str, checked: bool| {
            format!(
                r#"{{"type":"field","field":{{"name":"heating","kind":"radio","value":"{value}","checked":{checked}}}}}"#
            )
        };
        // By position: the first button of each value names the state.
        let pdf = render(&[radio("Off", false), radio("gas", true), radio("gas", false)].join(","));
        assert!(pdf.contains("/Opt [(Off) (gas) (gas)]"), "{pdf}");
        assert!(pdf.contains("/V /1"), "{pdf}");
        assert_eq!(pdf.matches("/AS /1").count(), 2, "{pdf}");
        assert!(!pdf.contains("/2 "), "{pdf}");
        // By name, every button of the chosen value is on.
        let pdf = render(&[radio("gas", false), radio("gas", true)].join(","));
        assert_eq!(pdf.matches("/AS /gas").count(), 2, "{pdf}");
    }
}
