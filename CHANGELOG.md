# Changelog

All notable changes to the packages in this repository. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and every package follows
[Semantic Versioning](https://semver.org/). Since 0.7.0, `inkoxide`, `@inkoxide/render`,
`@inkoxide/react`, `@inkoxide/tailwind` and `@inkoxide/style` share one version and are released
together, and so does `@inkoxide/wasm` from its first release. The document schema has its own version and
[changelog](packages/schema/CHANGELOG.md).

## Unreleased

### Added

- Threads in the browser. On a page that is cross-origin isolated, `@inkoxide/render/browser`
  renders on a pool of Web Workers that share the engine's memory: its `threads` option is
  `'auto'` by default (the number of cores, at most 8), and `engineStatus().threads` says how many
  the engine got. The PDFs are the same bytes as on one thread. The engine alone renders a 98-page
  report about 2.9 times as fast on 8 threads (in Node); a React document gains less (1.3 times
  for a 43-page report in Chromium and WebKit). `@inkoxide/wasm` ships a second module, `inkoxide-threads.wasm`, which
  `init(source, { threads })` loads and starts a pool for, and `threadCount()`; elsewhere it loads
  the one-thread module as before. A bug in the engine on a pool thread ends the render's worker,
  and the render fails with `INTERNAL`.

- A Tailwind build plugin, so that a browser need not load Tailwind's compiler. In client
  bundles, `inkoxideTailwind()` (Vite, Rollup, Rolldown) and `inkoxideTailwindEsbuild()` from
  `@inkoxide/tailwind/plugin` point `@inkoxide/tailwind` at `@inkoxide/tailwind/precompiled`,
  which reads a table of the classes the sources use, scanned at build time: the Tailwind
  JavaScript a page downloads goes from about 90 KB to about 6 KB with brotli, and styles are
  the same. Server bundles keep the compiler. A class the scan does not see fails as an unknown
  class, with a reason that says so; the plugin's `safelist` keeps classes put together at run
  time.

- Vertical SVG text and SVG text on a path. `writing-mode` (`vertical-rl`, `vertical-lr`, `tb`,
  `tb-rl`, `sideways-rl`, `sideways-lr`) lays text down a column, CJK upright in its vertical
  forms with the font's vertical metrics and Latin sideways, by `text-orientation` (or
  `glyph-orientation-vertical`) and the Unicode vertical orientation of each character.
  `<textPath>` sets text along a path or basic shape, or its own `path` data, with
  `startOffset` and `side`. Both read in order when the PDF's text is extracted. SVG text also
  takes `font-feature-settings`, `font-variant` and `font-kerning`. `@inkoxide/render` has
  `<TextPath>`, `writingMode` on SVG `<Text>`, and `textOrientation`, `fontFeatureSettings`,
  `fontVariant` and `fontKerning` on SVG text. Vertical text on a path fails with `INVALID_IMAGE`, as do the text
  paths of an SVG with more than 100,000 segments in all, and an SVG's text with more than 64
  different sets of font features; the SVGs of a render with more than 64 in all fail with
  `IMAGE_ERROR`.

- Tagged PDF and PDF/UA-1. The render option `tagged` writes a structure tree in document
  order: paragraphs, headings, lists (with the numbering their labels show), sections, figures
  with their alternative text and bounding box, tables with header cells and spans, links that
  say what they are, form fields and annotations; the links and images with `alt` in a paragraph
  are elements in place among its text. Everything else (backgrounds, borders, headers and footers,
  as pagination artifacts, fixed elements, repeated header rows, images with `alt=""`, in text
  too) is marked as decoration. Every glyph maps to text, and extraction reads each cluster once.
  `pdfUA` writes PDF/UA-1, with its metadata and without `.notdef` glyphs, and refuses a document
  without a title, a language, alternative text on its images (in text too), labels on its fields
  (in headers too), headings in order or lists made of items (`UA_CONFLICT`). New PDF/A levels
  2a and 3a are tagged. Nodes take `role`, `figure` boxes `alt`, table cells `header` and fields
  `label`; `@inkoxide/render` takes them as props (the `Role` type is exported), with
  `<Document tagged pdfUA>` and `conformance="PDF/A-2a"`. The CLI takes `--tagged` and
  `--pdf-ua`. Untagged output is unchanged.

- File attachments. A document's `attachments` are files the PDF carries (from the render
  option `attachments`, or base64 `data`), listed in the viewer's attachments panel; `attachment`
  nodes and runs show an icon (a push pin, a paper clip, a chart or a tag) that opens one, and
  `meta.pageMode` takes `useAttachments`. Under PDF/A-3b they are associated files, and
  `meta.xmpExtensions` declares and fills in XMP schemas, such as the metadata a Factur-X or
  ZUGFeRD e-invoice declares (the output passes veraPDF's PDF/A-3b profile; it is not checked
  with a Factur-X validator); PDF/A-1b, 2b and 2a refuse attachments, and 3a allows them. New validation codes
  `UNKNOWN_ATTACHMENT`, `ATTACHMENT_NOT_REGISTERED` and `ATTACHMENT_TOO_LARGE`. Rust takes an
  `AttachmentStore` in `RenderOptions::attachments`, whose `insert_too_large` registers a file
  over `AttachmentStore::MAX_FILE_BYTES` (64 MiB) by its size alone; the CLI takes
  `--attachments <DIR>`, whose files are named by their file name, and does not read one over
  the limit: a document that uses it fails with `ATTACHMENT_TOO_LARGE`. `@inkoxide/render`
  takes `<Document attachments xmpExtensions>`, `<Attachment>`, `facturX()` and
  `facturXRelationship()`.

- Variable fonts. `inkoxide` accepts TrueType (`gvar`) and CFF2 variable fonts, and a font
  declaration takes `source` (the registered font it draws with) and `variations` (axis values):
  text is shaped and measured at that instance, and the PDF embeds it as a static TrueType
  subset, never the variable font. `Font.register` takes a weight range (`fontWeight: [100,
  900]`) and `fontVariationSettings`, and styles and Tailwind's `[font-variation-settings:…]`
  take `fontVariationSettings`. `@inkoxide/style` exports `parseFontVariationSettings` and the
  types `FontVariationSettings` and `FontVariations`, and `@inkoxide/react` exports
  `fontVariationsValue`. An axis the font does not have is left out, with a warning. A variable
  font with more than 63 axes or an `avar` table of version 2 is refused, and so is one whose
  outlines the instancer cannot vary (CFF with CFF2, or `glyf` without `gvar`). A declaration
  takes at most 16 axes, and a document at most 64 declarations with `variations`. The
  WebAssembly engine grows by about 140 KB with brotli, for the instancer.

- `@media` for the page size. `@inkoxide/react` style objects take `'@media …'` blocks, and
  `@inkoxide/tailwind` the page variants `sm:` … `2xl:`, `max-sm:` … `max-2xl:`, `min-[…]:`,
  `max-[…]:`, `portrait:`, `landscape:`, `print:` and `[@media(…)]:`, and a theme's
  `@custom-variant` under one or several `@media`. A query reads the page's width and height
  (margins included), `aspect-ratio` and `orientation`; features of screens (`hover`,
  `prefers-color-scheme`) are errors, and so is a mistake in a block or class for another page
  size. Each element's queries are answered for its page before layout, so the engine and the
  schema do not change. `@inkoxide/react` and `@inkoxide/render` export the `MediaStyle` type, a
  style object with `@media` blocks (`StyleProp` stays without them, so a style typed as `object`
  still passes).

- `inkoxide`: `PdfALevel` implements `FromStr` (`"2b".parse()`), with `ParsePdfALevelError`; the
  CLI and the Node and WebAssembly bindings read the level through it. An unknown `pdfA` level in
  the bindings now says `(expected one of 1b, 2b, 2a, 3b, 3a); pdfA also takes true or false`.
- The `inkoxide` and `inkoxide-cli` crates have their crates.io metadata: a README each, the
  licence, keywords and categories. The packages leave out the tests and benchmarks, which read
  fixtures outside the crate.
- `@inkoxide/render`: `<Svg>` and `<Canvas>` take `alt`, as `<Image>` does.
- `textWrap: 'balance'` (`text-balance`): the lines of a heading or a caption come out about
  as long as each other. Each stretch between forced breaks is broken first fit into as many
  lines as `wrap` gives, in the narrowest width that keeps that many; a stretch of one line or
  more than 10, beside a float, or with a word wider than its line breaks as `wrap`.
- `hyphenationPenalty` (a style, and a prop of `<Text>` in `@inkoxide/react` and
  `@inkoxide/render`, which rejected it): how much `textWrap: 'pretty'` avoids breaking at a
  hyphenation point, 0 to 10 000, default 50, as TeX's `\hyphenpenalty`. From 10 000, a word is
  never broken at a hyphenation point, whatever `textWrap`.
- `shapeOutside` and `shapeMargin` on floats (a style; `shapeOutside` and `shapeMargin` in the
  `style` prop of `@inkoxide/react`, as CSS strings or objects; `[shape-outside:…]` and
  `[shape-margin:…]` as Tailwind arbitrary properties): the lines beside a float wrap around a
  circle, an ellipse, a rounded inset or a polygon in its margin box instead of the box, as CSS
  `shape-outside` with a basic shape. The float is still placed, and boxes beside it narrowed,
  by its margin box. `@inkoxide/style` has `parseShape`.
- `flexGrow` and `justifyContent` in a box with floats: the room a box of known height leaves
  goes to auto margins, else to the children that grow, else around and between the children
  as `justifyContent` says, as in a column box. `flexGrow` on a float itself is refused.
- Romanian hyphenation (`ro`, `ro-MD`). hyph-utf8's Romanian patterns have no licence, so the
  engine hyphenates Romanian by the syllable rules of DOOM2: a hyphen goes only between the
  consonants that separate two vowels (`ca-li-ta-te`, `în-tre-prin-de-re`, `sculp-tor`,
  `func-ție`), never between vowels, with `ș`/`ț` written with a comma or a cedilla. It works in
  every build, `@inkoxide/wasm` included, with no file to load; patterns registered for `ro` still
  take precedence.
