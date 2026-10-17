//! Interactive form fields (`AcroForm`): intrinsic size and the
//! widget ops painting hands to the PDF writer.

pub(crate) mod format;

use crate::fonts::{FontKey, FontSet, LoadedFont};
use crate::geom::Rect;
use crate::layout::{AvailableSpace, Geometry, Size};
use crate::paint::{ContentCtx, Op, TextGlyph, TextOp};
use crate::schema::{Color, Field, FieldKind, BLACK};

/// Inner padding between the widget edge and its text, points.
pub(crate) const PAD: f32 = 2.0;
/// Default width of text, select and list fields without one, points.
const DEFAULT_WIDTH: f32 = 150.0;
/// Visible lines of a multiline field without a height.
const MULTILINE_ROWS: f32 = 3.0;
/// Most options a list box without a height shows.
const LIST_ROWS: usize = 4;
/// The colour of a negative number in a red style.
const RED: Color = [1.0, 0.0, 0.0, 1.0];
/// Background of the selected options of a list box.
pub(crate) const HIGHLIGHT: Color = [0.6, 0.75, 0.86, 1.0];
/// Line height of a field's text, as a multiple of its font size.
const LINE_HEIGHT: f32 = 1.2;

/// Characters whose glyphs are embedded for every font a text, select or
/// list field uses, so a viewer can show edits made in the common Latin range
/// (including S and T with comma below, U+0218–U+021B) with the subset
/// font.
const EDIT_RANGES: &[(char, char)] = &[
    (' ', '~'),
    ('\u{A0}', '\u{17F}'),
    ('\u{218}', '\u{21B}'),
    ('\u{2013}', '\u{2014}'),
    ('\u{2018}', '\u{201E}'),
    ('\u{2022}', '\u{2022}'),
    ('\u{2026}', '\u{2026}'),
    ('\u{20AC}', '\u{20AC}'),
];

/// A widget to write as an annotation, with its appearance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldOp {
    /// Widget area in page coordinates (top-left origin).
    pub rect: Rect,
    pub field: Field,
    pub font: Option<FontKey>,
    /// The initial value as glyph runs, in widget coordinates (top-left
    /// origin).
    pub text: Vec<TextOp>,
    /// Whether its font embeds the glyphs for editing (see
    /// [`edit_glyphs`]): the document's writer adds them once per font.
    pub editable: bool,
    /// The glyphs of a select's or a list's options.
    pub option_glyphs: Vec<u16>,
    /// Each glyph of `text` and `option_glyphs` with its character, for its
    /// font's `ToUnicode` map: they are looked up by character, not shaped.
    pub glyph_chars: Vec<(u16, char)>,
    /// A list box: the rows of its selected options in view, in widget
    /// coordinates (top-left origin), filled behind the text.
    pub highlights: Vec<Rect>,
    /// A list box: the indices of its selected options (`/I`).
    pub selected: Vec<usize>,
    /// A list box: the index of the first option in view (`/TI`).
    pub top_index: usize,
    /// The transform in effect where the widget is painted (`None`:
    /// identity); the PDF writer sets it.
    pub ctm: Option<[f32; 6]>,
    /// In tagged output, the structure element of its widget; the PDF
    /// writer sets it (`None`: in decoration, an element of its own).
    pub owner: Option<crate::tag::StructId>,
}

/// Content size of a field without explicit dimensions.
pub(crate) fn measure(f: &Field, available: Size<AvailableSpace>) -> Size<f32> {
    let line = f.size * LINE_HEIGHT;
    let width = match available.width {
        AvailableSpace::Definite(w) => w.min(DEFAULT_WIDTH),
        _ => DEFAULT_WIDTH,
    };
    match f.kind {
        FieldKind::List => Size {
            width,
            height: line * f.options.len().clamp(1, LIST_ROWS) as f32 + 2.0 * PAD,
        },
        FieldKind::Text | FieldKind::Select => Size {
            width,
            height: if f.multiline {
                line * MULTILINE_ROWS + 2.0 * PAD
            } else {
                line + 2.0 * PAD
            },
        },
        FieldKind::Checkbox | FieldKind::Radio => Size {
            width: f.size + PAD,
            height: f.size + PAD,
        },
    }
}

