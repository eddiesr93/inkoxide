# Architecture

Inkoxide is a pure function `render(document_tree, options) -> pdf_bytes`,
with a React front end that builds the document tree. This page describes
how the pieces fit together, for contributors.

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | The engine, crate `inkoxide` on crates.io. `#![forbid(unsafe_code)]`, no I/O, no napi. |
| `crates/cli` | `inkoxide render / validate / bench`, the command line that does the file I/O: crate `inkoxide-cli`. |
| `crates/node` | The napi-rs binding, published as the npm package `inkoxide`. |
| `packages/inkoxide` | The npm package for the binding: loader, types and tests. The tests are the contract of both bindings. |
| `crates/wasm` | The WebAssembly binding, through a small C ABI of its own (see its `lib.rs`), for the npm package `@inkoxide/wasm`. |
| `packages/wasm` | The npm package for it: loader, types; it runs the tests of `packages/inkoxide`. |
| `crates/binding` | What the two bindings share: the render options as JavaScript passes them, their checks, and errors as JSON. |
| `packages/schema` | The document schema (`document.v1.json`), its TypeScript types and changelog: `@inkoxide/schema`. |
| `packages/style` | The flat style object, CSS units, and the readers of lengths, keywords, colours, `flex`, transforms, positions and page media queries that React and Tailwind share: `@inkoxide/style`. |
| `packages/tailwind` | Tailwind class strings to engine styles, through the Tailwind v4 compiler: `@inkoxide/tailwind`, optional. |
| `packages/react` | React components and a reconciler that build document trees: `@inkoxide/react`. |
| `packages/render` | `renderToBuffer`, `renderToFile`, `Font.register` and the bundled fonts: `@inkoxide/render`; templates served by id: `@inkoxide/render/templates`; in a browser, with the engine as WebAssembly in a Web Worker: `@inkoxide/render/browser`. The shared code reaches the engine and the bundled assets through an `EngineHost` (`host.ts`): `node-host.ts` in Node, `browser-host.ts` and `worker.ts` in a browser. |
| `examples` | Example documents in React; they produce the screenshots in the READMEs. |
| `benchmarks` | Benchmarks of `@inkoxide/render` on an invoice and a report, on the server and in the browser. |
| `fixtures` | Engine test documents, fonts, images and reference renders. |
| `fuzz` | cargo-fuzz targets (nightly, outside the Cargo workspace). |

The npm packages depend on each other like this:

- `@inkoxide/schema` and `@inkoxide/style` depend on no other Inkoxide package
  (`@inkoxide/style` only on culori).
- `@inkoxide/react` depends on `@inkoxide/schema` and `@inkoxide/style`.
- `@inkoxide/render` depends on `@inkoxide/react`, `@inkoxide/schema` and `inkoxide`.
- `@inkoxide/tailwind` depends on `@inkoxide/style`. `@inkoxide/react` and
  `@inkoxide/render` load it only when it is installed (an optional peer dependency), for
  `className`.
- `@inkoxide/wasm` is an optional peer dependency of `@inkoxide/render`, for
  `@inkoxide/render/browser`.

## From React to a document tree

