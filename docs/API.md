# Public API

This page covers the engine's three public surfaces: the Rust crate
`inkoxide` (`crates/core`), the Node package `inkoxide`
(`packages/inkoxide`, built from `crates/node`) and the WebAssembly
package `@inkoxide/wasm` (`packages/wasm`, built from `crates/wasm`). They
expose the same engine with the same render options and error codes, with
a few differences. The Node package has no `clearCaches`, no
`deferredFonts` option and no `NOT_LOADED` error; Rust and
`@inkoxide/wasm` have all three. `@inkoxide/wasm` takes only `threads: 1`
(see [WebAssembly](#webassembly-inkoxidewasm)).

The document format itself is described in [`SCHEMA.md`](SCHEMA.md) and
[`EXAMPLES.md`](../packages/schema/EXAMPLES.md). The JSX API that builds
documents from React components is in
[`packages/render/README.md`](../packages/render/README.md) and
[`GUIDE.md`](../packages/render/GUIDE.md).

The API follows semantic versioning. Changes are
additive: new options, new error codes and new functions are minor
versions; removing or renaming any of them is a major version.

## Side by side

| Concept | Rust (`inkoxide`) | Node (`inkoxide`) |
|---|---|---|
| Register a font (TrueType, CFF OpenType, WOFF or WOFF2, static or variable; not a `.ttc` collection) | `FontStore::register(id, bytes) -> Result<(), InkoxideError>` | `registerFont(id, data): void` (process-wide) |
| Remove a font | `FontStore::unregister(id) -> bool` | `unregisterFont(id): boolean` |
| Images | `ImageStore::insert(id, bytes)`, `remove(id) -> bool`, passed to each call | `options.images: Record<string, Uint8Array>` per render |
| Attachments (the files `attachments[].src` names) | `AttachmentStore::insert(id, bytes)`, `insert_too_large(id, len)` for a file over `AttachmentStore::MAX_FILE_BYTES`, `remove(id) -> bool`, via `RenderOptions::with_attachments` | `options.attachments: Record<string, Uint8Array>` per render |
| Hyphenation patterns | `HyphenationStore::register(lang, tex)`, via `RenderOptions::with_hyphenation` | `registerHyphenationPatterns(lang, data)` (process-wide), `unregisterHyphenationPatterns(lang)` |
| Validate | `validate(doc, format, &fonts, &images) -> Vec<ValidationError>` | `validate(doc, format?, images?): ValidationError[]` |
| Validate without the images | `validate_without_images(doc, format, &fonts) -> Vec<ValidationError>` | `validate(doc, format?)`, with no `images` |
| Render | `render(doc, format, &options, &fonts, &images) -> Result<Vec<u8>, InkoxideError>` | `render(doc, options?): Promise<Uint8Array>` (libuv pool) |
| Render, blocking | (same `render`) | `renderSync(doc, options?): Uint8Array` |
| Input format | `InputFormat::Json` (default), `InputFormat::MessagePack` (cargo feature `msgpack`, on by default; without it, `PARSE_ERROR`) | `options.format` / `format`: `'json'` (default) or `'msgpack'` |
| Empty the caches renders share (decoded images, font subsets, SVG with its text) | `clear_caches()` | (not available) |
| Version | `ENGINE_VERSION`, `SCHEMA_VERSION` | `version(): { engine, schema, gitSha }` |
| Error | `InkoxideError` (`.code()`), `ValidationError { code, path, message }` | `InkoxideError extends Error { code, path, message }` |

`doc` is bytes in Rust; in Node it is a `Uint8Array` (or `Buffer`) or a
string, which is read as UTF-8 JSON.

## WebAssembly (`@inkoxide/wasm`)

`@inkoxide/wasm` is the engine built for `wasm32-unknown-unknown`, with the functions and types
of the Node package, and two more:

| Function | Meaning |
|---|---|
| `init(source?, { hyphenation? }): Promise<void>` | Compiles and instantiates the module once: from a URL (default: `inkoxide.wasm` next to the package), a `Response`, its bytes or a compiled `WebAssembly.Module`. Every other function throws until it resolves. `hyphenation` is the folder of the built-in hyphenation patterns (default: the package's `hyphenation/`) |
| `loadHyphenation(langs): Promise<void>` | Fetches the built-in hyphenation patterns of BCP-47 tags, for `renderSync` |
| `clearCaches(): void` | Rust `clear_caches()`: empties the caches renders share (decoded images, font subsets, SVG with its text). The memory stays with the instance. |

How it differs from the Node package:

- **One thread.** Every call runs on the calling thread, `render` included; `threads` may only
  be `1`, and another number is `INVALID_ARGUMENT` at `options.threads`. In a page, call it from
  a worker, as `@inkoxide/render/browser` does.
- **JSON only.** It is built without the `msgpack` feature: `format: 'msgpack'` is a
  `PARSE_ERROR`.
- **Downloads on demand.** It is built without the `embedded-hyphenation` feature: `render`
  fetches the patterns of a language from `hyphenation/<file>.bin` the first time a document
  hyphenates text in it, and the fonts of the `deferredFonts` option (id → URL) a render needs.
  `renderSync` cannot fetch: it fails with `NOT_LOADED` instead (load the patterns first with
  `loadHyphenation`).
- **A bug in the engine** (a panic in a dependency) ends the instance, since WebAssembly cannot
  unwind: the call fails with `INTERNAL`, and the next one runs in a new instance with the fonts
  and hyphenation patterns registered so far. The engine's own limits return their errors, as
  in Node.
- **Memory** grows to at most 4 GiB and is not given back; the process-wide caches keep a quarter
  of their Node budgets.
- **Output.** A render has the bytes Node renders, except that numbers computed with
  trigonometry in a dependency (the angle of an SVG marker) can differ in the last digits.
  `scripts/wasm-parity.mjs` checks every fixture.

## Render options

| Rust field / setter | Node option | Default | Meaning |
|---|---|---|---|
| `compress` / `with_compress` | `compress` | `true` | Flate-compress page content streams |
| `compression_level` / `with_compression_level` | `compressionLevel` | `6` | Flate level 0–9 (Rust: a larger level is 9; Node: other values, fractions included, are `INVALID_ARGUMENT`) |
| `creation_date` / `with_creation_date` | `creationDate` | none | RFC 3339 timestamp; overrides `meta.creationDate`. With neither, no date is written. Another string fails the render: `INVALID_VALUE` at `options.creationDate` in Rust, `INVALID_ARGUMENT` in Node |
| `threads` / `with_threads` | `threads` | all cores | Worker threads for this render. `1` runs it on the calling thread (in Node, its libuv thread) with no parallelism inside, so concurrent renders do not wait for each other; a larger number uses a pool of that size, shared by renders that ask for the same number and kept for the life of the process. A pool has at most 256 threads. Rust: `0` is `1`, and a number above 256 uses the 256-thread pool; Node: an integer from 1 to 256, other values are `INVALID_ARGUMENT` |
| `subset_fonts` / `with_subset_fonts` | `subsetFonts` | `true` | Embed only the glyphs used |
| `pdf_a` / `with_pdf_a` | `pdfA` | `false` | Write PDF/A, at `pdf_a_level` (Node: `true` is PDF/A-2b). PDF/A forbids scripts, so text fields with a `format` keep their formatted text and value but not the form scripts that format what the user types |
| `pdf_a_level` / `with_pdf_a_level` (also sets `pdf_a`) | `pdfA: '1b' \| '2b' \| '3b'` | `PdfALevel::A2b` | The PDF/A level: 1b, 2b or 3b (see [PDF/A](#pdfa)). Node: another string is `INVALID_ARGUMENT` |
| `hyphenation` / `with_hyphenation` | (`registerHyphenationPatterns`) | none | Runtime TeX patterns; and, for a build without the `embedded-hyphenation` feature, built-in patterns given with `register_builtin` |
| `deferred_fonts` / `with_deferred_fonts` | (`@inkoxide/wasm`: `deferredFonts`, id → URL) | none | Ids of declared fonts that may be missing: a render that does not draw with one, or try it for a character, renders as if it were registered; one that does fails with `NOT_LOADED` naming it |
| `encryption` / `with_encryption` (`Encryption`) | `encryption: { userPassword?, ownerPassword?, permissions? }` | none | Encrypt the PDF with passwords and permissions (see [Encryption](#encryption)). Not with PDF/A |
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

## PDF/A

With `pdfA` (Rust: `pdf_a`), the file is PDF/A, for archiving: XMP
metadata that mirrors the document information, an sRGB output intent and
a file identifier. CMYK JPEGs are converted to RGB, and `.notdef` glyphs
are left out. Every level is "b": the pages look the same in every reader.

| Level | Node | Rust | Notes |
|---|---|---|---|
| PDF/A-2b | `true` or `'2b'` | `PdfALevel::A2b` (default) | Based on PDF 1.7 |
| PDF/A-3b | `'3b'` | `PdfALevel::A3b` | PDF/A-2b, with the document's `attachments` as associated files (`/AF`) |
| PDF/A-1b | `'1b'` | `PdfALevel::A1b` | Based on PDF 1.4, for archives that require it. Font subsets list their glyphs (`CIDSet`) |

In Rust, `PdfALevel::ALL` lists the levels, `as_str` gives a level's
short name (`"1b"`, `"2b"`, `"3b"`, as Node and the CLI take it) and
`from_name` reads one back. `PdfALevel` also implements `FromStr`: a name
that is not a level is a `ParsePdfALevelError`, whose message lists the
levels.

What a level does not allow (a page side under 3 points at every level,
transparency under PDF/A-1b) is `PDFA_CONFLICT`, at the path of its first
use. The [code table of `SCHEMA.md`](SCHEMA.md#validation-error-codes)
lists what counts as transparency. A `backgroundColor` of alpha 0 draws
nothing and is accepted at 1b. Only `render` reports `PDFA_CONFLICT`:
`validate` does not know the level.

A PDF/A-1b file also keeps within the limits of PDF/A-1: real numbers
within ±32767, strings of at most 65535 bytes, names of at most 127
bytes, arrays of at most 8191 entries, dictionaries of at most 4095, and
graphics states nested at most 28 deep. A document that would pass one
fails with `PDF_ERROR`.

The a and u levels need tagged PDF, which Inkoxide does not write yet.

In the CLI, `--pdf-a` alone writes PDF/A-2b, and a level follows an
equals sign:

```sh
inkoxide render doc.json --fonts fonts/ --pdf-a=1b
```

## Encryption

With `encryption`, the file is encrypted with AES-256 (PDF 2.0, revision
6 of the standard security handler; Acrobat X, pdf.js, PDFium, Preview,
Poppler and qpdf open it). The user password opens the file; the owner
password lifts the permissions.

| Rust (`Encryption`, `Permissions`) | Node | Default |
|---|---|---|
| `user_password` | `userPassword` | none: anyone opens the file |
| `owner_password` | `ownerPassword` | the user password |
| `permissions.printing: Option<Printing>` | `permissions.printing: 'lowResolution' \| 'highResolution'` | not allowed |
| `permissions.modifying`, `copying`, `annotating`, `filling_forms`, `document_assembly` | `modifying`, `copying`, `annotating`, `fillingForms`, `documentAssembly` | `false` |
| `permissions.content_accessibility` | `contentAccessibility` | `true` |

- At least one password is needed. Passwords are printable ASCII, at most
  127 bytes.
- `permissions` need an owner password other than the user password:
  without one, or with the same one, the user password lifts them. Readers,
  not the file, enforce the permissions.
- With `pdfA`, encryption is `INVALID_VALUE` at `options.encryption` in
  Rust and `INVALID_ARGUMENT` in Node (PDF/A forbids it).
- A bad value is `INVALID_VALUE` at its path (`options.encryption.userPassword`,
  …) in Rust and `INVALID_ARGUMENT` in Node. No error and no `Debug` output
  shows a password: Rust's `Encryption` prints them as `<redacted>`.
- Every string and stream is encrypted. The object structure is not: page
  count, page sizes, font names and widths, link rectangles, stream
  lengths and the other dictionary entries stay readable. Font subset tags
  (`ABCDEF+` in a font name) are keyed by the file's keys, and a font's
  widths are listed for all its glyphs, not only those used, so neither
  tells which glyphs the file uses. Listing them all adds up to about 5
  bytes a glyph to each font (some 15 KB for a Latin font of 3,000
  glyphs). Radio buttons, whose states are
  otherwise named after their values, are named by their position in the
  group instead, with the values in `/Opt`, so the values are encrypted
  too.
- Form field state that PDF writes as names and numbers stays readable:
  whether a checkbox is checked (`/V` and `/AS`), which radio button of a
  group is on (its position, as above), and the positions of a list box's
  selected options (`/I`) and of its first visible one (`/TI`). Text field
  values, select and list values and every option are strings, and are
  encrypted.

```rust
use inkoxide::{Encryption, Permissions, Printing, RenderOptions};

let options = RenderOptions::default().with_encryption(
    Encryption::new()
        .with_user_password("open")
        .with_owner_password("owner")
        .with_permissions(Permissions::new().with_printing(Printing::HighResolution)),
);
```

## Hyphenation patterns

Which text is hyphenated, and in which language, is in
[`SCHEMA.md`](SCHEMA.md#style) (`hyphenate` and `lang`; a run's own
`hyphenate` is under [`TextRun`](SCHEMA.md#textrun)). The warning for a
node with no language is a `tracing` warning that names the node's `id`.
The patterns of a language are chosen in this order:

1. patterns registered for the tag, then for each less specific tag
   (`en-GB`, then `en`);
2. the built-in patterns for the language (48 languages, English
   included: hypher's, in `crates/core/hyphenation`; `builtin_hyphenation_file`
   names the file of a tag);
3. none.

Soft hyphens (U+00AD) in the text are always honoured.

When a word wider than its line is broken between characters (as CSS
does with `overflow-wrap: anywhere`), a grapheme cluster is never split.
Text with `whiteSpace` `nowrap` or `pre` is not broken: a long word
overflows the line.

Patterns are registered for a BCP-47 tag with a two- or three-letter
language (`en`, `en-GB`, `haw`), in any case and with `-` or `_`.
Patterns for `en-GB` and for `en-US` coexist; patterns for `en` serve
every English tag without patterns of its own.

Patterns are TeX files in the hyph-utf8 format: `\patterns{…}`, with
an optional `\hyphenation{…}` exception list, in UTF-8. English is
built in; to hyphenate it with another pattern set (or to add a
language that is not built in), register a file:

1. Download a pattern file from hyph-utf8, for example `hyph-en-us.tex` from
   `https://github.com/hyphenation/tex-hyphen/blob/master/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-en-us.tex`.
2. Check that its licence suits you. The `inkoxide` package ships no
   pattern files. `@inkoxide/render` ships `hyph-en-us.tex`.
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

## Font features

A run's `fontFeatures` and their bounds are in
[`SCHEMA.md`](SCHEMA.md#textrun). A feature does something only when the
run's font has it. A tag that is not four printable ASCII characters, or
is set twice in a run, is `INVALID_VALUE`; a value or a count past its
bound is `OUT_OF_RANGE`. In `@inkoxide/render`, `fontFeatureSettings` and
`fontVariantNumeric` fill a run's `fontFeatures` (see
[`GUIDE.md`](../packages/render/GUIDE.md)).

## Variable fonts

A variable font (TrueType with `gvar`, or CFF2) is registered once, with
the same call as any font. Each [font declaration](SCHEMA.md#fontdecl)
that draws with it names it in `source` and gives its axis values in
`variations`; there is no other API. Layout, metrics (`MVAR`) and shaping
read the font at that instance, and the PDF embeds the instance as static
TrueType, a subset or (with `subset_fonts: false`) every glyph, never the
variable font. The output stays deterministic: an instance's bytes depend
only on the font and the clamped axis values.

- A value is clamped to its axis, and a value at the axis's default is
  dropped: `{"wght": 400}` on a font whose default is 400 is the default
  instance, as no `variations` is.
- An axis the font lacks is ignored, with a `tracing` warning at render
  that names the font; `validate` does not read the font's axes.
- A declaration takes at most 16 axes, each once, with a value within
  ±32767 (`OUT_OF_RANGE`, or `INVALID_VALUE` for a tag that is not four
  printable ASCII characters or that is given twice), and a document at
  most 64 declarations with `variations`.
- `register` refuses (`FONT_ERROR`) a variable font with more than 63
  axes or an `avar` table other than version 1.0, which the two font
  parsers would read differently. An instance that cannot be made fails
  the render with `FONT_ERROR`.

In `@inkoxide/render`, `Font.register` takes a weight range
(`fontWeight: [100, 900]`) and `fontVariationSettings`, and the builder
declares one instance per weight and settings a document uses (see
[`GUIDE.md`](../packages/render/GUIDE.md)).

## Determinism

For a fixed `creationDate` (or `meta.creationDate`), output bytes depend
only on the document, the font, image and hyphenation pattern bytes, and
the options. They
do not depend on the thread count or on JSON versus MessagePack input.
Without any creation date, no date is written, so output is still
deterministic. `textWrap: "pretty"` is bounded per paragraph (see
[`SCHEMA.md`](SCHEMA.md#style)), so its cost and its output do not depend
on the rest of the document either.

Encrypted output is deterministic too: the keys, salts and IVs that
revision 6 makes random are derived from the passwords and the whole
file as it would be written unencrypted, so identical input gives
identical bytes, and documents whose files differ anywhere (text,
images, links, form fields, notes, bookmarks, metadata) get different
keys. The same input with the same passwords gives the same bytes, so
equal files can be told apart from different ones. The derivation is not
the standard's random one; it has been reviewed internally but has not had
an independent cryptography review yet. What encryption does and does not
protect is in [SECURITY.md](../SECURITY.md#encryption).

## Errors

Every failure has a stable string code. In Rust it is
`ValidationCode::as_str()` or `InkoxideError::code()`. In Node it is
`InkoxideError.code`, typed as the `ErrorCode` union in `index.d.ts`.
Handle unknown codes, because new ones may be added in minor versions.

| Code | Rust | Node | When |
|---|---|---|---|
| `PARSE_ERROR` … `UNKNOWN_ANCHOR` | `ValidationCode` (in `ValidationError` or `InkoxideError::Validation`) | `ValidationError.code`, or `InkoxideError.code` from `render` | The document is invalid. See the code table in [`SCHEMA.md`](SCHEMA.md). In Rust, also an invalid `creation_date` option (`INVALID_VALUE` at `options.creationDate`) |
| `PDFA_CONFLICT` | `ValidationCode::PdfaConflict` | `InkoxideError.code` from `render` | The document uses something its PDF/A level does not allow: a page side under 3 points, at any level, or transparency under PDF/A-1b. The path names the first such use. Only `render` reports it: `validate` does not know the level |
| `FONT_ERROR` | `InkoxideError::Font` | same | Font bytes cannot be parsed or embedded, or the shaper panicked on the font, or the font shaped the text to far more glyphs than it has bytes (the message names the font) |
| `IMAGE_ERROR` | `InkoxideError::Image` | same | An image cannot be embedded. Images that do not decode are `INVALID_IMAGE` at their path, found by validation first |
| `LAYOUT_ERROR` | `InkoxideError::Layout` | same | Layout failed, for example the page limit (100,000 pages) or the limit on content repeated on every page was exceeded |
| `PDF_ERROR` | `InkoxideError::Pdf` | same | Writing the PDF failed, or a PDF/A-1b file would pass one of the limits of PDF/A-1 |
| `HYPHENATION_ERROR` | `InkoxideError::Hyphenation` | same | Hyphenation patterns cannot be parsed |
| `NOT_LOADED` | `InkoxideError::NotLoaded` (`NotLoaded { fonts, hyphenation }`) | `@inkoxide/wasm` only: from `renderSync`, and from `render` when it cannot fetch what the document needs | The render needs a font of `deferred_fonts`, or built-in hyphenation patterns that a build without the `embedded-hyphenation` feature has not been given: register them and render again (`@inkoxide/wasm`'s `render` fetches them and does). `@inkoxide/render/browser` reports a file it could not fetch as `INTERNAL` |
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
limit or the limit on content repeated on every page (`LAYOUT_ERROR`), on
a font that cannot be shaped or embedded (`FONT_ERROR`), on
something its PDF/A level does not allow (`PDFA_CONFLICT`, which `validate` cannot know),
or on a failure in writing the PDF (`PDF_ERROR`). `validate` reads the
document's structure first, and a problem found there (input that does
not parse, an unknown or missing property, a wrong type, a value the
reader rejects) is returned alone, as the rest cannot be checked.

Some rules are checked by the engine but not by the JSON Schema (an id
used twice, references that must resolve, the rules of `break`, floats,
form fields and grid areas, and others), so a document that passes the
schema can still fail validation. The schema's descriptions state them,
and [`SCHEMA.md`](SCHEMA.md) shows them with the properties they concern
and in its [code table](SCHEMA.md#validation-error-codes). One more limit
is not in the schema: the tables of a document have at most 1000000 grid
slots in all, a slot being one row of one column, however the cells span
them (`OUT_OF_RANGE` at the table's `rows`). How numbers are read against
their bounds (as 32-bit floats) is in [`SCHEMA.md`](SCHEMA.md#conventions).

## Not part of the API

- `InkoxideError.__testPanic` in the Node binding is a non-enumerable test hook, not a
  module export.
- `inkoxide::JSON_ONLY` in the Rust crate is public but `#[doc(hidden)]`: the message the
  bindings share for MessagePack input to a JSON-only build. Do not depend on it.
- Nothing else in the Rust crate is public: its modules are private.
