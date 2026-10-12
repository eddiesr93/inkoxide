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
| Register a font (TrueType, CFF OpenType, WOFF or WOFF2) | `FontStore::register(id, bytes) -> Result<(), InkoxideError>` | `registerFont(id, data): void` (process-wide) |
| Remove a font | `FontStore::unregister(id) -> bool` | `unregisterFont(id): boolean` |
| Images | `ImageStore::insert(id, bytes)`, passed to each call | `options.images: Record<string, Uint8Array>` per render |
| Hyphenation patterns | `HyphenationStore::register(lang, tex)`, via `RenderOptions::with_hyphenation` | `registerHyphenationPatterns(lang, data)` (process-wide), `unregisterHyphenationPatterns(lang)` |
| Validate | `validate(doc, format, &fonts, &images) -> Vec<ValidationError>` | `validate(doc, format?, images?): ValidationError[]` |
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
| `compression_level` / `with_compression_level` | `compressionLevel` | `6` | Flate level 0–9 (Node: other values are `INVALID_ARGUMENT`) |
| `creation_date` / `with_creation_date` | `creationDate` | none | RFC 3339 timestamp; overrides `meta.creationDate`. With neither, no date is written |
| `threads` / `with_threads` | `threads` | all cores | Worker threads for this render. `1` runs it on the calling thread (in Node, its libuv thread) with no parallelism inside, so concurrent renders do not wait for each other; a larger number uses a pool of that size, shared by renders that ask for the same number |
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

1. patterns registered for the language;
2. hypher's built-in patterns (about 50 languages, English included);
3. none.

Soft hyphens (U+00AD) in the text are always honoured.

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
registerHyphenationPatterns('en-US', fs.readFileSync('hyph-en-us.tex')); // once, at startup
```

```sh
inkoxide render doc.json --fonts fonts/ --hyphenation en-US=hyph-en-us.tex
```

```rust
let mut patterns = inkoxide::HyphenationStore::new();
patterns.register("en-US", &std::fs::read("hyph-en-us.tex")?)?;
let options = RenderOptions::default().with_hyphenation(patterns);
```

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
| `PARSE_ERROR` … `UNKNOWN_ANCHOR` | `ValidationCode` (in `ValidationError` or `InkoxideError::Validation`) | `ValidationError.code`, or `InkoxideError.code` from `render` | The document is invalid. See the code table in [`SCHEMA.md`](SCHEMA.md) |
| `FONT_ERROR` | `InkoxideError::Font` | same | Font bytes cannot be parsed or embedded |
| `IMAGE_ERROR` | `InkoxideError::Image` | same | An image cannot be decoded or embedded |
| `LAYOUT_ERROR` | `InkoxideError::Layout` | same | Layout failed, for example the page limit was exceeded |
| `PDF_ERROR` | `InkoxideError::Pdf` | same | Writing the PDF failed |
| `HYPHENATION_ERROR` | `InkoxideError::Hyphenation` | same | Hyphenation patterns cannot be parsed |
| `INTERNAL` | `InkoxideError::Internal` | same, plus caught panics | A bug in the engine; the process keeps running |
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
get the full list.

## Not part of the API

- `InkoxideError.__testPanic` in the Node binding is a non-enumerable test hook, not a
  module export.
- Nothing else in the Rust crate is public: its modules are private.
