# Architecture

Inkoxide is a pure function `render(document_tree, options) -> pdf_bytes`,
with front ends for React, Vue, Svelte and Angular that build the document
tree. This page describes how the pieces fit together, for contributors.

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | The engine, crate `inkoxide` on crates.io. `#![forbid(unsafe_code)]`, no I/O, no napi. |
| `crates/cli` | `inkoxide render / validate / bench`, the command line that does the file I/O: crate `inkoxide-cli`. |
| `crates/node` | The napi-rs binding, published as the npm package `inkoxide`. |
| `packages/inkoxide` | The npm package for the binding: loader, types and tests. The tests are the contract of both bindings. |
| `crates/wasm` | The WebAssembly binding, through a small C ABI of its own (see its `lib.rs`), for the npm package `@inkoxide/wasm`; built a second time with threads, whose pool (`threads.rs`) and allocator (`alloc.rs`) it has ([ADR 0003](adr/0003-browser-threads.md)). |
| `packages/wasm` | The npm package for it: loader (`index.js`), the pool's worker (`thread.js`), types; it runs the tests of `packages/inkoxide`. |
| `crates/binding` | What the two bindings share: the render options as JavaScript passes them, their checks, and errors as JSON. |
| `packages/schema` | The document schema (`document.v1.json`), its TypeScript types and changelog: `@inkoxide/schema`. |
| `packages/style` | The flat style object, CSS units, and the readers of lengths, keywords, colours, `flex`, transforms, positions and page media queries that the front ends and Tailwind share: `@inkoxide/style`. |
| `packages/tailwind` | Tailwind class strings to engine styles, through the Tailwind v4 compiler: `@inkoxide/tailwind`, optional. In a browser bundle, its build plugin (`./plugin`) puts a table of the classes the sources use in place of the compiler (`./precompiled`): for Vite, Rollup, Rolldown, esbuild, webpack and Rspack (a loader, `webpack-loader.ts`, fills the table module from the scans `webpack-scans.ts` keeps) and Next.js (`withInkoxideTailwind`, which writes the table for Turbopack's `resolveAlias` and adds the webpack plugin to client builds). `sources.ts` reads the `@source` directives of the theme for the scan. |
| `packages/tree` | What every front end shares, `@inkoxide/tree`: the host types and props of the components (`components/`), where each may go (`placement.ts`), host nodes (`nodes.ts`), and the builder that turns them into a document tree (`tree/`), with the font registry, Canvas, Factur-X and errors. See [ADR 0004](adr/0004-front-ends.md). |
| `packages/render` | The engine for every front end, `@inkoxide/render`: `Font.register` and the bundled fonts, image loading, the passes for the page count (`pipeline.ts`), `renderDocument`, and `createRenderer(frontEnd)`, which gives a front end its `renderToBuffer`, `renderToFile`, `renderToStream`, `generate` and `renderTree`; in a browser, with the engine as WebAssembly in a Web Worker, `@inkoxide/render/browser`; templates served by id, `@inkoxide/render/templates`. The shared code reaches the engine and the bundled assets through an `EngineHost` (`host.ts`): `node-host.ts` in Node, `browser-host.ts` and `worker.ts` in a browser. |
| `packages/react` | The React front end, `@inkoxide/react`: components, a reconciler that builds host nodes, and the render functions; `./browser` with `usePdf`, `PdfViewer`, `PdfDownloadLink` and `BlobProvider`; `./templates`. Its README and guide document the components for every front end. |
| `packages/vue` | The Vue front end, `@inkoxide/vue`: a custom renderer (`createRenderer`) over host nodes, components, the render functions, `./browser`, `./templates`, and `./vite`, which compiles single-file templates for the client in a server build. |
| `packages/svelte` | The Svelte front end, `@inkoxide/svelte`: components (generated) that render `ink-*` elements and put their props in a table of the render; the HTML of `svelte/server`, or the DOM `mount` builds in a browser, is read back into host nodes. |
| `packages/angular` | The Angular front end, `@inkoxide/angular`: a `RendererFactory2` over host nodes, for template components that use the `ink-*` elements; `./browser` with `injectPdf`. |
| `packages/conformance` | Not published: the examples and a document of features written with each front end, which must build the document and the PDF React builds; the names each exports and its components; a server build of the Vue and Svelte templates. |
| `examples` | Example documents in React; they produce the screenshots in the READMEs. `examples/browser` renders them in a browser with `@inkoxide/react/browser`, and `examples/frameworks` the invoice with the Vue, Svelte and Angular front ends. |
| `benchmarks` | Benchmarks of `@inkoxide/render` on an invoice and a report, on the server and in the browser. |
| `fixtures` | Engine test documents, fonts, images and reference renders. |
| `fuzz` | cargo-fuzz targets (nightly, outside the Cargo workspace). |

The npm packages depend on each other like this:

- `@inkoxide/schema` depends on no other Inkoxide package. `@inkoxide/style` depends on
  `@inkoxide/schema`, for the types of the styles it reads, and on culori.
- `@inkoxide/tree` depends on `@inkoxide/schema` and `@inkoxide/style`.
- `@inkoxide/render` depends on `@inkoxide/tree`, `@inkoxide/schema` and `inkoxide`.
- `@inkoxide/react`, `@inkoxide/vue`, `@inkoxide/svelte` and `@inkoxide/angular` depend on
  `@inkoxide/render`, `@inkoxide/tree`, `@inkoxide/schema` (types) and zod (templates served by
  id). Their framework is a peer dependency of the published package (`react`, with
  `react-reconciler` a dependency; `vue`; `svelte`; `@angular/core`); the workspace pins React as
  a dependency, and `scripts/release/stage.mjs` makes it a peer.
- `@inkoxide/tailwind` depends on `@inkoxide/style`. `@inkoxide/tree` loads it only when it is
  installed (an optional peer dependency), for classes.
- `@inkoxide/wasm` is an optional peer dependency of `@inkoxide/render` and the front ends, for
  their browser entries.

## From a template to a document tree

Each front end renders a template once into host nodes of `@inkoxide/tree`: an element per
component (`{ type: 'ink-view', props, children }`) and a text node per string. React does it
with a reconciler whose host components are the components; Vue with a custom renderer; Angular
with a `RendererFactory2`; Svelte, which has no custom renderers yet, by reading back what it
rendered ([ADR 0004](adr/0004-front-ends.md)). The template front ends join and trim their text
as JSX does (`trimTemplateText`), so that a document gives the same nodes in every framework.
`checkTree` (or React's host config, as it creates them) checks where each component is. A node
keeps an `origin` and a `trace` that names the components above it, for the component stacks of
errors, and a `render` prop is rendered for each page by the front end (`renderPage`).

`buildDocument` then converts the nodes into a schema v1 document: `className`
strings go through `@inkoxide/tailwind` when it is installed (the real
Tailwind compiler turns classes into CSS, and the CSS declarations are
mapped to engine styles; in a browser bundle built with its plugin, a
table of the classes the sources use, made by the same compiler at build
time, stands in for it), style objects are read with the same readers of
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
| Validate | `validate/rules/` | Semantic checks, a file per area (attachments, fields, images, links, styles, tables, tagging, text): font ids, image ids (images must decode), anchors, value ranges |
| Fonts | `fonts`, `pdf/font.rs` | Registry and metrics, WOFF/WOFF2 decoding, subsetting, CFF programs (`fonts/cff.rs`); Type0/CID embedding of TrueType and CFF, ToUnicode |
| Text | `text` | rustybuzz shaping, UAX #14 line breaking, hyphenation, font fallback, bidi, justification |
| Layout | `layout` | An arena tree laid out by Taffy (flexbox and grid), with a fast path for simple table cells |
| Tables | `layout/table.rs` | Column widths, spans, collapsed borders; splitting with repeated header rows |
| Floats | `layout/flow.rs`, `layout/float.rs`, `layout/shape.rs` | Float containers (the root, a column box or a cell with floated children), Taffy leaves like tables: children stacked as in a column, floats placed, text wrapped beside them (around a `shapeOutside`, in strips), column boxes laid out beside them with the floats seeded in, other boxes narrowed |
| Pagination | `paginate`, `pipeline` | Breaks flows into pages: orphans and widows, keep-together, forced breaks; page numbers (`pipeline/numbering.rs`) |
| Paint | `paint` | Display lists: backgrounds, borders, radii, opacity, transforms, clipping |
| PDF | `pdf` | Pages and content streams (encoded in parallel, `pdf/content.rs`), the document around them (`pdf/document.rs`: page tree, resources, catalog), links and outline (`pdf/nav.rs`), notes, forms, the structure tree of tagged PDF and PDF/UA-1 and 2 (`pdf/tag.rs`), PDF/A-1b, 2b, 2a, 3b and 3a, encryption. Objects go into the file through one `Writer` (the file, the object numbers, `Out`). Every string and stream goes into the file through `pdf/out.rs`, which encrypts it when the file is encrypted; `crates/core/clippy.toml` forbids pdf-writer's `TextStr`, `TextStrWithLang`, `Date`, the other writers that make strings of their own, and its stream writers anywhere else. The exceptions: in `pdf/crypt.rs`, `Dict::write` writes the strings of the `Encrypt` dictionary as they are, since the standard leaves them unencrypted, and `splice_encrypt` adds the `/Encrypt` entry to the trailer pdf-writer finished, which has no setter for it; in `pdf/mod.rs`, `Writer::packable_text` gives the strings of objects bound for object streams as they are, since the object stream is encrypted as a whole (ISO 32000-2, 7.6.2); and in `pdf/objstm.rs`, pdf-writer writes the cross-reference stream, which the standard leaves unencrypted |
| Images | `images` | JPEG passthrough, PNG with SMask (a stencil mask for binary alpha at PDF/A-1b), GIF and WebP, SVG as vector form XObjects, SVG text read and replaced with probes (`images/svg_text`) |
| SVG | `images/svg/` | Parsing through usvg with limits for untrusted input: what a file's copies, markers and references expand to is checked before usvg converts it (`check.rs`, against `limits.rs`), and the path segments the converted tree draws after (`limits.rs`); the root `viewBox` (`view_box.rs`); the text converted again with a render's fonts, cached across renders (`text.rs`) |
| Forms | `forms` | AcroForm fields: intrinsic sizes, the widget ops painting hands to the PDF writer, and field formats (`forms/format.rs`) |
| Notes | `notes`, `icons` | Text notes, from `note` nodes and note runs, as icon annotations: shown at a fixed size at a point |
| Attachments | `attachments`, `pdf/attach.rs` | Files a document carries: `AttachmentStore` holds the caller's bytes by id (or only the size of a file too large to read), `attachments::prepare` decodes and compresses each once per render, after layout; the writer embeds them as file specifications, with icon annotations and, under PDF/A-3b and 3a, associated files |
| Geometry | `geom` | Rectangles and 2×3 matrices, shared by layout, pagination, fonts and paint |
| Support | `date`, `pool`, `lru` | RFC 3339 parsing for `creationDate`; the thread pools of `threads`; the size-bounded LRU map of the process-wide caches |

## Public API (core)

```rust
pub fn validate(doc: &[u8], format: InputFormat, fonts: &FontStore, images: &ImageStore) -> Vec<ValidationError>;
pub fn validate_without_images(doc: &[u8], format: InputFormat, fonts: &FontStore) -> Vec<ValidationError>;
pub fn validate_with_options(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore,
    images: Option<&ImageStore>) -> Vec<ValidationError>;
pub fn render(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore, images: &ImageStore)
    -> Result<Vec<u8>, InkoxideError>;
pub fn clear_caches();
```

Besides these, the crate exports only their inputs and outputs (`RenderOptions`,
`PdfALevel` and its `ParsePdfALevelError`, `PdfUaPart` and its `ParsePdfUaPartError`,
`Encryption`, `Permissions`, `Printing`, `InputFormat`, `FontStore`, `ImageStore`, `AttachmentStore`, `HyphenationStore`,
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
total fit (`optimal.rs`), which falls back to first fit past its caps;
`textWrap: balance` searches for the narrowest width at which first fit
gives as many lines. Lines are laid in `LineBoxes` (`layout/float.rs`):
the content width, and bands where floats take room on a side, one per
float or, for a float with a `shapeOutside`, one per strip of its shape
(`layout/shape.rs`), where each line finds its box at its own top. A float container's layout records the line boxes of
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
glyphs as SVG, under a work budget, takes the glyphs of the `SVG` table
alone in font units, and takes PNG bitmaps from `CBDT` and `sbix`; they
decode through `images` like document images. A colour glyph in the run's
colour becomes one image per run colour it is drawn in. Each image is a
glyph of a Type 3 font (`pdf/type3.rs`), up to 255 per font: the painter
shows the glyph in a `/Span` whose `/ActualText` is its text, and the
font's `ToUnicode` map gives the text it was first drawn with. In the text
of an SVG image, a colour glyph is its image instead. A font with colour
bitmaps and no outlines is never embedded.

Two third-party crates stay behind one module each, so replacing them
touches only that module: ttf-parser (font parsing) is used only in
`fonts`, which gives the rest of the engine metrics, glyph ids and
advances; rustybuzz (shaping) only in `text/prepare.rs`, which parses its
shaping faces from the font bytes once per render and shares them between
threads. Two parts of `fonts/layout.rs` know rustybuzz without using it,
reading with ttf-parser what it would: registration refuses the fonts
whose `GSUB` or `GPOS` coverage offsets rustybuzz would unwrap, and
`vertical_form` finds the glyphs whose `vert` form rustybuzz substitutes
in vertical text (only `vert`, the feature it applies). Replacing
rustybuzz means checking both again. `pdf` depends on `fonts` and `images`,
never the reverse, and `text` does not depend on `pdf`: the pipeline asks
`pdf` what a colour glyph's image (from `ColorGlyphs`) needs for PDF/A,
and how many bytes the text of a node's images takes. `fonts`
subsets font programs and caches the compressed whole program, and the PDF
writer supplies the compression; the writer and `images` both compress
through `flate.rs`.

Tagged output (`tag.rs`, `pdf/tag.rs`) plans the structure from the
document before layout: one element per node that is one (a paragraph, a
heading, a figure, a table and its rows and cells, a field), keyed by the
node's address, in document order, which is the reading order, and one
per link and per image with `alt` in a paragraph's runs. Painting pushes
`Op::Tag` around what a node's content painter draws (and the text
painter switches to a run's element and back), and `Op::FigureArea` where
a figure is; the writer opens a marked-content sequence with an MCID when
something is drawn under a tag, and an artifact when something is drawn
outside one (backgrounds, borders, headers and footers, which
`Op::Band` makes pagination artifacts, and nodes painted on every page,
which the plan leaves out). Annotations carry their element (`LinkArea`,
`FieldOp` and `IconOp` take it from the page encoding), or get one of
their own. The tree, the parent tree and the elements are written last;
an element's kids are in document order, but the links and figures of a
paragraph, which sit among its text as they are drawn. Untagged output is
byte for byte what it was (`tests/output_hashes.rs` pins it).

PDF/UA-2 is PDF 2.0: `Plan::new` takes whether the profile is PDF 2.0,
and the writer then puts every element in a namespace (the standard
ones of PDF 2.0) and points links at structure destinations. When
the output is compressed and not PDF/A-1b, the writer hands the structure
elements and the parent tree to `pdf/objstm.rs` instead of writing them
as top-level objects. It packs them, 128 to an object stream (ISO
32000-1, 7.5.7), and ends the file with a cross-reference stream, whose
entries for the packed objects it fills in itself (pdf-writer writes
only its own). Encryption (`pdf/out.rs`) encrypts each stream as a
whole, not the objects in it. Untagged and uncompressed output keeps its
table.

The named parts of an SVG image (`images/svg/parts.rs`) are found in the
XML before usvg converts it, which keeps the `id` of groups, shapes,
images and text but drops titles and `aria-*` attributes; an element
without an `id` is given one (`inkoxide-part-N`). In tagged output an
image with parts is drawn as a form XObject of its own for each
placement, with `/StructParents`; its parts' content is marked inside the
form, and the structure tree refers to it with marked-content references
that carry `/Stm`.

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

Text can ask a declaration for an instance besides its own: the optical
size of its font size (`opsz`, unless the declaration sets it or
`opticalSizing` is `none`), and, in SVG text, the weight and width it was
matched by, within the declaration's `weightRange` and `stretchRange`.
Before shaping, `text::font_asks` collects what the document asks (the
sizes of its runs, the spans of its SVG text) and `FontSet::add_variants`
makes those instances, in order of key and coordinates (those of deferred
fonts last, so that a missing one moves no other key), at most 64 per
declaration and 1024 in all, as fonts of their own: their keys follow the declarations',
so the shaper loads a face for each, and itemizing swaps a run's font for
its variant. An instance's `FontBBox` is the union of `head`'s box and its
glyphs' boxes at its coordinates, of every glyph, so that an encrypted
file's box does not tell which are used.

SVG text goes through the same shaper. `images/svg_text` reads each
`<text>` of an SVG once, when the SVG is decoded: the cascade of its text
properties (simplecss and svgtypes are used only in `images/svg_text`, at
the versions usvg uses: simplecss in `extract.rs`, svgtypes there and in
`path.rs`, which reads the paths a `textPath` follows), white space and character positions. Each render
lays the text out with its fonts before layout (`text/svg.rs`, one
`TextShaper::prepare` call per chunk, so fallback fonts, bidi, the glyph
limit and ToUnicode apply), replaces each `<text>` with a "probe" path
over its ink, and converts the SVG again (cached by the rewritten source).
Vertical text shapes its upright runs top to bottom (`ParaStyle::vertical`,
the font's `vmtx`, `VORG` and `vert`) and its sideways runs as horizontal
text, by the Unicode vertical orientation (`text/vertical.rs`, generated);
text on a path follows the geometry `images/svg_text/path.rs` reads from
the XML, with an arc-length table. A `use` that gives the text it names
other font properties gets a copy of that element, laid out with the
`use`'s cascade, written into the SVG under an id of its own, which the
`use` names instead (`UseCopy`).
usvg resolves the probes' paint; the PDF writer draws each probe's text in
its place as PDF text (`pdf/svg.rs`). Upright glyphs down a column share a
`TJ` in the font's `Identity-V` Type0 font, over the same descendant, whose
`DW2` is the font's most common vertical metrics: the adjustments place
each glyph, so no metrics of a glyph are written. See
[ADR 0001](adr/0001-svg-text-probes.md).

`TextCache` is the layout engine's `LeafMeasure` for text nodes: Taffy asks
for min-content / max-content / definite widths and gets the paragraph's
size. Glyph → Unicode mappings are collected while shaping and become the
ToUnicode CMaps, so text is selectable and searchable; colour glyphs carry
theirs as `ActualText` too. Form fields are drawn without shaping: the glyphs
their appearances show, and those embedded for editing, are added to the
same maps when the PDF is written (`FontPrograms::field_text`).

## Pagination and paint

`pipeline::run` lays out every page template (flow, header, footer,
overlays), paginates each flow, then paints page by page: background →
overlays with a negative `zIndex` → flow (clipped to the content band,
only nodes and lines on the page) → the other overlays → header →
footer. The flow's fixed nodes are painted in it, where its paint order
puts them (by `zIndex`, then document order), at their place on their
first page: `BoxPainter::fixed` paints each without the band clip and
the opacity, transform and clip of the nodes around it, and sets those
up again after it. A fixed node is not painted on a page where a node
around it has `display: none` or `opacity: 0`, or `pages` without that
page; it is painted where a node around it is only not on the page.
An overlay is a page child that is fixed and absolutely
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

Flow and paged coordinates are `f32`. Below 2²⁰ pt of flow (some 1,200
A4 pages in one flow) one step is under `PAGED_EPS` (0.1 pt); past that it
doubles with each power of two, so content in a flow tens of thousands of
pages long is placed only to within a few points. Each `pages` entry is a
flow of its own, so a long document split into several keeps full
precision.

## Testing

See [TESTING.md](TESTING.md): unit and fixture tests (byte-determinism,
JSON/MessagePack parity, `qpdf --check`), schema parity, visual
regression, the Node suite, benchmarks and fuzzing.
