# Changelog

All notable changes to the packages in this repository. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and every package follows
[Semantic Versioning](https://semver.org/). The document schema has its own
[changelog](packages/schema/CHANGELOG.md).

## Unreleased

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
- The docs say how `minPresenceAhead`, percentage gaps and relative `top`/`left` offsets work,
  and where they differ from react-pdf (`MIGRATING.md`).
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
