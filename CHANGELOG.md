# Changelog

All notable changes to the packages in this repository. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and every package follows
[Semantic Versioning](https://semver.org/). The document schema has its own
[changelog](packages/schema/CHANGELOG.md).

## Unreleased

### Fixed

- An image over the 50-megapixel limit fails validation with `IMAGE_TOO_LARGE`, as documented.
  It was reported as `INVALID_IMAGE`.
- A document with `pages: []` fails validation with `INVALID_VALUE` at `pages`, as the schema
  requires at least one page. It rendered a PDF with no pages, which viewers reject.

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
