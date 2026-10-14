# Public API

Inkoxide has two public surfaces: the Rust crate `inkoxide`
(`crates/core`) and the Node package `inkoxide` (`packages/inkoxide`,
built from `crates/node`). They expose the same engine with the same options and
error codes. The document format itself is described in
[`SCHEMA.md`](SCHEMA.md) and [`EXAMPLES.md`](../packages/schema/EXAMPLES.md).

The API follows semantic versioning. Changes are
additive: new options, new error codes and new functions are minor
versions; removing or renaming any of them is a major version.

## Side by side

| Concept | Rust (`inkoxide`) | Node (`inkoxide`) |
|---|---|---|
| Register a font (TrueType, CFF OpenType, WOFF or WOFF2; not a `.ttc` collection) | `FontStore::register(id, bytes) -> Result<(), InkoxideError>` | `registerFont(id, data): void` (process-wide) |
| Remove a font | `FontStore::unregister(id) -> bool` | `unregisterFont(id): boolean` |
| Images | `ImageStore::insert(id, bytes)`, passed to each call | `options.images: Record<string, Uint8Array>` per render |
| Hyphenation patterns | `HyphenationStore::register(lang, tex)`, via `RenderOptions::with_hyphenation` | `registerHyphenationPatterns(lang, data)` (process-wide), `unregisterHyphenationPatterns(lang)` |
| Validate | `validate(doc, format, &fonts, &images) -> Vec<ValidationError>` | `validate(doc, format?, images?): ValidationError[]` |
| Validate without the images | `validate_without_images(doc, format, &fonts) -> Vec<ValidationError>` | `validate(doc, format?)`, with no `images` |
| Render | `render(doc, format, &options, &fonts, &images) -> Result<Vec<u8>, InkoxideError>` | `render(doc, options?): Promise<Uint8Array>` (libuv pool) |
| Render, blocking | (same `render`) | `renderSync(doc, options?): Uint8Array` |
| Input format | `InputFormat::Json` (default), `InputFormat::MessagePack` | `options.format` / `format`: `'json'` (default) or `'msgpack'` |
| Version | `ENGINE_VERSION`, `SCHEMA_VERSION` | `version(): { engine, schema, gitSha }` |
| Error | `InkoxideError` (`.code()`), `ValidationError { code, path, message }` | `InkoxideError extends Error { code, path, message }` |

`doc` is bytes in Rust; in Node it is a `Uint8Array` (or `Buffer`) or a
string, which is read as UTF-8 JSON.

## Render options

| Rust field / setter | Node option | Default | Meaning |
|---|---|---|---|
| `compress` / `with_compress` | `compress` | `true` | Flate-compress page content streams |
| `compression_level` / `with_compression_level` | `compressionLevel` | `6` | Flate level 0–9 (Rust: a larger level is 9; Node: other values, fractions included, are `INVALID_ARGUMENT`) |
| `creation_date` / `with_creation_date` | `creationDate` | none | RFC 3339 timestamp; overrides `meta.creationDate`. With neither, no date is written. Another string fails the render: `INVALID_VALUE` at `options.creationDate` in Rust, `INVALID_ARGUMENT` in Node |
| `threads` / `with_threads` | `threads` | all cores | Worker threads for this render. `1` runs it on the calling thread (in Node, its libuv thread) with no parallelism inside, so concurrent renders do not wait for each other; a larger number uses a pool of that size, shared by renders that ask for the same number and kept for the life of the process. A pool has at most 256 threads. Rust: `0` is `1`, and a number above 256 uses the 256-thread pool; Node: an integer from 1 to 256, other values are `INVALID_ARGUMENT` |
| `subset_fonts` / `with_subset_fonts` | `subsetFonts` | `true` | Embed only the glyphs used |
| `pdf_a` / `with_pdf_a` | `pdfA` | `false` | Write PDF/A-2b |
| `hyphenation` / `with_hyphenation` | (`registerHyphenationPatterns`) | none | Runtime TeX patterns |
| (argument `format`) | `format` | `'json'` | Input encoding |
| (argument `&ImageStore`) | `images` | `{}` | Image bytes by id: JPEG, PNG, GIF (first frame), WebP or SVG |

In Rust, `RenderOptions` is `#[non_exhaustive]`. Build it with the
setters; the fields stay public for reading and assignment:

```rust
use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

let mut fonts = FontStore::new();
fonts.register("Inter-Regular", std::fs::read("Inter-Regular.ttf")?)?;
let options = RenderOptions::default()
    .with_threads(1)
    .with_creation_date("2026-01-01T00:00:00Z");
let pdf = inkoxide::render(&doc, InputFormat::Json, &options, &fonts, &ImageStore::new())?;
```

```js
import { readFileSync } from 'node:fs';
import { registerFont, render } from 'inkoxide';

registerFont('Inter-Regular', readFileSync('Inter-Regular.ttf'));
const pdf = await render(JSON.stringify(doc), { threads: 1, creationDate: '2026-01-01T00:00:00Z' });
```

## Hyphenation patterns

A text node with `hyphenate: true` is hyphenated in its `lang` (or
`meta.lang`). A run's own `hyphenate` overrides the node's for that run's
words, in the node's language. The patterns are chosen in this order:

1. patterns registered for the tag, then for each less specific tag
   (`en-GB`, then `en`);
2. hypher's built-in patterns for the language (about 50 languages,
   English included);
3. none.

Soft hyphens (U+00AD) in the text are always honoured.

Patterns are registered for a BCP-47 tag with a two- or three-letter
language (`en`, `en-GB`, `haw`), in any case and with `-` or `_`.
Patterns for `en-GB` and for `en-US` coexist; patterns for `en` serve
every English tag without patterns of its own.

Patterns are TeX files in the hyph-utf8 format: `\patterns{…}`, with
an optional `\hyphenation{…}` exception list, in UTF-8. English is
built in; to hyphenate it with another pattern set (or to add a
language hypher lacks), register a file:

1. Download a pattern file from hyph-utf8, for example `hyph-en-us.tex` from
   `https://github.com/hyphenation/tex-hyphen/blob/master/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-en-us.tex`.
2. Check that its licence suits you. Pattern files are not shipped with
   Inkoxide.
3. Register it:

```js
registerHyphenationPatterns('en', fs.readFileSync('hyph-en-us.tex')); // once, at startup; 'en' covers every English lang
```

```sh
inkoxide render doc.json --fonts fonts/ --hyphenation en=hyph-en-us.tex
```

```rust
let mut patterns = inkoxide::HyphenationStore::new();
patterns.register("en-US", &std::fs::read("hyph-en-us.tex")?)?;
let options = RenderOptions::default().with_hyphenation(patterns);
```

## Line breaking