/// Emits the widget op of a field on the content box of `geo`.
pub(crate) fn paint(
    f: &Field,
    geo: &Geometry,
    ctx: &ContentCtx<'_>,
    fonts: &FontSet<'_>,
    ops: &mut Vec<Op>,
) {
    let Rect {
        x: cx,
        y: cy,
        w: cw,
        h: ch,
    } = geo.content_box();
    let y = cy + ctx.shift;
    // Taller than a page, a field is sliced across pages; one widget, on
    // the page it starts on, is the field.
    if ctx.window.is_some_and(|w| !w.starts(y)) {
        return;
    }
    let rect = Rect::new(cx + ctx.dx, y + ctx.dy, cw.max(0.0), ch.max(0.0));
    let font = f.font.as_deref().and_then(|id| fonts.key(id));
    let mut text = Vec::new();
    let mut option_glyphs = Vec::new();
    let mut glyph_chars = Vec::new();
    let editable = f.is_text_like() && !f.read_only;
    let line_h = f.size * LINE_HEIGHT;
    let (selected, top_index, highlights) = if f.kind == FieldKind::List {
        let selected = selected(f);
        let (top_index, highlights) = list_rows(f.options.len(), &selected, ch, line_h, cw);
        (selected, top_index, highlights)
    } else {
        (Vec::new(), 0, Vec::new())
    };
    if let Some(key) = font {
        let lf = fonts.get(key);
        let m = lf.metrics();
        let scale = lf.scale(f.size);
        let (asc, desc) = (m.ascender * scale, -m.descender * scale);
        // A formatted value shows formatted, in red for a negative number
        // in a red style.
        let formatted = f
            .format
            .as_deref()
            .zip(f.value_str())
            .and_then(|(fmt, v)| format::display(fmt, v));
        let color = match &formatted {
            Some(s) if s.red => RED,
            _ => f.color.unwrap_or(BLACK),
        };
        let shown = match (f.kind, f.password, &formatted) {
            (FieldKind::Text, false, Some(s)) => s.text.as_str(),
            (FieldKind::Text | FieldKind::Select, false, _) => f.value_str().unwrap_or(""),
            _ => "",
        };
        let rows = f.multiline || f.kind == FieldKind::List;
        let lines: Vec<&str> = if f.kind == FieldKind::List {
            f.options
                .iter()
                .skip(top_index)
                .take(in_view(ch, line_h))
                .map(|o| o.lines().next().unwrap_or(""))
                .collect()
        } else if f.multiline {
            shown.split('\n').collect()
        } else {
            vec![shown.lines().next().unwrap_or("")]
        };
        for (i, line) in lines.iter().enumerate() {
            let baseline = if rows {
                PAD + (line_h - asc - desc) / 2.0 + asc + i as f32 * line_h
            } else {
                (ch - asc - desc) / 2.0 + asc
            };
            let mut pen = PAD;
            let mut glyphs = Vec::new();
            for c in line.chars() {
                let Some(id) = lf.glyph(c) else { continue };
                glyph_chars.push((id, c));
                glyphs.push(TextGlyph {
                    id,
                    x: pen,
                    y_offset: 0.0,
                    continues: false,
                });
                pen += lf.advance(id) * scale;
            }
            if !glyphs.is_empty() {
                text.push(TextOp {
                    font: key,
                    size: f.size,
                    color,
                    x: 0.0,
                    y: baseline,
                    glyphs,
                });
            }
        }
        if editable {
            // A choice shows any option, whatever its script.
            for o in &f.options {
                for c in o.chars() {
                    let Some(id) = lf.glyph(c) else { continue };
                    option_glyphs.push(id);
                    glyph_chars.push((id, c));
                }
            }
        }
    }
    ops.push(Op::Field(Box::new(FieldOp {
        rect,
        field: f.clone(),
        font,
        text,
        editable: editable && font.is_some(),
        option_glyphs,
        glyph_chars,
        highlights,
        selected,
        top_index,
        ctm: None,
        owner: None,
    })));
}