`@inkoxide/react` is a React reconciler whose host components (`Document`,
`Page`, `View`, `Text`, `Image`, `Table`, …) build an in-memory tree. After
React commits, the tree is converted into a schema v1 document: `className`
strings go through `@inkoxide/tailwind` when it is installed (the real
Tailwind compiler turns classes into CSS, and the CSS declarations are
mapped to engine styles), style objects are read with the same readers of
lengths, keywords, colours, `flex`, transforms and positions from
`@inkoxide/style` (`em`, `vw` and `vh` resolve against the element's font
size and the page in both), and fonts are resolved against the font
registry with CSS font matching. `@media` blocks of style objects and page
variants of classes are answered there, for the page each element is on
(`@inkoxide/style`'s `parseMedia`): the blocks and declarations that match
are applied, the others only checked, so the document holds no query. In development the tree is validated
against the JSON Schema, and errors carry the component stack.

`@inkoxide/render` registers the bundled fonts with the engine once per
process, renders the tree, and, when the document uses `render` props that
need the page count, renders again with the page count until it settles
(at most four renders).

## Pipeline

```
bytes ──serde──▶ schema::Document ──validate──▶ (arena tree) ──layout──▶ boxes
      ──paginate──▶ pages ──paint──▶ display lists ──pdf──▶ bytes
```

| Stage | Module | What it does |
|---|---|---|
| Parse | `validate` (`parse`), `schema` | JSON or MessagePack into the typed structs of `schema` (hand-written deserialisers in `schema/de.rs`); serde errors become validation errors with their path and code |
| Validate | `validate/rules/` | Semantic checks, a file per area (fields, images, links, styles, tables, text): font ids, image ids (images must decode), anchors, value ranges |
| Fonts | `fonts`, `pdf/font.rs` | Registry and metrics, WOFF/WOFF2 decoding, subsetting, CFF programs (`fonts/cff.rs`); Type0/CID embedding of TrueType and CFF, ToUnicode |
| Text | `text` | rustybuzz shaping, UAX #14 line breaking, hyphenation, font fallback, bidi, justification |
| Layout | `layout` | An arena tree laid out by Taffy (flexbox and grid), with a fast path for simple table cells |
| Tables | `layout/table.rs` | Column widths, spans, collapsed borders; splitting with repeated header rows |
| Floats | `layout/flow.rs`, `layout/float.rs` | Float containers (the root, a column box or a cell with floated children), Taffy leaves like tables: children stacked as in a column, floats placed, text wrapped beside them, boxes narrowed |
| Pagination | `paginate`, `pipeline` | Breaks flows into pages: orphans and widows, keep-together, forced breaks; page numbers (`pipeline/numbering.rs`) |
| Paint | `paint` | Display lists: backgrounds, borders, radii, opacity, transforms, clipping |
| PDF | `pdf` | Pages and content streams (encoded in parallel, `pdf/content.rs`), the document around them (`pdf/document.rs`: page tree, resources, catalog), links and outline (`pdf/nav.rs`), notes, forms, PDF/A-1b, 2b and 3b, encryption. Objects go into the file through one `Writer` (the file, the object numbers, `Out`). Every string and stream goes into the file through `pdf/out.rs`, which encrypts it when the file is encrypted; `crates/core/clippy.toml` forbids pdf-writer's `TextStr`, `TextStrWithLang`, `Date`, the other writers that make strings of their own, and its stream writers anywhere else. Two exceptions, both in `pdf/crypt.rs`: `Dict::write` writes the strings of the `Encrypt` dictionary as they are, since the standard leaves them unencrypted, and `splice_encrypt` adds the `/Encrypt` entry to the trailer pdf-writer finished, which has no setter for it |
| Images | `images` | JPEG passthrough, PNG with SMask, GIF and WebP, SVG as vector form XObjects, SVG text read and replaced with probes (`images/svg_text`) |
| SVG | `images/svg/` | Parsing through usvg with limits for untrusted input: what a file's copies, markers and references expand to is checked before usvg converts it (`check.rs`, against `limits.rs`); the root `viewBox` (`view_box.rs`); the text converted again with a render's fonts, cached across renders (`text.rs`) |
| Forms | `forms` | AcroForm fields: intrinsic sizes, the widget ops painting hands to the PDF writer, and field formats (`forms/format.rs`) |
| Notes | `notes`, `icons` | Text notes, from `note` nodes and note runs, as icon annotations: shown at a fixed size at a point |
| Attachments | `attachments`, `pdf/attach.rs` | Files a document carries: `AttachmentStore` holds the caller's bytes by id (or only the size of a file too large to read), `attachments::prepare` decodes and compresses each once per render, after layout; the writer embeds them as file specifications, with icon annotations and, under PDF/A-3b, associated files |
| Geometry | `geom` | Rectangles and 2×3 matrices, shared by layout, pagination, fonts and paint |
| Support | `date`, `pool`, `lru` | RFC 3339 parsing for `creationDate`; the thread pools of `threads`; the size-bounded LRU map of the process-wide caches |

## Public API (core)

```rust
pub fn validate(doc: &[u8], format: InputFormat, fonts: &FontStore, images: &ImageStore) -> Vec<ValidationError>;
pub fn validate_without_images(doc: &[u8], format: InputFormat, fonts: &FontStore) -> Vec<ValidationError>;
pub fn render(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore, images: &ImageStore)
    -> Result<Vec<u8>, InkoxideError>;
pub fn clear_caches();
```

Besides these, the crate exports only their inputs and outputs (`RenderOptions`,
`PdfALevel` and its `ParsePdfALevelError`, `Encryption`, `Permissions`,
`Printing`, `InputFormat`, `FontStore`, `ImageStore`, `AttachmentStore`, `HyphenationStore`,
`InkoxideError` and its `NotLoaded`, `ValidationError`, `ValidationCode`),
`builtin_hyphenation_file`, and the constants `ENGINE_VERSION` and
`SCHEMA_VERSION`. `JSON_ONLY`, the message the bindings share for
MessagePack input to a JSON-only build, is public but `#[doc(hidden)]`.
Every module is private, so nothing else is reachable from outside the
crate.

## Layout data model

`layout::Tree` is an arena (`Vec<LNode>`) in breadth-first order, so each
node's children are a contiguous index range. Index 0 is a synthetic root:
the page content box (column flex, fixed width, unbounded height). After
`layout::compute` every node holds absolute `Geometry` (border box,
plus resolved border, padding and margin) in flow coordinates; paint adds the page
margin offset.

## Text pipeline

```
runs ─▶ prepare (whitespace, soft hyphens, font per char, rustybuzz) ─▶ Prepared   (cached per node)
Prepared + LineBoxes ─▶ break_lines (per chunk: greedy or optimal, metrics, maxLines/ellipsis) ─▶ LineLayout (cached per boxes)
LineLayout ─▶ TextPainter (align, justify, placeholders, decorations) ─▶ Op::Text
Op::Text in a colour font ─▶ ColorGlyphs ─▶ Op::Span + Op::Image + Op::InvisibleText
```

`break_lines` breaks each chunk between forced breaks on its own, first
fit (`lines.rs`) or, for `textWrap: pretty`, with Knuth and Plass's
total fit (`optimal.rs`), which falls back to first fit past its caps.
Lines are laid in `LineBoxes` (`layout/float.rs`): the content width,
and bands where floats take room on a side, where each line finds its
box at its own top. A float container's layout records the line boxes of
the text beside its floats (`Tree.wraps`); pagination, page numbers and
the painter look the lines up in them.

A run's `fontFeatures` go to rustybuzz with its words; shaped words and
shape plans are cached per feature set too. A run's `verticalAlign` moves
its baseline (`text::baseline_shift`, from the font's OS/2 superscript and
subscript metrics): line metrics count the moved glyph boxes, and the
painter draws the run, its decorations and its `debug` box there.

An image run is an inline object (`text/objects.rs`): one glyph-like
cluster in a segment of its own (U+FFFC in the text, so UAX #14 breaks
around it), as wide as the image, so line breaking, `textWrap: pretty` and
ellipses need no special case. Line metrics add its extent where its
`verticalAlign` puts it (`top` and `bottom` after the rest of the line),
and the painter draws its image there, in a span with its `alt`.

Itemisation picks a font per character, except in emoji sequences
(`text/emoji.rs`, UTS #51: flags, keycaps, skin tones, tag and ZWJ
sequences), which take one font each so the font's ligature can form; a
sequence with emoji presentation tries the colour fallback fonts first.
After shaping, the glyphs text uses from colour fonts become images once
per render (`text/color.rs`): `fonts/color.rs` writes `COLR` v0 and v1
glyphs as SVG, under a work budget, and takes PNG bitmaps from `CBDT` and
`sbix`; they decode through `images` like document images. A `COLR`
glyph with layers in the foreground colour becomes one image per run
colour it is drawn in. The painter
then replaces each colour glyph with its image, in a `/Span` whose
`/ActualText` is the glyph's text, with an invisible space for text
extraction. A font with colour bitmaps and no outlines is never embedded.

Two third-party crates stay behind one module each, so replacing them
touches only that module: ttf-parser (font parsing) is used only in
`fonts`, which gives the rest of the engine metrics, glyph ids and
advances; rustybuzz (shaping) only in `text/prepare.rs`, which parses its
shaping faces from the font bytes once per render and shares them between
threads. One check knows rustybuzz without using it: `fonts/layout.rs`
refuses at registration the fonts whose `GSUB` or `GPOS` coverage offsets
rustybuzz would unwrap, read with ttf-parser, so replacing rustybuzz
means checking that list again. `pdf` depends on `fonts` and `images`,
never the reverse, and `text` does not depend on `pdf`: the pipeline asks
`pdf` what a colour glyph's image (from `ColorGlyphs`) needs for PDF/A,
and how many bytes the text of a node's images takes. `fonts`
subsets font programs and caches the compressed whole program, and the PDF
writer supplies the compression; the writer and `images` both compress
through `flate.rs`.

Tagged output (`tag.rs`, `pdf/tag.rs`) plans the structure from the
document before layout: one element per node that is one (a paragraph, a
heading, a figure, a table and its rows and cells, a field), keyed by the
node's address, in document order, which is the reading order. Painting
pushes `Op::Tag` around what a node's content painter draws; the writer
opens a marked-content sequence with an MCID when something is drawn under
a tag, and an artifact when something is drawn outside one (backgrounds,
borders, headers, footers and nodes painted on every page, which the plan
leaves out). Annotations take the element in effect, or one of their own.
The tree, the parent tree and the elements are written last, so untagged
output is byte for byte what it was.

A variable font is registered once; each declaration with `variations`
draws with an `Instance` of it (`fonts/mod.rs`): canonical coordinates
(clamped, in tag order, defaults left out), metrics read by ttf-parser at
those coordinates (it applies `MVAR`), and a digest that keys the program
caches. Layout reads the ttf-parser face at the coordinates, and shaping
gives rustybuzz the same ones. Every program embedded is the instance as
static TrueType, made by subsetter's instancer (skrifa underneath), CFF2
outlines included. Two parsers read the font, so registration refuses
what they read differently: more than 63 axes (ttf-parser then applies
none) and `avar` 2 (which ttf-parser does not read).

SVG text goes through the same shaper. `images/svg_text` reads each
`<text>` of an SVG once, when the SVG is decoded: the cascade of its text
properties (simplecss and svgtypes are used only in its `extract.rs`, at
the versions usvg uses), white space and character positions. Each render
lays the text out with its fonts before layout (`text/svg.rs`, one
`TextShaper::prepare` call per chunk, so fallback fonts, bidi, the glyph
limit and ToUnicode apply), replaces each `<text>` with a "probe" path
over its ink, and converts the SVG again (cached by the rewritten source).
usvg resolves the probes' paint; the PDF writer draws each probe's text in
its place as PDF text (`pdf/svg.rs`). See
[ADR 0001](adr/0001-svg-text-probes.md).

`TextCache` is the layout engine's `LeafMeasure` for text nodes: Taffy asks
for min-content / max-content / definite widths and gets the paragraph's
size. Glyph → Unicode mappings are collected while shaping and become the
ToUnicode CMaps, so text is selectable and searchable; colour glyphs carry
theirs as `ActualText`. Form fields are drawn without shaping: the glyphs
their appearances show, and those embedded for editing, are added to the
same maps when the PDF is written (`FontPrograms::field_text`).

## Pagination and paint

`pipeline::run` lays out every page template (flow, header, footer,
overlays), paginates each flow, then paints page by page: background →
overlays with a negative `zIndex` → flow (clipped to the content band,
only nodes and lines on the page) → fixed nodes → the other overlays →
header → footer. An overlay is a page child that is fixed and absolutely
positioned: it is laid out on its own against the page content box, like
a band, instead of in the flow. Overlays are painted by `zIndex`, then in
document order. `FlowPart` tells
the box painter which part of a flow to paint; leaf painters get a
`ContentCtx` with the node's shift, per-line shifts and the page window.
Elements with `debug` have their boxes recorded as they are painted
(`paint/debug.rs`), a paragraph's with the lines and runs the text painter
placed (`ContentPainter::paint_outlined`); the outlines are drawn last,
over everything, the flow's clipped to its content band.

Page-number placeholders are laid out as `999` until the page count is
known. Then flow text that shows them gets the numbers of the page its
first number lands on (`TextCache::set_numbers`, through `fill_numbers`),
and the flows with such text are laid out and paginated again, until no
number changes (at most three times; `number_flows` in
`pipeline/numbering.rs`). Text in a
fixed node or a table's header rows is painted on several pages and keeps
the stand-ins' width. The painter always draws the page's own digits. A
header, footer or overlay that shows page numbers is laid out again on each page,
in the page's paint job: `text::PageText` puts the page's digits in place
of the stand-ins and breaks those paragraphs' lines again, and reads every
other paragraph from the shared cache.

## Testing

See [TESTING.md](TESTING.md): unit and fixture tests (byte-determinism,
JSON/MessagePack parity, `qpdf --check`), schema parity, visual
regression, the Node suite, benchmarks and fuzzing.
