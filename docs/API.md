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
| `pdf_a` / `with_pdf_a` | `pdfA` | `false` | Write PDF/A, at `pdf_a_level` (Node: `true` is PDF/A-2b). PDF/A forbids scripts, so text fields with a `format` keep their formatted text and value but not the form scripts that format what the user types |
| `pdf_a_level` / `with_pdf_a_level` (also sets `pdf_a`) | `pdfA: '1b' \| '2b' \| '3b'` | `PdfALevel::A2b` | The PDF/A level: 1b, 2b or 3b (see [PDF/A](#pdfa)). Node: another string is `INVALID_ARGUMENT` |
| `hyphenation` / `with_hyphenation` | (`registerHyphenationPatterns`) | none | Runtime TeX patterns |
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
| PDF/A-3b | `'3b'` | `PdfALevel::A3b` | PDF/A-2b that may carry embedded files |
| PDF/A-1b | `'1b'` | `PdfALevel::A1b` | Based on PDF 1.4, for archives that require it. Font subsets list their glyphs (`CIDSet`) |

PDF/A-1b has no transparency. A render at 1b fails with `PDFA_CONFLICT`
at the path of the first thing that needs it: a colour with alpha below
1 (a `backgroundColor` of alpha 0 draws nothing and is accepted), an
`opacity` between 0 and 1, `debug`, an image, inline image, emoji image
or background image with an alpha channel, an SVG drawn with opacity, a
mask, a blend mode, an isolated group or a clip path other than one path,
or a colour glyph that needs transparency (a bitmap with an alpha channel
or a layer with an opacity, at the path of its run). `validate` does not
know the level, so it does not report these. A PDF/A-1b file also keeps
within the limits of PDF/A-1: real numbers within ±32767, strings of at
most 65535 bytes, arrays of at most 8191 entries, dictionaries of at
most 4095, and graphics states nested at most 28 deep. A document that
would pass one fails with `PDF_ERROR`.

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

## Font features

A run's `fontFeatures` set OpenType features by tag, as CSS
`font-feature-settings`: 1 turns a feature on, 0 off, and a larger value
picks an alternate. A feature does something only when the run's font
has it. The features are bounded:

- a tag is four printable ASCII characters, set once per run
  (`INVALID_VALUE`);
- a value is at most 65535 (`OUT_OF_RANGE`);
- a run takes at most 32 features (`OUT_OF_RANGE`);
- a document uses at most 64 different sets of features, as each set is
  shaped with plans and words of its own (`OUT_OF_RANGE`).

`@inkoxide/render` reads `fontFeatureSettings` and `fontVariantNumeric`,
and their Tailwind classes, into a run's `fontFeatures`, within the same
bounds.

## Determinism

For a fixed `creationDate` (or `meta.creationDate`), output bytes depend
only on the document, the font, image and hyphenation pattern bytes, and
the options. They
do not depend on the thread count or on JSON versus MessagePack input.
Without any creation date, no date is written, so output is still
deterministic.

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
| `PDFA_CONFLICT` | `ValidationCode::PdfaConflict` | `InkoxideError.code` from `render` | The document uses something its PDF/A level does not allow: transparency under PDF/A-1b. The path names the first such use. Only `render` reports it: `validate` does not know the level |
| `FONT_ERROR` | `InkoxideError::Font` | same | Font bytes cannot be parsed or embedded, or the shaper panicked on the font, or the font shaped the text to far more glyphs than it has bytes (the message names the font) |
| `IMAGE_ERROR` | `InkoxideError::Image` | same | An image cannot be embedded. Images that do not decode are `INVALID_IMAGE` at their path, found by validation first |
| `LAYOUT_ERROR` | `InkoxideError::Layout` | same | Layout failed, for example the page limit or the limit on content repeated on every page was exceeded |
| `PDF_ERROR` | `InkoxideError::Pdf` | same | Writing the PDF failed, or a PDF/A-1b file would pass one of the limits of PDF/A-1 |
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
  used twice (the buttons of one radio group share theirs) or used both
  for a field and for a group of fields (`a` and `a.b`).
- Fonts, images and `#id` links must resolve: `UNKNOWN_FONT`,
  `FONT_NOT_REGISTERED`, `UNKNOWN_IMAGE`, `INVALID_IMAGE`,
  `IMAGE_TOO_LARGE` and `UNKNOWN_ANCHOR`. A `#id` link must name a node
  in the page body (the flow or an overlay) that is displayed: an id in
  a header or footer, or under `display: none`, is not a destination.
- A radio button needs a non-empty `value` (`MISSING_FIELD`), and a text
  field's `value` is at most `maxLength` characters (`OUT_OF_RANGE`).
  The `value` of a select or list field is one of its `options`; an
  array `value` needs a list with `multiSelect` and names each option at
  most once (`INVALID_VALUE`).
- A `format` applies to single-line text fields that are not passwords,
  with the keys of its `type`; a date or time `format` needs a `pattern`
  (`MISSING_FIELD`) made of the documented letters and ` .,/:-`, and the
  field's `value` must fit the format (`INVALID_VALUE`).
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
- The tables of a document have at most 1000000 grid slots in all, a
  slot being one row of one column, however the cells span them
  (`OUT_OF_RANGE` at the table's `rows`).

## Not part of the API

- `InkoxideError.__testPanic` in the Node binding is a non-enumerable test hook, not a
  module export.
- Nothing else in the Rust crate is public: its modules are private.