/// How many rows of a list box `h` high are in view, at least in part
/// (the appearance clips the last one).
fn in_view(h: f32, line_h: f32) -> usize {
    ((h - 2.0 * PAD) / line_h).ceil().max(1.0) as usize
}

/// The indices of the selected options of a select or list field, in
/// order (an option listed twice is selected twice).
fn selected(f: &Field) -> Vec<usize> {
    let values: std::collections::HashSet<&str> = f.values().iter().map(String::as_str).collect();
    if values.is_empty() {
        return Vec::new();
    }
    f.options
        .iter()
        .enumerate()
        .filter(|(_, o)| values.contains(o.as_str()))
        .map(|(i, _)| i)
        .collect()
}

/// The index of the first option a list box of `options` options and
/// `h` high shows, chosen so its first `selected` option is in view, and
/// the rows of the selected options in view (`w` wide).
fn list_rows(
    options: usize,
    selected: &[usize],
    h: f32,
    line_h: f32,
    w: f32,
) -> (usize, Vec<Rect>) {
    // Rows wholly in view.
    let fit = (((h - 2.0 * PAD) / line_h).floor().max(1.0) as usize).min(options);
    let top = match selected.first() {
        Some(&first) if first >= fit => first.min(options - fit),
        _ => 0,
    };
    let in_view = in_view(h, line_h);
    let rows = selected
        .iter()
        .filter(|&&i| i >= top && i - top < in_view)
        .map(|&i| {
            Rect::new(
                1.0,
                PAD + (i - top) as f32 * line_h,
                (w - 2.0).max(0.0),
                line_h,
            )
        })
        .collect();
    (top, rows)
}

