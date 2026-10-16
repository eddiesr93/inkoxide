# Plans for "Next"

Draft for review, written on 2026-10-13 against `main` at `f038fed`; line references point
to that commit. One plan per group of [ROADMAP.md](../../../ROADMAP.md) items that touch the
same code:

| Plan | Items | Effort (working days) |
|---|---|---|
| [PDF output](pdf-output.md) | Encryption; PDF/A-1b and PDF/A-3b | 7–8; 3b ½, 1b 4–5 |
| [Vector graphics](vector-graphics.md) | Text in `<Svg>` and `<Marker>`; `<Canvas>` text, clipping and gradients | Marker 1, text about 20; Canvas 4 |
| [Images in text](images-in-text.md) | Inline images; colour emoji; `<ImageBackground>` | 5–7; 11–14; 3 |
| [Line layout](line-layout.md) | Floats; optimal line breaking | 15–18; 7–9 |
| [Forms and notes](forms-and-notes.md) | Field groups, list boxes, number and date formats; `<Note>` | 1½, 2½, 4–5; 3 |

In total about 90–100 working days.

## What the plans found

- **`<Marker>` already works in SVG files.** usvg converts markers and the SVG checks charge
  them once per vertex. Only the React component is missing.
- **Canvas clipping, gradients and `miterLimit` need no engine work.** The painter records SVG,
  and the SVG writer already draws clip paths, gradients (with `fr`) and miter limits.
- **SVG text stays out of usvg.** usvg's `text` feature depends on harfrust 0.12, which waits
  with #83 for a release that includes harfrust#484, and shaping inside usvg would run outside
  the render's glyph limit. Inkoxide lays out SVG text with its own shaper instead (decided on
  2026-10-13), so text in `<Svg>`, and Canvas text, which depends on it, do not wait for #83.
- **pdf-writer has no encryption.** Strings are written through sealed types, and the trailer
  holds only `Root`, `Info` and `ID`. Encryption needs one module that writes every string and
  stream, enforced by clippy, and an `/Encrypt` entry spliced into the trailer.
- **Taffy's floats do not fit.** Taffy 0.14 has floats, but they are turned off here, its measure
  callback gets no float context, and it floats only in block containers. Float containers get
  their own layout, the way tables do.
- **`break_lines` takes one width per paragraph.** Floats and optimal breaking both need a width
  per line.

## Shared foundations

Several items need the same change. Each one lands first, on its own, with byte-identical
output, so the feature commits stay small.

| Foundation | Needed by | Plan |
|---|---|---|
| `pdf/out.rs`: one module writes every PDF string and stream; clippy forbids the rest | Encryption. Every later string or stream (notes, list boxes, formats, emoji, SVG text) then goes through it | PDF output |
| A PDF/A `Profile` instead of `pdf_a: bool` | PDF/A-1b and 3b; leaving form scripts out under PDF/A; refusing encryption with PDF/A | PDF output |
| `ImageSet::paint_in` | Inline images, background images, emoji images | Images in text |
| `Op::Span` with `/ActualText` | Emoji, `alt` on inline images; later tagged PDF | Images in text |
| Run-level `verticalAlign` and baseline shift | Superscript and subscript (Now); inline images | Images in text |
| `LineBoxes`: a width per line in `break_lines` | Floats; optimal line breaking | Line layout |
| `FontSet::select`: CSS font matching in the engine | SVG text; Canvas text | Vector graphics |

## Interactions

- PDF/A-1b forbids transparency. Background images, inline images, emoji with alpha and Canvas
  gradients with opacity all go through the 1b transparency check.
- PDF/A forbids JavaScript: number and date formats keep their appearance there but lose their
  scripts. PDF/A also forbids encryption; asking for both is an error.
- Inline images and emoji enter a paragraph as glyph-like objects, so optimal line breaking
  needs no special case for them.
- A `<Note>` inside `Text` waits for inline images.
- "Text outlines in `debug`" (Now) lands before floats and inline images, which then extend it.
- Colour emoji inside SVG text stay monochrome until both exist.

## Suggested order

Dependencies first, then what blocks a migration, which is the order ROADMAP.md gives.

1. **Now:** text outlines in `debug`; superscript, subscript and `fontFeatureSettings`.
2. **Foundations** (no visible change): `pdf/out.rs`, `Profile`, `ImageSet::paint_in`,
   `LineBoxes`.
3. **Small and independent** (about 3 weeks): `<Marker>`; Canvas clipping, gradients and
   `miterLimit`; `<ImageBackground>`; PDF/A-3b; field groups; list boxes; `<Note>`.
4. **Migration blockers** (about 6 weeks): encryption; inline images; optimal line breaking;
   PDF/A-1b; number and date formats.
5. **Large** (about 10 weeks): colour emoji; floats; text in `<Svg>`, then Canvas text.

Each step can ship in its own minor release. Each plan lists its commits, each one green.

## Decided

Taken on 2026-10-13:

1. **SVG text:** laid out by Inkoxide with its own shaper, now, as real PDF text. usvg's `text`
   feature stays off; each `<text>` becomes a placeholder path before usvg converts the file.
2. **Encryption keys:** AES-256 only, with keys derived deterministically from the document and
   the passwords, so the same input still gives the same bytes. The derivation is not the
   standard one and needs a cryptography review before release.
3. **PDF/A-1b and transparency:** rejected, with the path of the element in the document.
4. **Order:** the waves above.

## Still open

Every plan ends with its own open decisions. These are the ones that change the design, each
with the recommendation:

1. **Boxes beside a float:** narrowed, like CSS (recommended), or overlapping the float.
2. **Optimal line breaking:** opt-in through `textWrap: 'pretty'` (recommended), or the default.
3. **Emoji images:** substituted by the engine from a document-level `emojiImages` map, with URLs
   checked against the remote image allowlist (recommended), or split into image runs by React.
4. **Field groups:** dotted field names (recommended), or a group node in the schema.
5. **Passwords:** never in the document, only as `<Document>` props and a render option, which
   wins (recommended).
