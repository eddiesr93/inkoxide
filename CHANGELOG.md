# Changelog

All notable changes to the packages in this repository. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and every package follows
[Semantic Versioning](https://semver.org/). The document schema has its own
[changelog](packages/schema/CHANGELOG.md).

## Unreleased

### Fixed

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
  `innermostComponent`, which `@inkoxide/render` uses. `HOST_NAME` is typed by host type
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
  [MIGRATING.md](packages/render/MIGRATING.md).
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