A text node breaks its lines first fit by default (`textWrap: "wrap"`).
With `textWrap: "pretty"`, each stretch of a paragraph between forced
breaks is set as a whole (Knuth and Plass's total fit), for even lines.
It is bounded per paragraph, so its cost and its output do not depend on
the rest of the document or on threads; past a bound, the stretch is set
first fit:

- at most 20 000 break opportunities in a stretch;
- at most 512 open breakpoints at a time;
- at most 2 097 152 lines tried in a paragraph.

## Determinism

For a fixed `creationDate` (or `meta.creationDate`), output bytes depend
only on the document, the font, image and hyphenation pattern bytes, and
the options. They
do not depend on the thread count or on JSON versus MessagePack input.
Without any creation date, no date is written, so output is still
deterministic.

## Errors

Every failure has a stable string code. In Rust it is
`ValidationCode::as_str()` or `InkoxideError::code()`. In Node it is
`InkoxideError.code`, typed as the `ErrorCode` union in `index.d.ts`.
Handle unknown codes, because new ones may be added in minor versions.

| Code | Rust | Node | When |
|---|---|---|---|
| `PARSE_ERROR` … `UNKNOWN_ANCHOR` | `ValidationCode` (in `ValidationError` or `InkoxideError::Validation`) | `ValidationError.code`, or `InkoxideError.code` from `render` | The document is invalid. See the code table in [`SCHEMA.md`](SCHEMA.md). In Rust, also an invalid `creation_date` option (`INVALID_VALUE` at `options.creationDate`) |
| `FONT_ERROR` | `InkoxideError::Font` | same | Font bytes cannot be parsed or embedded, or the shaper panicked on the font, or the font shaped the text to far more glyphs than it has bytes (the message names the font) |
| `IMAGE_ERROR` | `InkoxideError::Image` | same | An image cannot be embedded. Images that do not decode are `INVALID_IMAGE` at their path, found by validation first |
| `LAYOUT_ERROR` | `InkoxideError::Layout` | same | Layout failed, for example the page limit or the limit on content repeated on every page was exceeded |
| `PDF_ERROR` | `InkoxideError::Pdf` | same | Writing the PDF failed |
| `HYPHENATION_ERROR` | `InkoxideError::Hyphenation` | same | Hyphenation patterns cannot be parsed |
| `INTERNAL` | `InkoxideError::Internal`, also for a panic in `render` | same, plus caught panics in any call | A bug in the engine; the process keeps running |
| `INVALID_ARGUMENT` | n/a (types are checked at compile time) | Node only | A JS argument has the wrong type or is out of range |

The full list of validation codes is `ValidationCode` in
`crates/core/src/error.rs` and the `ValidationCode` union in
`packages/inkoxide/index.d.ts`.

The fields of a Node error are:

- `code`: as above.
- `path`: the document location, like
  `pages[0].children[3].runs[1].font`. It is empty when the error is
  not tied to a location.
- `message`: the engine's description, without the code or the path.
- `name`: always `'InkoxideError'`.

`render` reports only the first validation problem; call `validate` to
get the others. `validate` returns at most 100 problems. It decodes the
images the document uses, so an image that would fail the render is
`INVALID_IMAGE` at its path (the decode is cached for the render). A JPEG
is embedded without decoding: its structure is checked (it must be
complete, and baseline or progressive), but corrupt compressed data
inside a complete JPEG is not found. Without images (Node `validate`
with no `images`, Rust `validate_without_images`), image ids are not
looked up and everything else is checked. An empty result means the
document is valid; `render` can still fail on an option, on the page
limit or the limit on content repeated on every page (`LAYOUT_ERROR`), or on a font that cannot be shaped or embedded
(`FONT_ERROR`). `validate` reads the document's structure first, and a problem found there (input that does
not parse, an unknown or missing property, a wrong type, a value the
reader rejects) is returned alone, as the rest cannot be checked.

Some rules are checked by the engine but not by the JSON Schema, so a
document that passes the schema can still fail validation. The schema's
descriptions and [`SCHEMA.md`](SCHEMA.md) state them. Among them:

- `break` with a value other than its defaults, on a header or footer,
  on a node in a table cell, or on any node inside these, is
  `INVALID_VALUE`; a `pageBreak` there is `INVALID_NODE`.
- Page margins that leave no room for content (`left + right` at least
  the page width, or `top + bottom` at least its height) are
  `OUT_OF_RANGE` at `margin`.
- `DUPLICATE_ID` is a font id declared twice, or a form field `name`
  used twice (the buttons of one radio group share theirs).
- Fonts, images and `#id` links must resolve: `UNKNOWN_FONT`,
  `FONT_NOT_REGISTERED`, `UNKNOWN_IMAGE`, `INVALID_IMAGE`,
  `IMAGE_TOO_LARGE` and `UNKNOWN_ANCHOR`. A `#id` link must name a node
  in the page body (the flow or an overlay) that is displayed: an id in
  a header or footer, or under `display: none`, is not a destination.
- A radio button needs a non-empty `value` (`MISSING_FIELD`), and a text
  field's `value` is at most `maxLength` characters (`OUT_OF_RANGE`).
- `gridArea` cannot be combined with `gridRow` or `gridColumn`
  (`INVALID_VALUE`), and `gridTemplateAreas` has at most 10000 cells
  (`OUT_OF_RANGE`).
- `float` applies to the children of a column box, not with `position:
  absolute` nor with a forced break (`INVALID_VALUE`), and not to table
  cells (`STYLE_NOT_APPLICABLE`); a box holds at most 1000 floated
  children (`OUT_OF_RANGE`). A box with floats takes no `justifyContent`
  or `alignContent`, and its children no `flexGrow` or `flexBasis`
  (`STYLE_NOT_APPLICABLE`).
- `transform`, `transformOrigin` and `zIndex` do not apply to table
  cells (`STYLE_NOT_APPLICABLE`).

## Not part of the API

- `InkoxideError.__testPanic` in the Node binding is a non-enumerable test hook, not a
  module export.
- Nothing else in the Rust crate is public: its modules are private.
