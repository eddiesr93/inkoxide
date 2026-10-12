//! Interactive form fields (`AcroForm`): intrinsic size and the
//! widget ops painting hands to the PDF writer.

use taffy::{AvailableSpace, Size as TSize};

use crate::fonts::{FontKey, FontSet};
use crate::layout::Geometry;
use crate::paint::{ContentCtx, Op, Rect, TextGlyph, TextOp};
use crate::schema::{Field, FieldKind, BLACK};

/// Inner padding between the widget edge and its text, points.
pub(crate) const PAD: f32 = 2.0;
/// Default width of text and select fields without one, points.
const DEFAULT_WIDTH: f32 = 150.0;
/// Visible lines of a multiline field without a height.
const MULTILINE_ROWS: f32 = 3.0;
/// Line height of a field's text, as a multiple of its font size.
const LINE_HEIGHT: f32 = 1.2;

/// Characters whose glyphs are embedded for every font a text or select
/// field uses, so a viewer can show edits made in the common Latin range
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
    /// Glyphs embedded for editing (see `EDIT_RANGES`), and a select's
    /// options.
    pub edit_glyphs: Vec<u16>,
    /// The transform in effect where the widget is painted (`None`:
    /// identity); the PDF writer sets it.
    pub ctm: Option<[f32; 6]>,
}

/// Content size of a field without explicit dimensions.
pub(crate) fn measure(f: &Field, available: TSize<AvailableSpace>) -> TSize<f32> {
    let line = f.size * LINE_HEIGHT;
    match f.kind {
        FieldKind::Text | FieldKind::Select => TSize {
            width: match available.width {
                AvailableSpace::Definite(w) => w.min(DEFAULT_WIDTH),
                _ => DEFAULT_WIDTH,
            },
            height: if f.multiline {
                line * MULTILINE_ROWS + 2.0 * PAD
            } else {
                line + 2.0 * PAD
            },
        },
        FieldKind::Checkbox | FieldKind::Radio => TSize {
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
    let top = cy + ctx.shift;
    if ctx.window.is_some_and(|w| w.hides(top, ch)) {
        return;
    }
    let rect = Rect::new(cx + ctx.dx, top + ctx.dy, cw.max(0.0), ch.max(0.0));
    let font = f.font.as_deref().and_then(|id| fonts.key(id));
    let mut text = Vec::new();
    let mut edit_glyphs = Vec::new();
    if let Some(key) = font {
        let lf = fonts.get(key);
        let m = &lf.data.metrics;
        let scale = lf.scale(f.size);
        let (asc, desc) = (m.ascender * scale, -m.descender * scale);
        let color = f.color.unwrap_or(BLACK);
        let shown = match (f.kind, f.password) {
            (FieldKind::Text | FieldKind::Select, false) => f.value.as_deref().unwrap_or(""),
            _ => "",
        };
        let lines: Vec<&str> = if f.multiline {
            shown.split('\n').collect()
        } else {
            vec![shown.lines().next().unwrap_or("")]
        };
        let line_h = f.size * LINE_HEIGHT;
        for (i, line) in lines.iter().enumerate() {
            let baseline = if f.multiline {
                PAD + (line_h - asc - desc) / 2.0 + asc + i as f32 * line_h
            } else {
                (ch - asc - desc) / 2.0 + asc
            };
            let mut pen = PAD;
            let mut glyphs = Vec::new();
            for c in line.chars() {
                let Some(id) = lf.glyph(c) else { continue };
                glyphs.push(TextGlyph {
                    id,
                    x: pen,
                    y_offset: 0.0,
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
        if matches!(f.kind, FieldKind::Text | FieldKind::Select) && !f.read_only {
            for &(a, b) in EDIT_RANGES {
                edit_glyphs.extend((a..=b).filter_map(|c| lf.glyph(c)));
            }
            // A choice shows any option, whatever its script.
            for o in &f.options {
                edit_glyphs.extend(o.chars().filter_map(|c| lf.glyph(c)));
            }
        }
    }
    ops.push(Op::Field(Box::new(FieldOp {
        rect,
        field: f.clone(),
        font,
        text,
        edit_glyphs,
        ctm: None,
    })));
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
        };
        let mut ops = Vec::new();
        paint(f, &geo, &ctx, &fonts, &mut ops);
        let Some(Op::Field(op)) = ops.pop() else {
            panic!("no field op");
        };
        let glyphs = chars.chars().map(|c| fonts.get(0).glyph(c).unwrap());
        (*op, glyphs.collect())
    }

    #[test]
    fn select_fields_embed_the_latin_range_and_every_option() {
        let (select, wanted) = painted(
            r#"{"name":"plan","kind":"select","font":"Inter-Regular","options":["Starter","Quokka","Ωmega"],"value":"Starter"}"#,
            "Qké€Ω",
        );
        for g in wanted {
            assert!(select.edit_glyphs.contains(&g), "{g}");
        }
        let (text, _) = painted(
            r#"{"name":"t","kind":"text","font":"Inter-Regular","value":"A"}"#,
            "",
        );
        assert!(text
            .edit_glyphs
            .iter()
            .all(|g| select.edit_glyphs.contains(g)));
        // A read-only choice cannot change.
        let (fixed, _) = painted(
            r#"{"name":"plan","kind":"select","font":"Inter-Regular","options":["Starter","Quokka"],"value":"Starter","readOnly":true}"#,
            "",
        );
        assert!(fixed.edit_glyphs.is_empty());
    }
}
