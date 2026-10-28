# Public API

This page covers the engine's three public surfaces: the Rust crate
`inkoxide` (`crates/core`), the Node package `inkoxide`
(`packages/inkoxide`, built from `crates/node`) and the WebAssembly
package `@inkoxide/wasm` (`packages/wasm`, built from `crates/wasm`). They
expose the same engine with the same render options and error codes, with
a few differences. The Node package has no `clearCaches`, no
`deferredFonts` option and no `NOT_LOADED` error; Rust and
`@inkoxide/wasm` have all three. `@inkoxide/wasm` takes `threads` above 1 only with a pool of
threads, on a page that is cross-origin isolated (see [WebAssembly](#webassembly-inkoxidewasm)).

The document format itself is described in [`SCHEMA.md`](SCHEMA.md) and
[`EXAMPLES.md`](../packages/schema/EXAMPLES.md). The JSX API that builds
documents from React components is in
[`packages/react/README.md`](../packages/react/README.md) and
[`GUIDE.md`](../packages/react/GUIDE.md).

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
| Validate for a render with its options | `validate_with_options(doc, format, &options, &fonts, Option<&images>) -> Vec<ValidationError>` | `validate(doc, format?, images?, options?)`, with the render's options |
| Render | `render(doc, format, &options, &fonts, &images) -> Result<Vec<u8>, InkoxideError>` | `render(doc, options?): Promise<Uint8Array>` (libuv pool) |
| Render, blocking | (same `render`) | `renderSync(doc, options?): Uint8Array` |
| Input format | `InputFormat::Json` (default), `InputFormat::MessagePack` (cargo feature `msgpack`, on by default; without it, `PARSE_ERROR`) | `options.format` / `format`: `'json'` (default) or `'msgpack'` |
| Empty the caches renders share (decoded images, font subsets, SVG with its text, CMYK JPEGs converted for PDF/A) | `clear_caches()` | (not available) |
| Version | `ENGINE_VERSION`, `SCHEMA_VERSION` | `version(): { engine, schema, gitSha }` |
| Error | `InkoxideError` (`.code()`), `ValidationError { code, path, message }` | `InkoxideError extends Error { code, path, message }` |

`doc` is bytes in Rust; in Node it is a `Uint8Array` (or `Buffer`) or a
string, which is read as UTF-8 JSON.

## WebAssembly (`@inkoxide/wasm`)

`@inkoxide/wasm` is the engine built for `wasm32-unknown-unknown`, with the functions and types
of the Node package, and four more:

| Function | Meaning |
|---|---|
| `init(source?, { hyphenation?, threads?, trapChannel? }): Promise<void>` | Compiles and instantiates the module once: from a URL (default: `inkoxide.wasm` next to the package), a `Response`, its bytes or a compiled `WebAssembly.Module`, of either module. Every other function throws `NOT_LOADED` until it resolves, and a module that cannot be fetched or compiled rejects it with `NOT_LOADED`. `hyphenation` is the folder of the built-in hyphenation patterns (default: the package's `hyphenation/`). `threads` above 1 (at most 256) loads `inkoxide-threads.wasm` and starts a pool of that many workers, where memory can be shared; `trapChannel` names the `BroadcastChannel` a pool thread reports a bug on |
| `threadCount(): number` | The threads renders use: the pool's size, or 1 |
| `loadHyphenation(langs): Promise<void>` | Fetches the built-in hyphenation patterns of BCP-47 tags, for `renderSync` |
| `clearCaches(): void` | Rust `clear_caches()`: empties the caches renders share (decoded images, font subsets, SVG with its text, CMYK JPEGs converted for PDF/A). The memory stays with the instance. |

How it differs from the Node package:

- **One thread, or a pool.** Every call runs on the calling thread, `render` included: in a page,
  call it from a worker, as `@inkoxide/render/browser` does. Without a pool `threads` may only be
  `1`, and another number is `INVALID_ARGUMENT` at `options.threads`. With `init(source, {
  threads })`, in a worker of a page that is cross-origin isolated (COOP `same-origin`, COEP
  `require-corp`), or in Node, the module with threads renders on a pool of Web Workers (worker
  threads in Node) that share its memory; a render's `threads: 1` keeps to the calling thread,
  and any other number up to 256 uses the pool. The bytes are those of one thread. Where the
  pool's workers cannot start, it renders on one thread. From a script of another origin (a CDN),
  a thread starts as a worker of the page's origin from a `blob:` URL that imports `thread.js`: it
  needs CORS on the CDN's files and `worker-src blob:` in a Content Security Policy. A bug on a pool thread leaves the call
  waiting: the thread posts `{ message }` on the `trapChannel` (a name of its own: a channel
  reaches every tab of the origin), and whoever listens ends the worker that waits (`timeoutMs` does too, in
  `@inkoxide/render/browser`). See [ADR 0003](adr/0003-browser-threads.md).
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
- **Memory** grows to at most 4 GiB (2 GiB in the module with threads, or less on a device that
  cannot reserve that much) and is not given back; the process-wide caches keep a quarter of their
  Node budgets.
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
| `pdf_a_level` / `with_pdf_a_level` (also sets `pdf_a`) | `pdfA: '1b' \| '2b' \| '2a' \| '3b' \| '3a'` | `PdfALevel::A2b` | The PDF/A level: 1b, 2b, 2a, 3b or 3a (see [PDF/A](#pdfa)). Node: another string is `INVALID_ARGUMENT` |
| `tagged` / `with_tagged` | `tagged` | `false` | Write tagged PDF: a structure tree in document order, with roles (`role`), alternative text (`alt`), table headers and field labels, and the rest marked as artifacts. PDF/UA and the "a" levels imply it |
| `flatten_alpha` / `with_flatten_alpha` | `flattenAlpha` | `false` | At PDF/A-1b, draw images with partial alpha (neither only clear nor only opaque) on white instead of failing with `PDFA_CONFLICT`. Other levels and plain PDF keep the alpha. Binary alpha needs no option: PDF/A-1b writes it as a stencil mask |
| `pdf_ua` / `with_pdf_ua` | `pdfUA` | `false` | Write PDF/UA, at `pdf_ua_part`: tagged, with XMP metadata (`pdfuaid`), the title shown as the window title, link descriptions and field labels. A document without a title, a language, `alt` on its images and `figure` boxes, labels on its fields, or headings in order fails with `UA_CONFLICT`; encryption must keep `contentAccessibility` |
| `pdf_ua_part` / `with_pdf_ua_part` (also sets `pdf_ua`) | `pdfUA: true \| '1' \| '2'` | `PdfUaPart::Ua1` | The PDF/UA part: 1 (ISO 14289-1, PDF 1.7) or 2 (ISO 14289-2, PDF 2.0; see [Tagged PDF and PDF/UA](#tagged-pdf-and-pdfua)). `PdfUaPart` parses `"1"` and `"2"`. Part 2 with `pdf_a` is `INVALID_VALUE` at `options.pdfUA` in Rust and `INVALID_ARGUMENT` in Node. Node: another string is `INVALID_ARGUMENT` |
| `attachments` / `with_attachments` (`AttachmentStore`) | `attachments: Record<string, Uint8Array>` | none | The bytes of the files the document's `attachments` name in `src`, by id. A `src` missing from them is `ATTACHMENT_NOT_REGISTERED` |
| `hyphenation` / `with_hyphenation` | (`registerHyphenationPatterns`) | none | Runtime TeX patterns; and, for a build without the `embedded-hyphenation` feature, built-in patterns given with `register_builtin` |
| `deferred_fonts` / `with_deferred_fonts` | (`@inkoxide/wasm`: `deferredFonts`, id → URL) | none | Ids of declared fonts that may be missing: a render that does not draw with one, or try it for a character, renders as if it were registered; one that does fails with `NOT_LOADED` naming it. A fallback is tried for a character only once the font of its run is registered |
| `encryption` / `with_encryption` (`Encryption`) | `encryption: { userPassword?, ownerPassword?, permissions?, entropy? }` | none | Encrypt the PDF with passwords and permissions (see [Encryption](#encryption)). Not with PDF/A |
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
are left out. The "b" levels make the pages look the same in every reader;
the "a" levels are tagged too (see `tagged`).

| Level | Node | Rust | Notes |
|---|---|---|---|
| PDF/A-2b | `true` or `'2b'` | `PdfALevel::A2b` (default) | Based on PDF 1.7 |
| PDF/A-3b | `'3b'` | `PdfALevel::A3b` | PDF/A-2b, with the document's `attachments` as associated files (`/AF`) |
| PDF/A-1b | `'1b'` | `PdfALevel::A1b` | Based on PDF 1.4, for archives that require it. Font subsets list their glyphs (`CIDSet`) |
| PDF/A-2a | `'2a'` | `PdfALevel::A2a` | PDF/A-2b, tagged |
| PDF/A-3a | `'3a'` | `PdfALevel::A3a` | PDF/A-3b, tagged |

In Rust, `PdfALevel::ALL` lists the levels, `as_str` gives a level's
short name (`"1b"`, `"2b"`, `"2a"`, `"3b"`, `"3a"`, as Node and the CLI take it) and
`from_name` reads one back. `PdfALevel` also implements `FromStr`: a name
that is not a level is a `ParsePdfALevelError`, whose message lists the
levels.

What a level does not allow (a page side under 3 points at every level,
transparency under PDF/A-1b) is `PDFA_CONFLICT`, at the path of its first
use. The [code table of `SCHEMA.md`](SCHEMA.md#validation-error-codes)
lists what counts as transparency. A `backgroundColor` of alpha 0 draws
nothing and is accepted at 1b. An image whose alpha is only clear or
opaque (a GIF's transparency) is written with a stencil mask, which
PDF/A-1b allows; one with partial alpha needs transparency, unless
`flattenAlpha` (`flatten_alpha`) draws it on white. `validate` reports
`PDFA_CONFLICT` only when given the render's options
(`validate_with_options` in Rust, the `options` argument in Node and
WebAssembly, `--pdf-a` in the CLI), and a colour glyph that needs
transparency only `render` finds, after shaping.

A PDF/A-1b file also keeps within the limits of PDF/A-1: real numbers
within ±32767, strings of at most 65535 bytes, names of at most 127
bytes, arrays of at most 8191 entries, dictionaries of at most 4095, and
graphics states nested at most 28 deep. A document that would pass one
fails with `PDF_ERROR`.

In the CLI, `--pdf-a` alone writes PDF/A-2b, and a level follows an
equals sign:

```sh
inkoxide render doc.json --fonts fonts/ --pdf-a=1b
inkoxide validate doc.json --fonts fonts/ --pdf-a=1b
```

## Tagged PDF and PDF/UA

With `tagged` (Rust: `with_tagged`), the file has a structure tree:
what each part of the page is, in reading order, for screen readers,
reflowing viewers and text extraction. `pdfUA` (`with_pdf_ua`) writes
PDF/UA, which is tagged: part 1 (ISO 14289-1) by default, part 2
(ISO 14289-2) with `pdfUA: '2'` (`with_pdf_ua_part(PdfUaPart::Ua2)`).
The PDF/A levels 2a and 3a are tagged too, and combine with part 1.
Part 2 is PDF 2.0, which writes the structure elements in PDF 2.0's
namespace and links to structure destinations; PDF/A-1, -2 and -3 are
older than PDF 2.0, so part 2 with `pdfA` is refused. In the CLI,
`--tagged`, `--pdf-ua` (part 1) and `--pdf-ua=2`.

- **Elements.** The reading order is document order, whatever `zIndex`,
  floats or positioning do on the page. A text is a paragraph and an
  image a figure with its `alt` and its bounding box; inside a
  paragraph, an image with `alt` is a figure and a linked run a link,
  each where it is among the text. A table keeps its header rows (`TH`,
  scope column), header cells (`header`, scope row) and spans; a field is
  a form field with its `label` (`/TU`), a note or an attachment icon an
  annotation, and a link a link that says what it is (`/Contents`: the
  text it covers, or else where it goes). A box is no element of its own
  unless its [`role`](SCHEMA.md#role) makes it one (`h1`–`h6`,
  `section`, `blockQuote`, `figure`, a list and its parts); a `figure`
  box that draws only its background and borders is still a figure.
  Under part 2, a box with a `link` holds everything inside it in its
  link, which PDF 2.0 does not let hold a paragraph: the roles inside it
  make no elements, as inside a paragraph (a link on a heading's runs
  keeps the heading). A list says how its labels number it
  (`/ListNumbering`), read from them: `1.`, `a)`, `iv`, `•` and the like.
- **Decoration** is marked as artifacts, which screen readers skip:
  headers and footers (pagination artifacts that say which), other
  content painted on every page, repeated table header rows, backgrounds
  and borders, nodes with the role `artifact`, and images with an empty
  `alt`, in text too. Under PDF/A and PDF/UA, glyphs a font lacks
  (`.notdef`) are left out.
- **Glyphs.** Every glyph drawn maps to text, as PDF/UA requires: a
  glyph a font draws apart from the first of its cluster (a mark, the
  dots of an Arabic letter) maps to the combining grapheme joiner, inside
  a span with no text (`/ActualText ()`), so that extraction reads the
  cluster once.
- **Parts of SVG images.** An element of an SVG image (a group, `use`,
  shape, `image`, `text` or `a`) with `aria-label`, or else a `<title>`
  child, is a figure of its own with that text as its `alt`, inside the
  image's figure; `aria-hidden="true"` makes an element decoration. Only
  the outermost SVG counts, and only elements that are drawn (not those
  in `defs`, `symbol`, `clipPath`, `mask`, `marker`, patterns, gradients
  or filters). Inside a group drawn in a layer of its own (with
  `opacity`, a mask, a blend mode, `isolation` or a clip path other than
  one path), a part is a figure only when it is all the group draws; the
  others are read with the figure around the group. Each placement of the
  image is a form XObject of its own,
  with its parts marked inside it. An image with parts and `alt` is not
  one `/ActualText` span, which would hide them.
- **`UA_CONFLICT`.** With `pdfUA` (either part), a document without a `meta.title`, a
  `meta.lang`, `alt` on each image (in text too) and `figure` box that
  is not decoration, a `label` on each field (in decoration too: its
  widget is tagged), headings that go down one level at a time from
  `h1`, or lists made of items (a `listItem` inside a `list`, a
  `listBody` inside a `listItem`) fails, at the path of the first thing
  it lacks. Under part 2, so does a `#id` link to decoration (a `fixed`
  node, one with the role `artifact`, or a node inside one) or a
  bookmark on it: part 2 links and bookmarks go to a structure element,
  which decoration has none of. `render` reports it, and so does
  `validate` given the render's options. An encrypted file must keep
  `contentAccessibility`.

Without these options the output does not change. A tagged file is the
same on any number of threads, draws the same pixels and extracts the
same text as untagged; it is larger, about 2.5 times for a long report
of tables. With compression on, other than at PDF/A-1b, the structure
elements and the parent tree go into object streams (up to 128 objects
each) and the file ends with a cross-reference stream, not a table: a
tagged 100-page report is half the size it would be otherwise. Encrypted
files encrypt the object streams as a whole.

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
| `entropy: Option<Vec<u8>>` | `entropy: true \| Uint8Array` | none: the same input gives the same file |

- At least one password is needed. A password is any text SASLprep
  (RFC 4013) accepts, which PDF 2.0 prepares passwords with: no control,
  private-use or unassigned characters, and no text that mixes
  right-to-left and left-to-right letters. Once prepared (NFKC, non-ASCII
  spaces made spaces, soft hyphens and the like removed), it is at most
  127 bytes of UTF-8. `"a\u{0308}"` and `"ä"` are the same password.
- `entropy` is 16 to 256 random bytes mixed into the keys, so renders of
  the same input differ; in Node and the browser, `true` draws 32 with
  `crypto.getRandomValues` for each render. Without it, encryption is
  deterministic (see [Determinism](#determinism)).
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
  glyphs). Colour glyphs (emoji) are drawn by fonts of their own, whose
  widths and number of glyphs are those of the colour glyphs used.
  Radio buttons, whose states are
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
   names the file of a tag), or, for Romanian (`ro`, `ro-MD`), the engine's
   rules: hyph-utf8's Romanian patterns have no licence, so the engine
   hyphenates Romanian by the syllable rules of DOOM2, in every build,
   with no file to load;
3. none.

Soft hyphens (U+00AD) in the text are breaks with or without patterns,
unless `hyphenationPenalty` is 10 000 or `whiteSpace` is `nowrap` or
`pre`.

When a word wider than its line is broken between characters (as CSS
does with `overflow-wrap: break-word`), a grapheme cluster is never split.
The word still counts whole when a table column or a box is sized to its
content.
Text with `whiteSpace` `nowrap` or `pre` is not broken: a long word
overflows the line.

Patterns are registered for a BCP-47 tag with a two- or three-letter
language (`en`, `en-GB`, `haw`), in any case and with `-` or `_`.
Patterns for `en-GB` and for `en-US` coexist; patterns for `en` serve
every English tag without patterns of its own.

Patterns are TeX files in the hyph-utf8 format: `\patterns{…}`, with
an optional `\hyphenation{…}` exception list, in UTF-8. English is
built in; to hyphenate it with another pattern set (or to add a
language that is not built in), register a file. A hyphen leaves at
least as many letters before and after it as the language's built-in
patterns do (two and three in English), else two; exception words
break where they are written.

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
bound is `OUT_OF_RANGE`. In every front end (`@inkoxide/react`, `@inkoxide/vue`,
`@inkoxide/svelte`, `@inkoxide/angular`), the styles `fontFeatureSettings` and
`fontVariantNumeric` fill a run's `fontFeatures` (see [`GUIDE.md`](../packages/react/GUIDE.md)).

## Variable fonts

A variable font (TrueType with `gvar`, or CFF2) is registered once, with
the same call as any font. Each [font declaration](SCHEMA.md#fontdecl)
that draws with it names it in `source` and gives its axis values in
`variations`. Layout, metrics (`MVAR`) and shaping
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
- Text draws at the optical size of its font size: a font with an `opsz`
  axis gets an instance per size a document uses, unless the declaration
  sets `opsz` in `variations` or `opticalSizing: "none"`.
- A declaration with `weightRange` (`[100, 900]`) or `stretchRange`
  (`[75, 100]`, percentages) stands for every weight or width in it, as a
  CSS `@font-face` range does: SVG text is matched to it, and draws at its
  own weight and width (`wght` and `wdth`), the nearer end when outside.
  Matching follows CSS: the family, then the width (`stretch`, 100 when
  absent), the style, the weight.
- Each of those instances is embedded as one of the declaration's, at most
  64 per declaration (the smallest values first) and 1024 in a document
  (those of the first declarations first); text past them draws at the
  declaration's own.
- An instance's `FontBBox` holds its own outlines, which a heavy instance
  draws outside the default instance's `head` box.
- `register` refuses (`FONT_ERROR`) a variable font with more than 63
  axes or an `avar` table other than version 1.0, which the two font
  parsers would read differently. An instance that cannot be made fails
  the render with `FONT_ERROR`.

In `@inkoxide/render`, `Font.register` takes a weight range
(`fontWeight: [100, 900]`), a width or a width range (`fontStretch:
'condensed'`, `[75, 100]`) and `fontVariationSettings`, and the builder
declares one instance per weight, width and settings a document uses (see
[`GUIDE.md`](../packages/react/GUIDE.md)).

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
equal files can be told apart from different ones; the `entropy` option
mixes random bytes in, for output that cannot be linked to its input. The
derivation is not
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
| `PDFA_CONFLICT` | `ValidationCode::PdfaConflict` | `InkoxideError.code` from `render` | The document uses something its PDF/A level does not allow: a page side under 3 points, at any level, transparency under PDF/A-1b, or `attachments` under PDF/A-1b, 2b and 2a. The path names the first such use; a colour glyph that needs transparency is found once the text is shaped, after the rest, and named only when nothing else conflicts. `validate` reports it with the render's options, but for colour glyphs |
| `UA_CONFLICT` | `ValidationCode::UaConflict` | `InkoxideError.code` from `render` | With `pdfUA`, the document lacks something PDF/UA requires, at either part (see [Tagged PDF and PDF/UA](#tagged-pdf-and-pdfua)). The path names the first thing it lacks. `validate` reports it with the render's options |
| `FONT_ERROR` | `InkoxideError::Font` | same | Font bytes cannot be parsed or embedded, or the shaper panicked on the font or refused its text, or the font shaped the text to far more glyphs than it has bytes (the message names the font) |
| `IMAGE_ERROR` | `InkoxideError::Image` | same | An image cannot be embedded. Images that do not decode are `INVALID_IMAGE` at their path, found by validation first |
| `LAYOUT_ERROR` | `InkoxideError::Layout` | same | Layout failed, for example the page limit (100,000 pages) or the limit on content repeated on every page was exceeded |
| `PDF_ERROR` | `InkoxideError::Pdf` | same | Writing the PDF failed, or a PDF/A-1b file would pass one of the limits of PDF/A-1 |
| `HYPHENATION_ERROR` | `InkoxideError::Hyphenation` | same | Hyphenation patterns cannot be parsed |
| `NOT_LOADED` | `InkoxideError::NotLoaded` (`NotLoaded { fonts, hyphenation }`) | `@inkoxide/wasm` only: from `renderSync`, from `render` when it cannot fetch what the document needs, from `init` when the module cannot be fetched or compiled, and from any call before `init` has resolved | The render needs a font of `deferred_fonts`, or built-in hyphenation patterns that a build without the `embedded-hyphenation` feature has not been given: register them and render again, until it renders: a deferred fallback is needed only once the font of its run is registered (`@inkoxide/wasm`'s `render` fetches them and does). `@inkoxide/render/browser` reports a file it could not fetch as `INTERNAL` |
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
something its PDF/A level or PDF/UA does not allow (`PDFA_CONFLICT`, `UA_CONFLICT`), on an
attachment's `src` that is not among the render's `attachments` or is too large
(`ATTACHMENT_NOT_REGISTERED`, `ATTACHMENT_TOO_LARGE`), on an SVG image
whose text, with its paths, draws more than 5,000,000 path segments and
glyphs (`IMAGE_ERROR`; its paths alone are checked by validation), or on
a failure in writing the PDF (`PDF_ERROR`). Given the render's
options (Rust `validate_with_options`; in Node and WebAssembly the
fourth argument, the object `render` takes, whose `format` and `images`
the arguments override), `validate` reports the conflicts and the
attachments too, and the options' invalid `creationDate` or
`encryption` as `INVALID_VALUE` at `options.…`; a fallback font in
`deferred_fonts` (WebAssembly `deferredFonts`) may be missing. Only
`render` still finds the layout limits, the fonts that cannot be shaped
or embedded, and at PDF/A-1b a colour glyph that needs transparency. `validate` reads the
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