/// The glyphs of `font` for the characters of `EDIT_RANGES`, each with
/// its character, embedded for the editable text, select and list fields
/// set in it.
pub(crate) fn edit_glyphs<'f>(font: &'f LoadedFont<'_>) -> impl Iterator<Item = (u16, char)> + 'f {
    EDIT_RANGES
        .iter()
        .flat_map(|&(a, b)| a..=b)
        .filter_map(|c| Some((font.glyph(c)?, c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The widget op of `field` (a field object) painted in a 100 × 20
    /// box, and Inter's glyphs for `chars`.
    fn painted(field: &str, chars: &str) -> (FieldOp, Vec<u16>) {
        let json = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
               "pages":[{{"size":{{"width":200,"height":200}},"children":[{{"type":"field","field":{field}}}]}}]}}"#
        );
        let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
        let store = crate::test_fixtures::inter();
        let fonts = FontSet::load(&doc, &store).unwrap();
        let crate::schema::NodeKind::Field(f) = &doc.pages[0].children[0].kind else {
            panic!("not a field");
        };
        let geo = Geometry {
            w: 100.0,
            h: 20.0,
            ..Default::default()
        };
        let ctx = ContentCtx {
            dx: 0.0,
            dy: 0.0,
            shift: 0.0,
            line_shifts: &[],
            window: None,
            opacity: 1.0,
            line_boxes: None,
            ctm: None,
            tag: None,
        };
        let mut ops = Vec::new();
        paint(f, &geo, &ctx, &fonts, &mut ops);
        let Some(Op::Field(op)) = ops.pop() else {
            panic!("no field op");
        };
        let glyphs = chars.chars().map(|c| fonts.get(0).glyph(c).unwrap());
        let mut op = *op;
        // What the writer embeds for it.
        if op.editable {
            op.option_glyphs
                .extend(edit_glyphs(fonts.get(0)).map(|(g, _)| g));
        }
        (op, glyphs.collect())
    }

    fn field(json: &str) -> Field {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn list_boxes_show_their_first_selected_option() {
        let list = |value: &str| {
            field(&format!(
                r#"{{"name":"l","kind":"list","multiSelect":true,"options":["a","b","c","d","e"],"value":{value}}}"#
            ))
        };
        let list_rows = |f: &Field, h: f32, line_h: f32, w: f32| {
            list_rows(f.options.len(), &selected(f), h, line_h, w)
        };
        // Four rows of 12 pt fit in 52 pt: nothing to scroll for `b`.
        let (top, rows) = list_rows(&list(r#""b""#), 52.0, 12.0, 100.0);
        assert_eq!(top, 0);
        assert_eq!(rows, [Rect::new(1.0, 14.0, 98.0, 12.0)]);
        // `e` is past them: the list starts as low as it can.
        let (top, rows) = list_rows(&list(r#"["e"]"#), 52.0, 12.0, 100.0);
        assert_eq!(top, 1);
        assert_eq!(rows, [Rect::new(1.0, 38.0, 98.0, 12.0)]);
        // The first selected option, in the order of the options, decides;
        // `e` is then out of view.
        let (top, rows) = list_rows(&list(r#"["e","b"]"#), 52.0, 12.0, 100.0);
        assert_eq!((top, rows.len()), (0, 1));
        // One row in view: the first selected one, and the partly visible
        // one after it is not highlighted.
        let (top, rows) = list_rows(&list(r#"["c","d"]"#), 20.0, 12.0, 100.0);
        assert_eq!((top, rows.len()), (2, 2));
        let (top, rows) = list_rows(&list("[]"), 20.0, 12.0, 100.0);
        assert_eq!((top, rows.len()), (0, 0));
        // Without a height, at most four rows.
        let size = |options: usize| {
            let o = vec!["o"; options];
            let f = field(&format!(
                r#"{{"name":"l","kind":"list","options":{}}}"#,
                serde_json::to_string(&o).unwrap()
            ));
            measure(
                &f,
                Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            )
            .height
        };
        assert_eq!([size(1), size(4), size(9)], [16.0, 52.0, 52.0]);
    }

    #[test]
    fn formatted_values_show_formatted() {
        let text = |op: &FieldOp| op.text.iter().map(|t| t.glyphs.len()).sum::<usize>();
        let (op, _) = painted(
            r#"{"name":"n","kind":"text","font":"Inter-Regular","value":"-1234.5","format":{"type":"number","negative":"parensRed"}}"#,
            "",
        );
        // `(1,234.50)`, in red.
        assert_eq!(text(&op), 10);
        assert_eq!(op.text[0].color, RED);
        let (op, _) = painted(
            r#"{"name":"n","kind":"text","font":"Inter-Regular","value":"1234.5","format":{"type":"number","negative":"parensRed"}}"#,
            "",
        );
        assert_eq!(text(&op), 8);
        assert_eq!(op.text[0].color, BLACK);
    }

    #[test]
    fn list_boxes_paint_the_options_in_view() {
        let (op, _) = painted(
            r#"{"name":"l","kind":"list","font":"Inter-Regular","options":["a","b","c","d"],"value":"c"}"#,
            "",
        );
        // A 20 pt box shows the row of `c` and part of the one after it.
        assert_eq!(op.top_index, 2);
        assert_eq!(op.text.len(), 2);
        assert_eq!(op.highlights.len(), 1);
        assert!(op.editable && !op.option_glyphs.is_empty());
    }

    #[test]
    fn select_fields_embed_the_latin_range_and_every_option() {
        let (select, wanted) = painted(
            r#"{"name":"plan","kind":"select","font":"Inter-Regular","options":["Starter","Quokka","Ωmega"],"value":"Starter"}"#,
            "Qké€Ω",
        );
        for g in wanted {
            assert!(select.option_glyphs.contains(&g), "{g}");
        }
        let (text, _) = painted(
            r#"{"name":"t","kind":"text","font":"Inter-Regular","value":"A"}"#,
            "",
        );
        assert!(text
            .option_glyphs
            .iter()
            .all(|g| select.option_glyphs.contains(g)));
        // A read-only choice cannot change.
        let (fixed, _) = painted(
            r#"{"name":"plan","kind":"select","font":"Inter-Regular","options":["Starter","Quokka"],"value":"Starter","readOnly":true}"#,
            "",
        );
        assert!(fixed.option_glyphs.is_empty() && !fixed.editable);
    }
}