- Form fields: `sort` (the options of a select or list in alphabetical order), `edit` (a select
  that takes a typed value besides its options), `noSpell` (no spelling check, on a text field
  or a select with `edit`) and `align` (`left`, `center` or `right`: where the value of a text,
  select or list field goes in its box, in the appearance and as the field's `/Q`). They are
  props of the field components of `@inkoxide/react` and `@inkoxide/render`.
- An `icon` for notes: `note` (the default), `comment`, `key`, `help`, `newParagraph`,
  `paragraph` or `insert`, written as the annotation's `/Name` and drawn into its appearance; a
  prop of `<Note>`.
- Colour emoji from SVG-in-OpenType fonts (the `SVG ` table, its documents plain or gzipped, one
  glyph each or many in one), with `currentColor` and `context-fill` taking the run's colour.
  `COLR` v1 sweep gradients draw around their centre, with every stop, and its composite modes
  draw as they say: the blend modes (`multiply`, `screen`, `color-dodge` and the others) and the
  Porter-Duff ones (`src-in`, `dest-out`, `xor` and the others; `plus` as `screen`). They drew
  with the sweep's first colour, and as `src-over`.
- Variable fonts: optical sizes, widths and ranges. Text in a font with an `opsz` axis draws at
  the optical size of its font size, each size an instance of its own, unless the declaration
  sets `opsz` or the new `opticalSizing: "none"`. A font declaration takes `stretch` (its width,
  a percentage) and `weightRange` and `stretchRange`, as CSS `@font-face` does: SVG text is
  matched by `font-stretch` too (the family, then the width, the style, the weight, as CSS
  does) and draws at its own weight and width within the ranges. At most 64 such instances per
  declaration. `@inkoxide/react` and `@inkoxide/render` have `fontStretch` in styles, on SVG
  text and in `Font.register` (a width or a range, `[75, 100]`, drawn at its `wdth`), and
  `@inkoxide/tailwind` reads `font-stretch-*`. An instance's `FontBBox` holds its own outlines;
  it was the default instance's, which a heavy instance can exceed.

### Changed

- Colour glyphs in text are the glyphs of Type 3 fonts, one per colour font for each 255 of its
  glyphs, instead of an image and an invisible space each: a glyph's image is in the PDF once,
  and the text copies and searches through the font's `ToUnicode` map. Colour glyphs in the
  text of an SVG image are still images.

- `@inkoxide/react`: a negative `flexGrow` or `flexShrink` is refused with the same message as
  its Tailwind class, from `@inkoxide/style` (`flexGrow: -1 (cannot be negative)`); a `flexGrow`
  of `NaN` is refused too.
- A column box after a float, with children and no floats of its own, keeps its width and lays
  its children out beside the float, so that its text wraps beside the float line by line, as
  CSS does (to a depth of 8 such boxes). It was narrowed to the room beside the float over its
  whole height, or moved below it. Boxes with `overflow: 'hidden'` or an `aspectRatio`, rows,
  grids, images and tables are still narrowed or moved below, as CSS does with boxes that make
  a block formatting context: give a box `overflow: 'hidden'` to keep the old layout.

### Fixed

- A class for which Tailwind writes CSS that does not parse is an unknown class in
  `@inkoxide/tailwind`, instead of an error from postcss.
- A run's `verticalAlign` or a table column's `width` of the wrong type (`true`, an object) is
  reported as `INVALID_TYPE` with what it takes, instead of naming a type inside the engine.
- `@inkoxide/render/browser`: the types of `generate` take `signal` and `timeoutMs` (its
  `GenerateOptions` is `BrowserRenderOptions`); they already worked at run time.
- `@inkoxide/react` keeps `alt=""` in the document: in tagged output it makes an image decoration.
- `alt` on a block `<Image>` is no longer dropped: an image node takes `alt` (see the schema
  changelog), and copying, searching and screen readers get it for the image, as they do for an
  image in text.
- The glyphs a form field shows, and those embedded for editing it, map to their characters in
  the font's `ToUnicode` map: text extraction from field appearances works, and veraPDF's PDF/UA
  check no longer reports them (ISO 14289-1, 7.21.7).
- With `subsetFonts: false`, a variable font's instance that cannot be made for every glyph embeds
  the glyphs the document uses, as with subsetting, instead of failing the render.

### Repository

- The release workflow publishes the crates `inkoxide` and `inkoxide-cli` to crates.io, in a
  `crates` job after the npm packages, with trusted publishing: crates.io trusts the workflow, and
  the job's token is short-lived. A version already on crates.io is skipped (#137).
- The documentation is reorganised and brought up to date. The implemented plans in `docs/plans/`
  are removed: what they left for later is in [ROADMAP.md](ROADMAP.md), and the design of
  rendering in the browser is [ADR 0002](docs/adr/0002-browser-rendering.md). Releasing is
  described in [docs/RELEASING.md](docs/RELEASING.md). `@inkoxide/react`, `@inkoxide/tailwind`
  and `@inkoxide/style` have READMEs, and `packages/schema/EXAMPLES.md` has examples of forms,
  notes, grids, floats, inline images and font features. The benchmark files lose their `-inkoxide` suffix.

## 0.9.1 - 2026-10-16

`inkoxide`, `@inkoxide/wasm`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/tailwind` and
`@inkoxide/style` 0.9.1. Output is unchanged.

### Removed

- `@inkoxide/render` no longer ships MIGRATING.md, and the benchmarks measure Inkoxide alone.

## 0.9.0 - 2026-10-16

`inkoxide`, `@inkoxide/wasm` (new), `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/tailwind`
and `@inkoxide/style` 0.9.0, and `@inkoxide/schema` 2.5.1. Rendering in the browser, with the
engine as WebAssembly. Native output is unchanged.

### Added

- Rendering in the browser. `@inkoxide/render/browser` renders the same templates with the
  engine as WebAssembly, in a Web Worker, to the same PDF bytes as Node (but for the last digits
  of SVG marker angles): `renderToBuffer` and `renderToBlob`, whose `signal` or `timeoutMs`
  stops a render, `generate`, `clearCaches`, and the React helpers `usePdf`, `<PdfDownloadLink>`
  and `<PdfViewer>`. It needs no
  cross-origin isolation, and a Content Security Policy needs only `'wasm-unsafe-eval'`. The
  engine is about 1.2 MB to download with brotli; a bundled font downloads when a document first
  draws with it (Noto Sans, the fallback, only for a character Inter lacks), and a language's
  hyphenation patterns when a document first hyphenates text in it. See GUIDE.md, "In the
  browser".
- `@inkoxide/wasm`, the engine as WebAssembly, with the functions of `inkoxide`, and `init`,
  `clearCaches` and `loadHyphenation`; its `render` fetches the hyphenation patterns and the
  `deferredFonts` a document needs. It is released with the other packages, at their version.
- `@inkoxide/tailwind` has a browser entry, chosen through the `browser` export condition, with
  Tailwind's theme bundled.
- Rust: `inkoxide::clear_caches()`; the cargo features `msgpack`, for
  `InputFormat::MessagePack`, and `embedded-hyphenation`, for the built-in hyphenation
  patterns, both on by default; without the latter, `HyphenationStore::register_builtin` gives
  a language's patterns (`builtin_hyphenation_file` names its file). `RenderOptions::deferred_fonts`
  lists declared fonts that may be missing until a render needs one. A render that needs what
  it was not given fails with `InkoxideError::NotLoaded` (`NOT_LOADED`), naming it.
- A text node with `hyphenate` on and no `lang` (nor `meta.lang`) logs a warning, and the
  `warnings` of an `@inkoxide/render` result say so, since such text is not hyphenated.

### Changed

- `@inkoxide/react` validates trees in development with code generated from the schema, which
  compiles nothing at run time: it no longer depends on `ajv`.
- The engine matches words against the built-in hyphenation patterns itself, with hypher's
  patterns and algorithm, so that they can also come at run time; it hyphenates as before.

### Fixed

- The READMEs, GUIDE.md, MIGRATING.md, docs/API.md, docs/SCHEMA.md, docs/CLIENT_INTEGRATION.md,
  the schema README and descriptions, and the `--png-dir` help of the CLI no longer contradict
  the code. They now say that a word wider than its line breaks between any two characters, and
  that `[word]` from a hyphenation callback keeps a word from being hyphenated, not from breaking.

### Security

- The work budget of a `COLR` colour glyph also counts the paint tables its paint graph reaches,
  each as often as it is reached, before the glyph is painted: past 10000 the glyph draws
  without colour, whether or not those paints draw anything. So does a glyph whose paint graph
  has a cycle or nests more than 64 deep, which the `COLR` specification does not allow.

## 0.8.0 - 2026-10-15

`inkoxide`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/tailwind` and `@inkoxide/style`
0.8.0, and `@inkoxide/schema` 2.5.0. Fixes for layout, pagination, colour glyphs, forms, PDF/A
and the packages' option checks. Upgrading: some documents and templates that were accepted are
now errors, because what they asked for was ignored or drawn wrong. These are a field character
its font lacks, two checked radio buttons in a group, `permissions` with equal owner and user
passwords, a PDF/A page with a side under 3 points, `options: []`, React props an element would
ignore, and wrongly typed render options. `baseline` alignment and `fixed` elements in the body
now render as documented (see **Fixed**).

### Fixed

- An SVG without `width`, `height` or `viewBox` is laid out at the size of what it draws without
  its text, the size it is drawn at, instead of a box sized by its text that stretched the drawing
  and clipped the text.
- `alignItems` and `alignSelf: 'baseline'` align the first baselines of the items' text, as in CSS,
  instead of their bottom edges.
- A table nested in a row split across pages continues below the repeated header rows of the table
  around it, and repeats its own header rows below them; its first row on each new page was hidden
  under the outer header.
- Fixed elements in the body are painted in paint order, by `zIndex` then document order, as
  without `fixed`; they were painted over everything else in the flow. A fixed element inside an
  element that is not painted on a page (`display: none`, `opacity: 0`, `pages`) is not painted
  there either.
- Encryption `permissions` with an owner password equal to the user password are `INVALID_VALUE` at
  `options.encryption.permissions`, since the user password would lift them.
- A second checked radio button in a group is `INVALID_VALUE`, and radio buttons of one value share
  an on state in encrypted files too, so a radio repeated on every page turns on and off as one, as
  in a plain file.
- A text, select or list field whose value or options have a character its font lacks is
  `INVALID_VALUE`, instead of the character being left out silently. Field text is drawn with its
  own font only, without fallback fonts, shaping or right-to-left ordering, and the docs now say
  so.
- `@inkoxide/render` and `inkoxide` reject a render option of the wrong type (`threads: '4'`,
  `pdfA: 5`, a permission that is not a boolean) with `ENGINE_REJECTED` / `INVALID_ARGUMENT` at its
  path instead of `INTERNAL`; `localImages` without `root` is a `RenderError`; and option errors no
  longer name `<Document>`. Options that are not an object now fail.
- The guide says what happens after a font fails to load (it is dropped, and later renders fall
  back from it), and that only faces of the same weight and style clash on their family name.
- `w-screen`, `h-screen`, `min-h-screen` and the other `*-screen` sizing classes are listed and
  tested as supported.
- `lineHeight` takes lengths and percentages (`'14pt'`), `letterSpacing` in `em` reads the same in
  a style and a class, and props an element would ignore (`<Page wrap={false}>`, `href` on a `Cell`
  or `Header`, `<Document pdfVersion>`, `hyphenationPenalty`, `className` on an SVG shape) now
  throw a `TemplateError`; errors inside a `render` prop keep the components around it in their
  stack.
- An emoji drawn from `emojiImages` in a run with `verticalAlign: 'super'` or `'sub'` is raised or
  lowered with the run's text; it stayed on the line's baseline.
- The limit of 4096 colour glyphs per render counts only glyphs drawn in colour, not every glyph of
  a colour font. Glyphs of a bitmap-only colour font that are not drawn in colour (past the limits,
  or without colour, such as its space) still copy and search as their text instead of
  disappearing.
- `COLR` layers in the foreground colour take the run's `color`, as in browsers, instead of always
  drawing black. In the text of SVG images they still draw black.
- A page with a side under 3 points fails a PDF/A render with `PDFA_CONFLICT` at the page's `size`,
  at every level; such a file was not PDF/A.
- The engine and the JSON Schema agree on more values: the schema rejects negative sizes, gaps,
  padding and grid tracks as the engine does, integers may be written as `400.0`, `options: []` is
  `INVALID_VALUE` on every field, and percentages and `fr` sizes are bounded as written
  (`"10000.0001%"` is `OUT_OF_RANGE`).

## 0.7.1 - 2026-10-15

`inkoxide`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/tailwind` and `@inkoxide/style`
0.7.1, and `@inkoxide/schema` 2.4.1. Tighter limits on tables, `fallbackFonts`, SVG text and
repeated content, and fixes for encrypted files and remote images. Upgrading: a host entry in
`remoteImages.allow` now allows only port 443 unless it names a port, and encrypted files are
somewhat larger (see **Changed**).

### Changed

- A host entry in `remoteImages.allow` (`cdn.example.com`, `*.example.com`) allows only port 443,
  unless it names a port (`cdn.example.com:8443`). Host entries used to allow any port; URL
  entries already matched it.
- Encrypted PDFs give each font the widths of all its glyphs (`/DW` and `/W`), not only those of
  the glyphs used, so the widths no longer depend on the text. This adds up to about 5 bytes a
  glyph per font (some 15 KB for a Latin font); unencrypted output is unchanged.

### Fixed

- A colour glyph drawn as an image, in page text and in SVG text, extracts as the text of its
  own cluster, not the text it was first shaped with.
- Images are told apart by their bytes, not only by their hash: two different images whose
  hashes are equal are drawn as two images, and each is checked for transparency on its own
  under PDF/A-1b. Font subsets kept across renders are keyed by the font's SHA-256.
- A render waiting for an emoji image another render is already fetching stops at its own
  `remoteImages.timeoutMs`, and the download goes on for the other renders.
- SECURITY.md and the API docs say which form field state stays readable in encrypted files:
  checkbox states, the position of the chosen radio button, and the positions of a list box's
  selected and first visible options. Field values and options are encrypted.

### Security

- The tables of a document are limited to 1000000 grid slots in all (rows times columns,
  however the cells span them); more is `OUT_OF_RANGE` at the table's `rows`. Checking a
  table's rows takes time in proportion to them.
- `fallbackFonts` holds at most 256 ids; a longer list is `OUT_OF_RANGE`. A font listed more
  than once is tried once, where it is first listed.
- A form field `name` that breaks the name rules is reported once as `INVALID_VALUE` and is not
  checked further against other fields' names and groups.
- SVG text counts the properties each span inherits toward the SVG limits, as shapes do, and
  spans that inherit one `font-family` share it; an SVG over a limit fails with
  `INVALID_IMAGE`. The image cache counts the text read from an SVG in the size of its entry.
- `textLength` in SVG text takes time in proportion to the characters inside it, however many
  spans set one.
- A wavy text decoration draws its waves only where the page shows them, and at most 16 half
  waves per glyph: a wider piece of text gets longer waves.
- Headers, footers and fixed nodes count the alt text of images in text, the text of colour
  glyphs and link targets in the estimate of what they repeat on every page.

## 0.7.0 - 2026-10-15

`inkoxide`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/tailwind` and `@inkoxide/style`
0.7.0, and `@inkoxide/schema` 2.4.0. From this release on, the five share one version number and
are released together; the schema package keeps its own, since it versions the document format.
New: floats, optimal line breaking, inline images, colour emoji, text in SVG and Canvas, PDF/A-1b
and 3b, encryption, field groups, list boxes, number and date formats, and notes. Upgrading:
Canvas miter joins, `em` lengths in SVG images and font family case changed (see **Changed**), and
SVG text and field options have new limits (see **Security**). Encryption derives its keys
deterministically; that derivation has had an internal review but no independent one yet (see
[SECURITY.md](SECURITY.md#encryption)).

### Added

- Floats: `float: 'left' | 'right'` and `clear: 'left' | 'right' | 'both'` (Tailwind
  `float-left`, `float-right`, `clear-*`) on the children of a column `View`, a `Page` or a
  `Cell`. Text after a float wraps beside it, line by line; a box after it is narrowed to the
  room beside it, or moved below it; the parent grows to hold its floats; and a float moves to
  the next page with the lines beside it. A float taller than a page is sliced. `debug` on a
  paragraph beside a float shows its narrower line boxes.
- Encryption: user and owner passwords and permissions (printing at low or high resolution,
  modifying, copying, annotating, filling forms, content accessibility, document assembly),
  with AES-256 (PDF 2.0, revision 6). `RenderOptions::encryption` with `Encryption`,
  `Permissions` and `Printing` in Rust, `encryption: { userPassword, ownerPassword,
  permissions }` in the Node binding and in `@inkoxide/render`, and `<Document userPassword
  ownerPassword permissions>`, which the option replaces. Passwords are printable ASCII up to
  127 bytes, never part of the document tree, and never shown in errors or `Debug` output; in
  Node, a password that is not a string is `INVALID_ARGUMENT` at its path before the engine
  sees it.
  Output stays deterministic: the keys are derived from the document and the passwords.
  Radio buttons name their states by position in an encrypted file, with their values in
  `/Opt`, so the values are encrypted too, and the subset tags of font names are keyed by the
  file's keys. Encryption with PDF/A is `INVALID_VALUE` at `options.encryption` in Rust and
  `INVALID_ARGUMENT` in Node. SECURITY.md lists what encryption does not protect.
- PDF/A-1b and PDF/A-3b, next to PDF/A-2b: `RenderOptions::pdf_a_level` with `PdfALevel` in
  Rust (`with_pdf_a_level`; `PdfALevel::ALL` lists the levels), `pdfA: '1b' | '2b' | '3b'` in
  the Node binding and in `@inkoxide/render` (`true` is still PDF/A-2b), `--pdf-a=LEVEL` in the
  CLI (`--pdf-a` alone is still PDF/A-2b, also before the document), and
  `<Document conformance>` (`'PDF/A-1b'`, `'PDF/A-2b'`, `'PDF/A-3b'`, or the part alone), which
  the `pdfA` option overrides. The a and u levels, which need tagged PDF, are a
  `TEMPLATE_ERROR`.
- PDF/A-1b has no transparency: a render at 1b fails with the new code `PDFA_CONFLICT` at the
  path of the first colour with alpha below 1, `opacity` between 0 and 1, `debug`, image (or
  background image) with an alpha channel, or SVG drawn with opacity, a mask or a blend mode
  (`ENGINE_REJECTED` with the component in `@inkoxide/render`). `validate` does not know the
  level and does not report it. A PDF/A-1b file also keeps within the limits of PDF/A-1
  (numbers, strings, arrays, dictionaries, graphics state nesting) or fails with `PDF_ERROR`.
- `textWrap: 'pretty'` (Tailwind `text-pretty`) breaks a paragraph's lines over the whole
  paragraph for even lines (Knuth and Plass's total fit), with fewer hyphens; justified
  `pretty` lines may also shrink their spaces by up to a third. It is inherited. `'wrap'`, the
  default, keeps first fit and the same output; `'balance'` is reserved. `text-wrap` maps to
  `wrap` and `text-nowrap` to `whiteSpace: 'nowrap'`.
- Superscripts and subscripts: `verticalAlign: 'super'` or `'sub'` on a `Text` (the Tailwind
  classes `align-super` and `align-sub`) raises or lowers its baseline by the font's own
  superscript and subscript offsets, and the line grows to fit it. `verticalAlign` stays `top`,
  `middle` or `bottom` on a `Cell`.
- OpenType features: `fontFeatureSettings` as a CSS string (`"'tnum' 1, 'liga' off"`), a list
  of tags or an object of tags to values or switches, and `fontVariantNumeric`
  (`'tabular-nums slashed-zero'`); in Tailwind, `font-features-[…]`, `tabular-nums`,
  `oldstyle-nums`, `slashed-zero`, `ordinal`, `diagonal-fractions` and the other numeric
  classes. Both are inherited. A run takes at most 32 features, with values up to 65535, and a
  document at most 64 different sets of them (`OUT_OF_RANGE`). `@inkoxide/style` exports
  `parseFontFeatureSettings`, `parseFontVariantNumeric` and `RUN_ALIGNS`.
- `debug` on a `Text` outlines the paragraph line by line: each line box, its baseline, and the
  glyphs of every run on the line, so nested `Text` and `Link` elements show where their words
  landed.
- **`<Marker>`** in `<Svg>`: a shape drawn at the start, middle or end vertices of a path, line
  or poly shape, referenced with the new `markerStart`, `markerMid` and `markerEnd` props. It
  takes `viewBox`, `refX`, `refY`, `markerWidth`, `markerHeight`, `orient` and `markerUnits`.
  `Marker` and `MarkerProps` are exported.
- `fr`, the radius of the focal circle, on `RadialGradient`.
- The `<Canvas>` painter clips and paints gradients: `clip(rule?)` clips what follows to the
  current path until `restore()`, and `linearGradient(x1, y1, x2, y2)` and
  `radialGradient(x1, y1, r1, x2, y2, r2)` return a `CanvasGradient` (exported as a type) that
  takes colour stops with `stop(offset, color, opacity?)` and paints in place of a colour.
  `miterLimit(n)` sets how far mitred joins reach.
- **Text in the `<Canvas>` painter**: `font(name, size?)` (a registered font id, a family name,
  or one of pdfkit's standard names such as `Helvetica-Bold`, which picks the canvas's family in
  that weight and style), `fontSize(n)` (12 by default) and `text(str, x, y, options?)`, which
  draws one line with the fill colour or gradient, its top at `y` by default. The options are
  `width` with `align` (`justify`, as in pdfkit, is `left`: a line does not wrap), `baseline`,
  `characterSpacing`, `wordSpacing`, `underline`, `strike`, `fill` (whether to fill the text,
  `true` by default) and `stroke`; `CanvasTextOptions` is exported. A line break in the text is
  an error.
- **Text in SVG images**: `<text>`, its `<tspan>`s and `<a>`s are drawn as PDF text with the
  document's fonts, so it can be selected, searched and copied, in SVG files and in SVGs nested
  in them. Each span's font is the declared font that matches its `font-family`, `font-weight`
  and `font-style` (as CSS matches them), with the document's fallback fonts for characters it
  lacks. Positions (`x`, `y`, `dx`, `dy`, `rotate`), `text-anchor`, `dominant-baseline`,
  `alignment-baseline`, `baseline-shift`, `letter-spacing`, `word-spacing`, `text-decoration`,
  `textLength`, `lengthAdjust`, `xml:space`, style sheets, and fills and strokes with gradients
  and patterns work, also in clip paths, masks, patterns, markers and `use` copies, whatever an
  author style sheet says about the paths of the SVG. Colour emoji fonts draw in colour there,
  and emoji the document maps to images are those images, as in other text. Text on a path,
  vertical text and text that an XML entity expands to fail with `INVALID_IMAGE`, as all SVG
  text did. An SVG without a size takes it from what it draws without its text, so the same
  with any fonts. An SVG with text in a
  document that declares no fonts fails validation with `INVALID_IMAGE`, and an SVG may hold up
  to 1,000,000 characters of text, counting every copy.
- **`<Text>` and `<Tspan>` in `<Svg>`**: a `Text` inside an `<Svg>` (or its `G`, `Defs`,
  `ClipPath` or `Marker`) is SVG text, with `x`, `y`, `dx`, `dy`, `rotate`, `textAnchor`,
  `dominantBaseline`, `letterSpacing`, `wordSpacing`, `textDecoration`, `textLength`,
  `lengthAdjust`, `direction` and the SVG paint props; a `Tspan` inside it is a span, which also
  takes `baselineShift` and `alignmentBaseline`. Its font comes from `fontFamily`, `fontWeight` and `fontStyle` on it or
  around it, and from the text style around the drawing, matched against the registered fonts
  like other text, and declared in the document. `Tspan`, `SvgTextProps` and `TspanProps` are
  exported; `SvgPresentation` gains the font props, `textAnchor`, `dominantBaseline` and
  `visibility`.
- Background images: `<ImageBackground src fit>`, a `View` with an image behind its children,
  and `backgroundImage` on a box node of the document (`src` or `data`, `fit`, `objectPosition`).
  The image fills the padding box, over the background colour and under the borders, clipped to
  it and rounded by `borderRadius`; a box split across pages is sliced with its image, like its
  background colour. `src` takes every form `<Image src>` takes, and `fit`, `objectFit`,
  `objectPosition` and the `object-*` classes place it as on an `Image`.
- Colour emoji from fonts. Glyphs in a font's `COLR` table (versions 0 and 1) and PNG bitmaps in
  `CBDT` or `sbix` are drawn in colour, as images that copy and search as their text
  (`ActualText`). Register an emoji font, such as Noto Color Emoji, and list it in
  `fallbackFonts`: an emoji goes to the first colour fallback font that has it. A font with
  colour bitmaps and no outlines now registers; it is never embedded and cannot be a form
  field's font. A colour glyph that takes over 10 000 drawing steps, or past the first 4096 of a
  render or the first 32 MB of colour glyph images, draws without colour. Under PDF/A-1b, which
  forbids transparency, a colour glyph that needs it (a bitmap with an alpha channel, a layer
  with an opacity) fails the render with `PDFA_CONFLICT` at its run, as an emoji image does at
  its `emojiImages` key, or at the `src` of an SVG whose text has it.
- Emoji from images: `Font.registerEmojiSource({ url, format, withVariationSelectors })` or
  `{ builder }` draws the emoji of every `Text` from an image set, through the document's new
  `emojiImages` (emoji sequence → image id). The images load like `<Image src>` (URLs allowed by
  `remoteImages`, kept between renders); an emoji image is 1em high with its aspect ratio and
  copies as the emoji. An emoji whose image cannot be loaded, or is not an image the engine can
  draw, is drawn by the fonts with a warning, and a ZWJ sequence without an image uses those of
  its parts. `EmojiSource` is exported.
- Images in text: an `<Image>` inside a `<Text>` (and `image` on a document's text run) sits in
  the line, as wide and high as asked in points or em (1em high by default, keeping its aspect
  ratio), placed by `verticalAlign` (`baseline`, `middle`, `text-top`, `text-bottom`, `top`,
  `bottom`, `super`, `sub` or points; `align-text-top` and `align-text-bottom` in Tailwind), with
  `alt` as the text it copies as. Lines break around it like around a character.
- Groups of form fields: a dot in a field `name` puts the field in a group, as in PDF's own
  field names. `address.street` and `address.city` are the fields `street` and `city` of the
  group `address`; a name has at most 32 parts, so groups nest up to 31 deep. The PDF holds
  them as a field tree, which viewers show and export by their full names.
- List boxes: a form field of `kind: "list"` shows several of its `options` at once (four rows
  without a height) and scrolls to the first selected one. With `multiSelect`, `value` is an
  array and several options can be selected.
- Formats on text fields: `format` shows a number (decimals, separators, negative style,
  currency), a percent, a date or a time (with a pattern such as `dd/mm/yyyy`), a postal code, a
  phone number or a social security number, and viewers format what the user types with the
  standard form scripts. PDF/A output keeps the formatted text and value but leaves the scripts
  out, as PDF/A requires.
- Text notes: a `note` node is a sticky-note icon that viewers open to show its text, with an
  author, a colour and whether it starts open. It takes no space; the icon sits at the top-left
  corner of its box, keeps its size at any zoom and stays upright under a transform. It has an
  appearance stream, so it prints and passes PDF/A-2b. A note can also be part of a line of text
  (`note` on a text run): it takes no room there, and its icon hangs from the top of the line
  where it is. Line breaking keeps it with the word before it.
- `<FieldSet name>` in `@inkoxide/render`: a `View`, with its props and `render`, whose name goes
  in front of the names of the fields inside it, at any depth (`address.street`); sets nest.
- `<List>` in `@inkoxide/render`, a list box, with `multiSelect` and an array `value`; `Field`
  takes `kind="list"`. `Select` and `List` take their choices as `options` or `select`, and
  `TextInput` is another name for `TextField`.
- `format` on `TextField` in `@inkoxide/render`, with the readable keys of the schema, or `param`,
  `nDec`, `sepComma`, `negStyle` and `currencyPrepend`; the type `TextFormat` is exported. A
  number or percent format with any of those keys defaults to no decimals and no thousands
  separator; on a `time`, `param` is one of four styles, `0` to `3`; `zip4` is `zipPlus4`.
- `<Note>` in `@inkoxide/render`: a text note, with its text as children, `author`, `open`, and
  its `backgroundColor` as its colour. Inside a `<Text>`, it is part of the line.

### Changed

- Emoji sequences (flags, keycaps, skin tones, tag sequences and ZWJ sequences such as `👩‍💻`) stay
  in one font, so the font's ligature forms: their selectors, joiners and modifiers could go to
  another font than their base, which drew them as separate glyphs.
- Strokes with mitred joins in a `<Canvas>` have a miter limit of 10, PDF's default, as in
  pdfkit; they had SVG's 4, so joins sharper than about 29 degrees (down to about 11) are now
  mitred instead of bevelled. `miterLimit(4)` gives the old joins.
- In SVG images, `em` and `ex` lengths and `font-size` keywords are relative to a font size of
  16 where nothing sets one, as in browsers; it was 12.
- Font family names match ignoring ASCII case, as in CSS: `fontFamily: 'inter'` picks Inter
  instead of falling back with a warning. `Font.register` refuses a family that differs from
  a registered or bundled one only in case, for the same weight and style.

### Security

- The text of SVG images is bounded as the rest of the SVG is: what replaces it for the SVG
  conversion counts toward the SVG limits as it is drawn, before it is written, and validation
  checks it as a render does, so an SVG over a limit fails validation with `INVALID_IMAGE`.
  The text nodes of an SVG text count as elements. Reading, laying out and drawing SVG text
  takes time in proportion to it.
- Field options and values are limited to 1000 options of at most 1000 characters each, and a
  `value` of at most 8000 characters. Longer ones are `INVALID_VALUE`.

## 0.6.5 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.5; the other packages are unchanged. Upgrading: text that
shapes to millions of glyphs more than it has bytes now fails with `FONT_ERROR` (see
**Security**).

### Security

- Text is held to a limit on the glyphs shaping makes beyond one per byte of text: about
  4 million over a render. Over it, the render fails with `FONT_ERROR` naming the font. Text
  shapes to about one glyph per byte or fewer, but a font whose layout tables substitute one
  glyph with thousands made a few kilobytes of text take gigabytes of memory. Those glyphs
  also count toward the limit on what is painted on every page, which counted characters only.

## 0.6.4 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.4; the other packages are unchanged. Upgrading: a form
field taller than a page is one widget, on its first page (see **Fixed**).

### Fixed

- A form field taller than a page is one widget, on the page it starts on: it was a widget on
  every page it was sliced across, each embedding the field's editing glyphs again, so a single
  field could make tens of thousands of widgets and take seconds to render. The glyphs for
  editing a text or select field are now added once per font. Found by the `render_tree` fuzz
  target.

## 0.6.3 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.3; the other packages are unchanged. Upgrading: CFF fonts
with a number out of range in a DICT are rejected when they are registered (see **Fixed**).

### Fixed

- A CFF font with a number in its Top, Font or Private DICT that overflows a 32-bit float (or
  beyond 10³⁰) is rejected with `FONT_ERROR` when it is registered. Registering it panicked in
  the font subsetter: a panic in Rust, an `INTERNAL` error in Node. Found by the `register_font`
  fuzz target.

## 0.6.2 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.2; the other packages are unchanged. Upgrading: the limit
on what is painted on every page is lower (see **Changed**).

### Changed

- The limit on what is painted on every page is about 64 MB instead of 256 MB: a document just
  under 256 MB made a PDF that size and took seconds to render.

## 0.6.1 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.1; the other packages are unchanged. Upgrading: a
document whose headers, footers and fixed elements would add more than about 256 MB repeated on
every page now fails with `LAYOUT_ERROR` (see **Security**).

### Security

- What is painted on every page (headers, footers, overlays, fixed elements and repeated table
  header rows) is held to a limit of about 256 MB over the whole document, estimated before
  anything is painted; a document over it fails with `LAYOUT_ERROR`. Repeated on every one of
  thousands of pages, a few kilobytes of content (a fixed element taller than its page, links
  on every line) made a PDF of several gigabytes. Elements shown on some pages only (`pages`)
  count for those pages. Found by the `render_tree` fuzz target.

## 0.6.0 - 2026-10-13

`inkoxide` and `@inkoxide/render` 0.6.0, `@inkoxide/react` and `@inkoxide/tailwind` 0.4.0,
`@inkoxide/style` 0.2.0 and `@inkoxide/schema` 2.3.0. Upgrading: validation is stricter where
the engine used to ignore a value silently (forced page breaks where they do nothing, styles a
table cell does not apply, `opacity` outside 0–1, out-of-range `threads` and
`compressionLevel`, fonts with malformed layout tables), and a `remoteImages.allow` URL entry
now matches by origin and path (see **Changed**). The names listed under **Deprecated** still
work.

### Added

- Document metadata `producer`, `pageMode` and `pageLayout` (`meta` in the document, props of
  `<Document>`): the PDF producer (default `Inkoxide <version>`), what the viewer shows when it
  opens the PDF (no panel, the bookmarks, thumbnails or full screen; by default the bookmarks
  when there are any, as before), and how it lays out the pages (one or two pages, or one or two
  scrolling columns).
- `inkoxide` (Rust): `validate_without_images`, which validates a document without looking up
  its image ids, as the Node `validate` does when it is given no `images`.

### Changed

- A URL entry in `remoteImages.allow` is matched by origin and path instead of as a string
  prefix: it allows URLs with the same scheme, host and port, and, when it has a path, that path
  and the paths under it (`https://cdn.example.com/img` allows `/img/logo.png`, not
  `/imgs/logo.png`). Entries such as `https://cdn.example.com/` work as before.
- The `inkoxide` CLI's `validate` refuses `--full-fonts` and `--hyphenation`, which it accepted
  and ignored; `render` and `bench` take them as before.
- The published `@inkoxide/tailwind` no longer declares `"sideEffects": false`: importing it
  loads `@tailwindcss/node`, which registers Node module resolution hooks.
- The `@inkoxide/tailwind` README documents `DEFAULT_THEME_CSS`, every `initTailwind` option,
  `setTailwindStrict`, `tw.warm` and `tw.cacheSize`.
- Lengths in `style` and in classes take the same syntax, read by one reader in
  `@inkoxide/style`. `style` now takes `calc()` (`'calc(10mm + 1em)'`), units in any case
  (`'5PX'`) and exponents (`'1e1pt'`), and `em`, `vw` and `vh` in a `transform` or
  `transformOrigin` string. Classes now take `em`, `vw` and `vh` (`w-[2em]`, `h-[10vh]`; `tw()`
  takes the font and page sizes as a second argument) and a space before the unit
  (`w-[5_pt]`). A number too large for a float (`'1e999pt'`) is an error in both, as is a bare
  `infinity` (`w-[infinity]`), which CSS takes only inside `calc()`, and `style` length errors
  name the property (`width: 3ex (unsupported unit "ex")`). `textDecoration` and `objectFit` in
  `style` are read as their classes are, and `text-decoration-line` in a class takes both lines
  (`underline line-through`), as `style` does.

### Deprecated

- `@inkoxide/style`: `Reader`, `ENUMS`, `EnumKey`, `evalDim`, `Dim`, `substituteVars` and
  `parseFontFamily`, which only `@inkoxide/tailwind` uses. They moved to
  `@inkoxide/style/internal`, which is for Inkoxide's own packages and not covered by semver,
  and will be removed from the main entry.
- `TwStyle` and `TwKey` (from `@inkoxide/style` and `@inkoxide/tailwind`) are renamed
  `FlatStyle` and `FlatStyleKey`: `style` props build this style too, not only classes. The old
  names stay as aliases for now.
- `@inkoxide/tailwind`: `TailwindCompiler`, which wraps Tailwind's unstable design-system API
  and which `tw()` cannot use (set a theme with `initTailwind({ css })`), and `SUPPORTED`,
  `UNSUPPORTED` and `SupportedFamily`, the data of the Tailwind support table. They will be
  removed from the package's exports.
- The default Tailwind theme's colours `ink`, `ink-muted`, `brand`, `field`, `field-line`,
  `rule` and `head` (`bg-brand`, `text-ink`, …), made for the engine's test documents, will be
  removed from it. Set them in your own theme (`initTailwind({ css })`) to keep using them.
- `@inkoxide/react`: `FontRegistry.loadDirectory` and the `file` field of `FontFace` that it
  reads. Read the font files yourself and pass their bytes to `FontRegistry.register`;
  `@inkoxide/render` now loads its bundled fonts that way.
- `@inkoxide/react`: `FontRegistry.families()`, which nothing uses.

### Fixed

- The examples in the `@inkoxide/render` README run when copied into a new project and started
  with `npx tsx`, as the README says: like the quick start, they name the JSX runtime and end in
  a plain `renderToFile` call, without a top-level await. The files in `examples/` render their
  PDF on every Node.js 22, not only on 22.18 and later, also when started through a symlinked
  path such as `/tmp` on macOS.
- `validate` decodes the images a document uses, so an image that does not decode is
  `INVALID_IMAGE` at its path. Until now only the header was read: a corrupt or truncated PNG
  passed validation and failed the render with `IMAGE_ERROR` and no path. A JPEG is still
  embedded without decoding, but its structure is checked: a JPEG cut short, or coded in a way
  PDF readers cannot show (lossless, hierarchical or arithmetic), is `INVALID_IMAGE`. The decode
  is cached, so the render that follows does not decode again.
- Node: `validate` without `images` found nothing past 100 image references. It counted the
  `UNKNOWN_IMAGE` errors it then dropped against the 100-error cap; it now skips the image
  lookups instead.
- Node: `threads` and `compressionLevel` that are not integers in range (`threads` 1–256,
  `compressionLevel` 0–9) are `INVALID_ARGUMENT`. They were converted: `threads: -1` started a
  256-thread pool, and `1.5`, `NaN` or `2 ** 32 + 3` were truncated or wrapped. An invalid
  `creationDate` is `INVALID_ARGUMENT` too, like every other bad option, not the document code
  `INVALID_VALUE`. `@inkoxide/render` reports a bad `threads` or `creationDate` as
  `ENGINE_REJECTED` (400) at the option's path (`options.threads`); `threads: 0` was an
  `INTERNAL` error (500).
- `@inkoxide/render`: `imageTimeoutMs`, `remoteImages.timeoutMs` and the `maxBytes` of
  `remoteImages` and `localImages` must be numbers from 0 (`Infinity` included); another value
  fails the render with `ENGINE_REJECTED` at its path before any image loads. A NaN or negative
  timeout failed every image at once, and a NaN `maxBytes` lifted the limit. A fractional
  timeout is rounded up to a whole millisecond: `remoteImages.timeoutMs: 1.5` failed every
  remote image with a `RangeError` message.
- `@inkoxide/render`: a remote image answered with a 3xx status that is not a redirect (`304`)
  fails with `HTTP 304`, not `HTTP 304, and redirects are not followed`.
- Node: `InkoxideError` no longer has an undocumented `detail` property (the same text as
  `message`).
- Rust: the `RenderOptions::threads` docs state that a pool has at most 256 threads and that
  `Some(0)` renders on the calling thread, as `Some(1)` does.
- A transform function with two keys or none (`{ translate: [1, 2], rotate: 5 }`, `{}`), and an
  array with more items than its fixed length (a five-item colour, a three-item
  `transformOrigin`), are `INVALID_VALUE` at their path. They were `PARSE_ERROR` ("not
  well-formed JSON") with no path.
- JSON and MessagePack report a bad `version` alike: a number other than 1 (`2`, `2.5`) is
  `UNSUPPORTED_VERSION`, and a `version` that is not a number (`"1"`, `true`) is
  `INVALID_TYPE`. JSON reported `"1"` as an unsupported version, and MessagePack reported `2.5`
  as a wrong type.
- Validation follows the schema more closely (see the
  [schema changelog](packages/schema/CHANGELOG.md)): an integer past the engine's integer type is
  `OUT_OF_RANGE`, not `INVALID_VALUE`; `".5%"`, `"5.%"`, `".5fr"` and `"-0fr"` are rejected, as
  the schema rejects them; a `"0fr"` table column is `OUT_OF_RANGE`; and an empty or too long
  `link` or bookmark `title` is `INVALID_VALUE`, like an id of the wrong length.
- `@inkoxide/schema` types: a transform function or a grid track object takes the keys of one
  form only, so `{ translate: [1, 2], rotate: 5 }` no longer type-checks.
- The `inkoxide` CLI printed a validation error's code twice (`UNKNOWN_FONT UNKNOWN_FONT at …`).
- A `PageBreak`, `break="page-before"` or `break="page-after"` inside a `View` that fits on the
  page starts a new page; it was ignored. It also breaks inside `break="avoid"`
  (`wrap={false}`), as in CSS, and a break before a `View`'s first child takes the `View` to the
  new page instead of leaving its top padding behind. Inside a `flex-row`, grid, wrapping or
  reversed `View`, and inside an absolutely positioned, rotated or transformed element, where it
  did nothing, it is now an error, as is `break="page-before"` or `break="page-after"` on an
  absolutely positioned element.
- A page break at the end of the content, or one with nothing above it on its page (at the top
  of the first page, below a top margin), no longer adds a blank page.
- An absolutely positioned element of the page body written after a page break is placed from
  the top of the body, like one written first; it moved with the break and could be cut across
  two pages.
- A `flex-row`, grid or wrapping `View` taller than a page splits between the lines of its
  children, each column continuing on the next page, and a `break="avoid"` element taller than
  a page splits like any other; both were cut through a line of text at the page edge.
- Fixed elements in the body are painted in document order and by `zIndex`, as without `fixed`;
  nested ones were painted in the wrong order.
- A table cell's `height` is a minimum, as in CSS: the row grows to fit the cell's content.
  Until now it fixed the row's height, and taller content ran over the next row.
- Styles a table cell ignored are validation errors (`STYLE_NOT_APPLICABLE`), like `transform`
  and `zIndex` already were: `width`, `minWidth`, `maxWidth`, `maxHeight`, `margin`, `position`,
  `inset`, `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gridRow`,
  `gridColumn`, `gridArea`, `overflow`, `opacity` and `rotate`. They passed validation and had no
  effect, or, for `minWidth` and `maxWidth`, changed the width of the cell's text but not of its
  column, so the text ran into the next cell or wrapped in a narrow strip.
- Collapsed table borders (the default `borderCollapse`) keep their `borderStyle`: dashed and
  dotted cell borders were drawn solid. Where two borders of equal width meet, solid wins over
  dashed and dashed over dotted, as in CSS.
- A box whose first child has `minPresenceAhead` or `break.before: "page"` (or is a
  `PageBreak`) moves to the next page with that child, as CSS carries a break up to the parent.
  It left its empty top (padding and background) at the foot of the page before.
- A table whose first row is taller than a page starts on the current page and splits that row,
  as it already did for later rows. It moved whole to a new page, leaving the rest of the page
  empty.
- Percentage heights of a `Page`'s children (`height: '50%'`, `minHeight`, `maxHeight`) are of
  the page content height. They behaved as `auto`.
- Text collapses only spaces, tabs and line breaks, as in CSS. Thin, em, figure, ideographic and
  other space characters keep their own width, and the non-breaking ones no longer allow a line
  break.
- Tabs in `pre` and `pre-wrap` text advance to the next tab stop, as CSS `tab-size: 8`: stops are
  8 spaces of the font apart from the start of the line, so tab-separated columns line up in any
  font. Each tab was four spaces.
- A box sized to the min-content width of hyphenated text leaves room for the hyphen; the
  hyphen was drawn outside it.
- The docs say how `minPresenceAhead`, percentage gaps and relative `top`/`left` offsets work.
- A creation date with an offset under an hour west of UTC (`-00:30`) is written with its sign
  in the PDF information dictionary; it was written east of UTC, an hour off the XMP date that
  PDF/A requires it to match.
- Registering a font collection (`.ttc`) fails with `FONT_ERROR`. It was accepted, only its first
  face could be used, and with `subsetFonts: false` the whole collection was embedded where the
  PDF needs a single TrueType program.
- An SVG image whose width or height is under 0.00001 px, or whose `viewBox` width or height
  is, fails with `INVALID_IMAGE` at its path (a nested SVG image is left out). Scaling it up
  made the PDF writer output `inf`, or numbers too large for PDF readers, in the image's matrix.
- Huge but finite numbers in an SVG image (`transform="scale(1e30)"`, a gradient, mask,
  pattern or shape at 1e30) no longer make an invalid PDF. The PDF writer wrote them as digits
  only, integers far past the 32 bits PDF readers parse, which they treat as errors or null.
  Every number the writer emits, in page content, SVG drawings, annotations and dictionaries,
  is now at most ±10¹¹.
- `borderRadius` rounds an image's corners without `overflow: hidden`: the image is clipped to
  the rounded content box, as in CSS.
- A select field embeds the glyphs of the common Latin range and of every option, as a text
  field does, so a viewer can show the option picked; it embedded only those of its value.
- The mark of a checkbox or radio button takes its size from `size`, centred in the widget; it
  grew with the widget box.
- A checkbox or radio button without a width or height keeps its `size` + 2 pt square where its
  container would stretch it (as in a column, by default); it stretched across the container.
- A radio button whose value is `Off`, or longer than 127 bytes, is no longer written with a
  broken appearance state: such a group names its states by position and lists its values in
  `/Opt`. A checked `Off` button showed as unselected.
- Links and form fields inside a box with `transform` or `rotate` are clickable where the box is
  drawn: their areas follow the transform, and a field's appearance turns and scales with it.
  They stayed at the untransformed position.
- An SVG with a `viewBox` drawn with `fit: fill` (the default) into a box of another shape keeps
  its proportions, as in CSS: the box becomes its viewport, and its `preserveAspectRatio`
  (centred and whole by default) places the drawing. It was stretched to the box, so an `<Svg>`
  given `width: '100%'` came out squashed.
- An SVG element with a `filter` (a drop shadow, a blur) is drawn without the filter; it was left
  out with its children, so a logo with a shadow showed as a blank box.
- SVG gradients with `spreadMethod="repeat"` or `"reflect"` repeat or mirror their stops over
  what they paint; they were drawn as `pad`, one ramp and then a flat colour.
- GIF and WebP images embedded in an SVG as `data:` URLs are drawn; they were left out.
- An SVG fails with `INVALID_IMAGE` when an SVG image nested in it has text, as it does with text
  of its own, and when an image embedded in it is over 50 megapixels; both were left out
  silently. The guide lists what SVG drawing does not support.
- **Grid placement:** a `gridColumn` or `gridRow` with `start`, `end` and `span` runs from
  `start` to `end`, as in CSS; the engine used `start` and `span` and ignored `end`. In Tailwind,
  a span after the slash is kept (`col-[2/span_3]` was one column at line 2),
  `col-span-3 col-start-1 col-end-3` is 1 / 3, not three columns, and a span at the end is
  ignored after one at the start (`col-start-[span_2] col-end-[span_3]` is two columns).
- **Tailwind themes:** `initTailwind({ css })` run alongside `init()` or another `initTailwind()`
  keeps its theme; whichever load finished last used to win, so the default theme could replace
  it.
- **`flex`:** a lone length is the basis, as in CSS: `flex: '30%'` and `flex-[30%]` are
  `1 1 30%`. The style prop rejected it and the class read it as a grow of 0.3. Grow and shrink
  with a unit are an unsupported value.
- **Reading values:** an unsupported Tailwind value always names the CSS property
  (`leading-[abc]`, `rotate-[abc]`, `p-[1pt_2pt_3pt_4pt_5pt]`, `m-[var(--nope)]`), and
  `parseTransform` throws `UnsupportedValueError` for a bad angle (`rotate(1grad)`).
  `rotate-none` works, and lengths above 10000pt are no longer cut to 10000pt (`h-[12000pt]`).
- **Tailwind mapping:** a class or style value outside what the engine takes fails where it is
  read, naming the class: `line-clamp-0`, `line-clamp-[2.5]`, `opacity-[1.5]`, `col-span-0`,
  `aspect-[0]`, a skew of ±90deg or more, and a percentage or negative grow or shrink
  (`grow-[30%]`, `shrink-[-1]`), which CSS rejects. `grid-flow-dense` works. Switching strict
  mode on checks class strings cached while it was off. Only selector problems that style
  children suggest `gap`; `container` says that media queries have no meaning in a PDF.
- **Style values:** `transformOrigin: 'center left'` and `'center right'` work, and names such
  as `toString` are not taken as keywords by `transformOrigin` or `fontWeight`. Colour keywords
  are case-insensitive (`'Transparent'`).
- **Docs:** the Fonts section and the Tailwind support table say that `font-serif` is Noto Sans,
  since no serif face is bundled, and the `@inkoxide/schema` README's ajv example type-checks
  with `module: nodenext`.
- A component that suspends (`use()` of a promise, a `lazy` component) fails the render with a
  template error that names it (a `lazy` component by its `displayName`, a `render` prop by its
  element), or the `<Suspense>` boundary it is in, and says that templates cannot wait for data.
  Without a boundary the error blamed a missing `<Document>`; with one, the PDF showed the
  fallback and the process crashed shortly afterwards.
- A later `border`, `borderStyle` or `borderRadius` for every side, in a style array or over
  `className`, replaces the per-side styles and per-corner radii set before it, and a per-corner
  `borderRadius` object keeps the corners it does not name instead of setting them to 0.
- `@inkoxide/react` errors for a style key an element does not take (`maxLines` on a `View`,
  `padding` on a `Row` or an inline `Text`) have kind `'props'` and name the style when the key
  comes from `style`; only a key from `className` is kind `'tailwind'`, with the class. A string
  directly in a `Table`, `Row` or `Document` says where it belongs (a `Cell`, a `Page`) instead
  of suggesting a `Text`, which is not allowed there either. The remote-image error of
  `@inkoxide/react` names its own option, `remoteImages.allow`, and only `@inkoxide/render` adds
  the `INKOXIDE_REMOTE_IMAGES` variable, for every refused URL or request.
- Percentage padding on a `Page` (`style={{ padding: '10%' }}`, `p-[10%]`) is a share of the
  page width on every side, as in CSS, instead of being dropped, which also left the header and
  footer unaligned. A percentage minimum height on a `Row` (`min-h-[10%]`), which has no table
  height to resolve against, is a template error instead of being dropped.
- A `Text` or `Link` inside a `Text` with `id`, `bookmark`, `pages`, `render`, `dir`, `orphans`,
  `widows`, `break`, `wrap={false}` or `minPresenceAhead` is a template error that names the
  prop; these need a paragraph of their own and were dropped. A run in another `lang` inside a
  hyphenated paragraph is no longer hyphenated with the paragraph's patterns.
- `textTransform` with a `lang` that JavaScript does not take as a language tag (`en_US`) cases
  the text by the default rules instead of failing with an internal error.
- `truncate`, `line-clamp-*`, `maxLines` and `ellipsis` on a `Page`, `Row` or `Spacer` are a
  template error, as on other elements, instead of doing nothing; a negative numeric `flex`
  (`style={{ flex: -1 }}`) is rejected where the style is read, like `flex: '-1'`, and so is a
  negative or non-numeric `flexGrow` or `flexShrink`.
- `ClipPath` and `Defs` have exported props types, `ClipPathProps` and `DefsProps` (also from
  `@inkoxide/render`), and `ClipPath` takes `clipPathUnits`.
- `@inkoxide/render`: a remote image that times out or drops its connection after the headers
  fails the render with `IMAGE_FETCH_FAILED`, as one that fails before them does, instead of a
  raw `TimeoutError` or `TypeError`. `remoteImages.timeoutMs` takes `Infinity` (no limit) and
  values past what a timer holds, as `imageTimeoutMs` does, instead of throwing a `RangeError`.
  A redirect from an allowed URL, still not followed, fails with
  `HTTP 302, and redirects are not followed` instead of `fetch failed`, and the guide and
  SECURITY.md say so.
- `@inkoxide/render`: `Font.register` fetches a URL with a 30-second limit for the whole
  download and a 50 MiB size limit, through the same download code as remote images. A font
  host that never answered held up every render in the process for up to five minutes, or for
  good when it sent bytes slowly; now the render that loads the font fails with `INVALID_FONT`
  (`timed out after 30000 ms`). The guide lists `http:` URLs, which were already fetched.
- `@inkoxide/render/templates`: `renderTree` called before `init()` has finished, or while fonts
  added with `Font.register` are not loaded yet, throws an `INTERNAL` RenderError that says to
  call `await init()`, instead of failing with `No fonts are registered`, pointing to functions
  the package does not export, or building a tree without those fonts. `renderSettled` runs
  `init()` itself. The guide says that `renderTree` needs `init()` first.
- `@inkoxide/render/templates`: `renderDocument` takes images from the `images` option as well as
  from its `images` argument, which it ignored. `generate` resolves with `images`, the images the
  document was rendered with by id (and `renderSettled` with the same), so a stored
  `result.document` whose images were bytes, files, URLs or functions renders again with
  `renderDocument(result.document, result.images)`; until now the ids the tree made up for them
  (`image-0`) failed with `UNKNOWN_IMAGE`.
- `Font.register` tells faces apart by family, weight and style, and compares byte sources by
  their bytes. It throws for a face the bundled fonts have (`Inter` 400 from another file was
  accepted and never used), for a family whose font id another family has (`Brand-Two` after
  `Brand Two` was skipped without a word), for a different font of the same byte length (was
  ignored), and for a `src` that is not a path, a URL or bytes (`null` failed the next render
  with a `TypeError`). A registration that throws adds none of its faces. The guide gives font
  weights as 1–1000, the range the code accepts.
- `@inkoxide/render`: `renderToBuffer(null)`, or a string or a number in place of an element,
  fails with a `TEMPLATE_ERROR` RenderError instead of a `TypeError`.
- `@inkoxide/render/templates`: `generate`'s `timings.engineMs` no longer includes the tree
  rebuilds for the page count, and `treeMs` counts each rebuild once, so the stages add up.
  `renderSettled` resolves with that rebuild time as `treeMs`. `GenerateOptions` is the same
  type as `RenderToBufferOptions`.
- `@inkoxide/render`: the start-up error for missing fonts says what `INKOXIDE_ASSETS_DIR` must
  point to, and the guide's FAQ explains when to set it (a bundled deployment).
- A font whose `GSUB` or `GPOS` lookups point to a missing or malformed coverage table is
  rejected with `FONT_ERROR` when it is registered. It registered, and the first render that
  shaped text with it panicked in the shaper: a panic in Rust, an `INTERNAL` error in Node. The
  Rust `render` now returns any panic as an error: `FONT_ERROR` naming the font when it happens
  while shaping, `INTERNAL` otherwise.

## 0.5.0 - 2026-10-12

`inkoxide` 0.5.0, `@inkoxide/render` 0.5.0, `@inkoxide/react` and `@inkoxide/tailwind` 0.3.0, and
the new `@inkoxide/style` 0.1.0; `@inkoxide/schema` stays 2.2.0. Upgrading: Tailwind is optional,
so if your templates use `className`, run `npm install @inkoxide/tailwind` (see **Changed**).

### Added

- `@inkoxide/style`, the style values that `@inkoxide/react` and `@inkoxide/tailwind` share: the
  readers of CSS lengths, colours, `flex`, transforms, transform origins and object positions,
  and the style object type. They moved out of `@inkoxide/tailwind`, which still exports the same
  names.

### Changed

- Page numbers in the page body (`<PageNumber />` and `<TotalPages />` in a `Text`) are laid out
  with their own digits, as in headers and footers. The text wraps, justifies and sizes its box
  at the width of the numbers of the page it lands on, and the page breaks are worked out again
  when those numbers move it. Until now the body kept room for a three-digit number: a box
  sized by its text was too wide for a short number, and a number of four digits or more could
  overflow it. Text the body repeats on several pages (a table's header rows) still keeps that
  room, since each page draws its own digits in it.
- **Tailwind is optional.** `@inkoxide/render` and `@inkoxide/react` no longer install
  `@inkoxide/tailwind`: they load it when it is installed, for `className`. Templates styled with
  `style` alone need nothing more, and the install is about 10 MB smaller (no Tailwind compiler,
  Lightning CSS or PostCSS). **Upgrading:** if you use `className`, run
  `npm install @inkoxide/tailwind`; without it, a `className` fails with a template error that
  says so. `renderToTree` in `@inkoxide/react` loads Tailwind itself, as `@inkoxide/render`
  does; before the synchronous `renderTemplate`, call the new `loadTailwind()` once.

## 0.4.1 - 2026-10-12

`inkoxide` 0.4.1, `@inkoxide/tailwind` 0.2.2, `@inkoxide/react` 0.2.2 and `@inkoxide/render`
0.4.0; `@inkoxide/schema` stays 2.2.0. Upgrading: `@inkoxide/render` exports only what its
guide documents, and the template helpers moved to `@inkoxide/render/templates` (see
**Changed**). SVG styles are held to a new limit (see **Security**).

### Added

- `@inkoxide/render/templates`, a second entry of `@inkoxide/render` for serving templates by
  id with a zod schema for their data: `generate`, `parseData`, `renderTree`, `renderSettled`,
  `renderDocument` and the types `TemplateLike`, `GenerateOptions`, `RenderOpts`,
  `RenderResult`, `Timings` and `TreeResult`. The guide documents it under
  [Serving templates](packages/render/GUIDE.md#serving-templates).
- The `@inkoxide/render` guide documents the rest of what the package exports: `usePage`,
  `Field`, `version`, `engineStatus` and `countPages` (health checks), the `RenderError` fields
  `className`, `engineCode` and `issues` and its JSON form, the exported types, the `Document`
  props `creator`, `creationDate` and `fallbackFonts`, and `pages` on elements of the page
  body, with negative page numbers counting from the end.
- `@inkoxide/tailwind` exports `UnsupportedValueError`, which `parseTransform`, `parseOrigin` and
  `parseObjectPosition` throw for a value the engine cannot express.
- `@inkoxide/tailwind` exports the CSS value helpers that `@inkoxide/react` shares with it:
  `PT_PER_UNIT`, `parseFlex`, `round`, `splitTopLevel`, `BORDER_STYLES` and `OBJECT_FITS`.
- `@inkoxide/react` exports `imageSource` and its `ImageSource` type, `remoteRefused` and
  `innermostComponent`, which `@inkoxide/render` uses, and takes the tree option
  `remoteImagesElsewhere`, another place the caller reads the remote allowlist from, which the
  error for a refused URL names. `HOST_NAME` is typed by host type
  (`Record<HostType, string>`).

### Changed

- **`@inkoxide/render` exports only what its guide documents**, a list now pinned by a test.
  This breaks only code that used undocumented names:
  - `generate`, `parseData`, `renderTree`, `renderSettled`, `renderDocument`, `TemplateLike`,
    `GenerateOptions`, `RenderOpts`, `RenderResult` and `Timings` moved to
    `@inkoxide/render/templates`.
  - `renderTemplate`, `renderToTree`, `assertValidTree`, `validateTree`, `SchemaProblem`,
    `TemplateError`, `TemplateErrorKind`, `fonts`, `FontRegistry`, `FontFace`, `ResolvedFont`,
    `HOST`, `HOST_NAME`, `HostType`, `isImagePath`, `remoteAllowed`, `RemoteImagePolicy`,
    `PendingImage`, `parsePath`, `fontWeightValue`, `TreeOptions`, `TreeResult` and `Located`
    are no longer re-exported; import them from `@inkoxide/react` (`TreeResult` is also in
    `@inkoxide/render/templates`).
  - `SCHEMA_VERSION` is no longer re-exported; import it from `@inkoxide/schema`, or read
    `version().schema`.
  - `initEngine`, `assetsDir`, `DEFAULT_FACES` and `RenderErrorInit` are no longer exported:
    `init()` starts the engine, and `INKOXIDE_ASSETS_DIR` still sets where the bundled fonts
    are read from.
  - The deprecated aliases `StudioError` and `StudioErrorCode` are removed; use `RenderError`
    and `RenderErrorCode`.
- The `flex` shorthand reads the same in `style` and in Tailwind classes:
  `style={{ flex: '1 auto' }}` sets grow and basis, as `flex-[1_auto]` does, and
  `flex-[initial]` works as `flex: 'initial'` does. A lone grow in a class is rounded to six
  decimals like every other value.
  An empty `flex` (`''`) is an error instead of a grow of 0, and a negative grow or shrink
  (`flex-[-1]`) is an unsupported value where the class or style is read, not a schema error
  later.

### Security

- SVG style attributes and sheets are held to a limit on what the CSS parser does with every
  unitless number and `#` as well as every declaration: it scans the text from its start each
  time, so a style with a long `stroke-dasharray` used seconds to minutes of CPU. Found by a
  new structure-aware SVG fuzz target (`render_svg_tree`).

### Fixed

- A table with `break: { fixed: true }` in the flow repeats whole on every page: its row and
  cell backgrounds and borders were painted only on its first page, and the later pages showed
  only the cells' content.
- A table with `headerRows` that starts at the top of a page no longer paints its header twice
  on that page (a translucent header background came out darker, and text extraction read the
  header twice).

## 0.4.0 - 2026-10-12

`inkoxide` 0.4.0, `@inkoxide/schema` 2.2.0, `@inkoxide/tailwind` 0.2.1, `@inkoxide/react` 0.2.1
and `@inkoxide/render` 0.3.0. Upgrading: documents with an empty or over-long id, an empty font
`family`, a grid `span` of 0 or no pages now fail validation, and hyphenation patterns
registered for a region (`en-US`) no longer serve other English text (see **Changed**).

### Security

- SVG images are held to their limits as the SVG converter expands them: nested SVG images and
  every copy of them, markers at every vertex of every shape, references however they are
  written, style sheet rules and the declarations they copy, inherited attributes and embedded
  raster pixels all count. Entities that expand to more than 1 MiB, `href` chains that loop and
  radii or path coordinates beyond 10⁷ are refused. A small SVG could otherwise use minutes of
  CPU and gigabytes of memory. Each raster image in an SVG is embedded in the PDF once.

### Fixed

- `threads: 1` runs the whole render on the calling thread (in Node, the libuv thread that
  runs it), as documented. Every one-thread render used to queue for the same worker thread,
  so concurrent renders ran one at a time.
- Every failure of `renderToBuffer`, `renderToStream`, `renderToFile` and `init` is a
  `RenderError`. An engine that cannot start and a PDF that `renderToFile` cannot write are
  `INTERNAL`; a `render` prop that throws on a later pass is `TEMPLATE_ERROR`. The guide's error
  table lists `INTERNAL`, and gives `TEMPLATE_ERROR` as the code of an invalid prop.
- `Font.register` reads a `file:` URL given as a string (`'file:///fonts/Lora.ttf'`), not only
  as a `URL`.
- A font from `Font.register` that cannot be loaded no longer keeps the fonts registered with
  it from loading. The render still fails with `INVALID_FONT`, and the next one can use them.
- `inkoxide` no longer exports its `__testPanic` test hook, which was never public API.
- `<Svg fixed>` and `<Svg debug>` no longer write `fixed` and `debug` attributes into the
  drawing.
- The `@inkoxide/react` renderer gives React its real version, from its `package.json`, instead
  of `0.1.0`.
- An image over the 50-megapixel limit fails validation with `IMAGE_TOO_LARGE`, as documented.
  It was reported as `INVALID_IMAGE`.
- A document with `pages: []` fails validation with `INVALID_VALUE` at `pages`, as the schema
  requires at least one page. It rendered a PDF with no pages, which viewers reject.
- Validation enforces the remaining bounds of the schema: an id (of a node, row, cell, font or
  image, or a font named by a run, a field or `fallbackFonts`) is 1 to 256 characters and a
  font `family` is not empty (`INVALID_VALUE`), and a grid `span` is at least 1
  (`OUT_OF_RANGE`; `span: 0` was laid out as 1). The schema states the engine's bounds of grid
  lines: within ±32767 and not 0, and a `span` of at most 65535.
- The documentation states that `validate` returns at most 100 problems, and only the first
  when the document's structure cannot be read. The schema, `SCHEMA.md` and `API.md` describe
  the rules the engine applies beyond the schema's shapes: `break` and `pageBreak` in headers,
  footers and table cells, margins that leave no room for content, and repeated form field
  names.

### Changed

- Hyphenation patterns are registered for the whole language tag. Patterns for `en-GB` no longer
  replace those for `en-US`; text uses the patterns of its own tag, else of the nearest less
  specific one (`en-GB`, then `en`), else the built-in ones. Three-letter languages such as
  `haw` or `fil` can be registered. **Patterns registered for a region now serve only that
  region:** register for `en` (not `en-US`) to cover every English `lang`. `@inkoxide/render`
  registers its US English patterns for `en`, so they still serve every English `lang`;
  `engineStatus().hyphenation` lists them as `en`.

### Removed

- `inkoxide::debug_raw_parse_error`, a hidden test hook that was never part of the API. The
  schema-parity check that used it is now a unit test inside the crate.

### Repository

- [ARCHITECTURE.md](docs/ARCHITECTURE.md) lists every public item of the crate, and places
  parsing in `validate`, where it lives.

## 0.3.0 - 2026-10-12

`inkoxide` 0.3.0, `@inkoxide/schema` 2.1.0, `@inkoxide/tailwind` 0.2.0, `@inkoxide/react` 0.2.0
and `@inkoxide/render` 0.2.0.

### Added

- **`<Link>`**: a link as a component, next to `href` on `Text` and `View`. Inside a `Text` it
  is a linked run, elsewhere a box. `src` is an alias of `href`.
- **Page sizes**: the ISO A, B and C series, RA, SRA, Folio and ID-1, matched in any case;
  `size={[width, height]}`, `size={[side]}` and a single length for a square page.
- **Image sources**:
  - Files, read only from folders you allow with the `localImages` render option or
    `INKOXIDE_LOCAL_IMAGES`. Paths cannot leave those folders, through `..` or symbolic
    links, and files are capped at 20 MiB by default.
  - Requests with a method, headers and a body: `{ uri, method, headers, body }`, under the
    same allowlist and limits as URL strings.
  - Functions and promises that resolve to any source, with a timeout (`imageTimeoutMs`,
    10 s by default).
  - `ArrayBuffer` bytes.
- **Style keys**:
  - `objectFit` on images, and `textOverflow: 'ellipsis'`.
  - `textDecoration: 'line-through'` and `'underline line-through'`, which draws the
    line-through, plus `textDecorationColor`, `textDecorationStyle` and
    `textDecorationThickness`.
  - The shorthands `border`, `borderTop`, `borderRight`, `borderBottom` and `borderLeft`,
    e.g. `'1px solid #e5e7eb'`.
- **Units** in style objects: `pc`, `rem`, `em`, `vw` and `vh`.
- **Fonts**:
  - WOFF and WOFF2 files, in `Font.register`, `registerFont` and the CLI `--fonts` folders.
  - `Font.registerHyphenationCallback` and a `hyphenationCallback` prop on `Text`, to split
    words with your own function.
- **`renderToStream`**: the PDF as a Node `Readable`.
- **`fixed`**: an element placed directly in a `Page` with `position: 'absolute'` repeats on
  every page, placed against the page's content box, for watermarks, borders and page
  numbers. A negative `zIndex` puts it under the content.
- **`objectPosition`** on images, in style objects and as Tailwind `object-*` classes, places
  a fitted image in its box: keywords, percentages or lengths. `objectFit` also has its
  Tailwind classes now.
- **`debug`**: outlines the margin, border, padding and content boxes of an element and labels
  its size; on a `Page` or the `Document`, of every element. Labels are drawn, not text, so
  copying text from the PDF is unaffected.
- **`@inkoxide/schema`**: `hyphenate` on a text run, overriding its text node's setting for
  the run's words; `objectPosition` on images; `debug` on nodes, rows, cells and pages.

### Changed

- An image `src` string that names an id you pass in `images` is always that image, even when
  it looks like a URL. Such strings used to be fetched.
- `Page` `size` strings are checked in full: `"A4-landscape-x"` is an error instead of `A4`
  landscape.
- Every length, percentage and factor in a document has a range (percentages within ±10000%,
  points within ±1000000), and a value outside it is `OUT_OF_RANGE`. In React, an infinite or
  `NaN` style value is an error instead of being dropped.
- A fixed, absolutely positioned child of a page in a JSON document is placed against the
  page's content box on every page. Its insets used to be resolved against the whole flow, so
  `bottom` and percentages put it in the wrong place.

### Fixed

- Page numbers in headers and footers are laid out with their real digits, so centred and
  right-aligned numbers line up, and documents of 1,000 pages or more no longer overflow the
  space.
- A panic while registering a font or hyphenation patterns is reported as an `INTERNAL` error
  instead of ending the process.
- Huge percentages, for example in `width`, `margin` or a `translate`, no longer write `inf` or
  `NaN` into the PDF, which made it invalid. The PDF writer also never writes a non-finite
  number, from layout or from an SVG image.

## 0.2.0 - 2026-10-11

`inkoxide` 0.2.0, `@inkoxide/schema` 2.0.0, `@inkoxide/tailwind` 0.1.1, `@inkoxide/react` 0.1.1
and `@inkoxide/render` 0.1.2.

### Breaking: ES modules only

`inkoxide` 0.2.0 and `@inkoxide/schema` 2.0.0 are ES modules and need Node.js 22 or later, like
the React packages already did. Replace `require('inkoxide')` with `import … from 'inkoxide'`.
`@inkoxide/schema` 2.0.0 still describes document schema version 1; documents do not change.

### Dependency updates

`@inkoxide/tailwind` 0.1.1, `@inkoxide/react` 0.1.1 and `@inkoxide/render` 0.1.2 depend on the
releases above. The `@inkoxide/render` README now shows the project setup the quick start needs.

### Repository

- `@inkoxide/tailwind`, `@inkoxide/react` and `@inkoxide/render` are developed in this
  repository, under `packages/`, with the examples and the benchmarks.
- Continuous integration on every pull request.
- Every package is published by the `release` workflow, with npm provenance.

## @inkoxide/render 0.1.1 - 2026-10-11

- A new README with examples, and two new guides: [GUIDE.md](packages/render/GUIDE.md) and
  MIGRATING.md.
- `INKOXIDE_ASSETS_DIR` replaces `STUDIO_ASSETS_DIR` for loading the bundled fonts from another
  directory.
- `StudioError` and `StudioErrorCode` are deprecated aliases of `RenderError` and
  `RenderErrorCode`.

## 0.1.0 - 2026-10-11

First release.

- `inkoxide` 0.1.0: the engine as a Node package, with prebuilt binaries for macOS (arm64, x64),
  Linux (x64 glibc and musl, arm64 glibc) and Windows (x64).
- `@inkoxide/schema` 1.0.0: TypeScript types and the JSON Schema of document schema version 1.
- `@inkoxide/tailwind` 0.1.0, `@inkoxide/react` 0.1.0 and `@inkoxide/render` 0.1.0: React
  components to PDF, styled with Tailwind classes or style objects.
