# Schema changelog

Changes to the document format. Within schema version 1, changes are backwards compatible:
documents that were valid stay valid and render the same way, except to fix behaviour that was
clearly wrong; such fixes are listed under **Fixed**. A breaking change would start schema
version 2.

## Unreleased

## 2.5.0 - 2026-10-15

Still document schema version 1. New definitions `NonNegativePercent`, `NonNegativeLength`
and `NonNegativeLengthPercent`.

### Fixed

- A character that the font of a text, select or list field has no glyph for, in the shown
  value or in an option, is `INVALID_VALUE`. Fields are drawn with their own font only, so the
  character was left out without a word; such a document was valid before.
- A second radio button with `checked` in the same group is `INVALID_VALUE`; such a group was
  valid before, with both buttons drawn on but only the first one's value as the group's.
  Buttons of a group with the same value are now on together, as viewers show them, encrypted
  or not.
- An `emojiImages` image in a run with `verticalAlign` `super` or `sub` is raised or lowered
  with the run's glyphs, its bottom 0.1em below the run's moved baseline. It stayed on the
  line's baseline.
- A page with a side under 3 points renders as PDF/A no more: the render fails with
  `PDFA_CONFLICT` at the page's `size`, at every level. Such a file was not PDF/A. The `Size`
  description says so.
- The schema and the engine agree on more values:
  - `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight`, `flexBasis`, `gap`,
    `rowGap`, `columnGap` and `padding`, percentage grid tracks and `fitContent` are not
    negative in the schema either (new `NonNegativePercent`, `NonNegativeLength` and
    `NonNegativeLengthPercent`). The engine already rejected negative ones with `OUT_OF_RANGE`.
  - An integer written with a zero fraction (`"weight": 400.0`, `"version": 1.0`,
    `"colSpan": 1.0`) is read as the integer, as JSON Schema reads it. It was `INVALID_TYPE`.
  - `"options": []` is `INVALID_VALUE` on every field kind, as the schema's `minItems` says. On
    a text or checkbox field the engine accepted it.
  - A percentage or `fr` size past its bound by digits a 32-bit float drops (`"10000.0001%"`,
    `"1000000.01fr"`) is `OUT_OF_RANGE`, as the schema's pattern says, and a negative
    percentage too small for a float stays negative. Other numbers are read as 32-bit floats,
    which `docs/SCHEMA.md` now says.
  - `TableCell.style` lists `float` and `clear` among the properties a cell does not take, and
    `float`, `Break.before` and `PageBreakNode` say that a float takes no forced page break,
    on it or inside it.

## 2.4.1 - 2026-10-15

Still document schema version 1.

### Fixed

- `fallbackFonts` holds at most 256 ids; a longer list is `OUT_OF_RANGE`. It was unbounded, so
  a document with a longer one was valid before. An id listed again is skipped: each font is
  tried once, where it is first listed.

## 2.4.0 - 2026-10-15

Still document schema version 1.

### Added

- `Style.float` (`left`, `right`, `none`) and `Style.clear` (`left`, `right`, `both`,
  `none`): floats in column boxes, the page and cells, with text wrapping beside them and boxes
  narrowed beside them. Floats go in column boxes only, not on cells, not absolutely positioned
  and without forced breaks; at most 1000 per box; a box with floats takes no `justifyContent`
  or `alignContent`, and its children no `flexGrow` or `flexBasis`.
- `Style.textWrap` on text nodes: `wrap` (the default, first fit, as before) or `pretty`, which
  sets each stretch between forced breaks as a whole (Knuth and Plass's total fit) and lets the
  spaces of justified lines shrink by up to a third. Bounded per paragraph (20 000 break
  opportunities, 512 open breakpoints, 2 097 152 lines tried), past which it breaks first fit.
- `TextRun.verticalAlign`: `baseline` (the default), `super` or `sub`. A superscript's
  baseline is raised and a subscript's lowered by the offsets the font gives (OS/2), for text
  of the run's size, and the line grows to fit them. The run keeps its own `size`.
- `TextRun.fontFeatures`: OpenType features by tag, as CSS `font-feature-settings`
  (`{"tnum": 1, "liga": 0}`): 0 turns a feature off, 1 on, and a larger value picks an
  alternate. At most 32 per run, values up to 65535, and 64 different sets in a document.
- `BoxNode.backgroundImage` (`$defs/BackgroundImage`): an image painted into the box's padding
  box, over its `backgroundColor` and under its borders and children, clipped to the padding box
  rounded by `borderRadius`. It takes `src` or `data` (exactly one, as on an `image` node), `fit`
  (default `fill`) and `objectPosition`, with the same bounds as on an `image` node. Optional;
  documents without it render as before.
- `TextRun.image` (`$defs/InlineImage`): an image in the line in place of the run's text, a run
  having exactly one of `text`, `placeholder` and `image`. It takes `src` or `data`, `width` and
  `height` in (0, 14400] points (one from the other by the image's aspect ratio, and 1em high,
  the run's `size`, with neither), `fit`, `objectPosition`, and `alt` (at most 1024 characters),
  the text copying and searching give for it. The run's `font` and `size` are the line's strut.
  It breaks lines like one character.
- `Document.emojiImages`: emoji sequences drawn as images, sequence → image id (at most 4096,
  each key one emoji sequence of at most 64 bytes in UTF-8; U+FE0F is ignored). Where text has the
  sequence with emoji presentation, the image takes its place, 1em high with its aspect ratio,
  its bottom 0.1em below the baseline, and copies as the sequence. A ZWJ sequence without an
  image tries its parts; the rest is drawn by the fonts.
- `TextRun.verticalAlign` places an image run with `middle`, `textTop`, `textBottom`, `top`,
  `bottom`, or a number: points its bottom sits above the baseline (within ±14400). On a text
  run they are `INVALID_VALUE`.
- `Field.name` may contain dots, which make groups of fields: `address.street` is the field
  `street` in the group `address`. A name has at most 32 parts of 1 to 128 characters, and at
  most 512 characters in all (it was one part of at most 128). A name that is both a field and a
  group (`a` and `a.b`) is `DUPLICATE_ID`. Documents without dots render as before.
- `Field.kind: "list"`, a list box: `options` (required) and `font` (required) as for `select`,
  and `value`, the selected option. `Field.multiSelect` (list only) lets several options be
  selected; `value` is then an array of options, each at most once (at most 1000). An array
  `value` on any other field is `INVALID_VALUE`.
- `Field.format` (`$defs/FieldFormat`), on single-line text fields that are not passwords:
  `type` (`number`, `percent`, `date`, `time`, `zip`, `zipPlus4`, `phone`, `ssn`) with
  `decimals` (0 to 10), `separators`, `negative`, `currency` (at most 8 characters, no control
  characters, quotes or backslashes) and `currencyBefore` for numbers, and `pattern` (required,
  at most 64 characters; a time's shows only `H`, `h`, `M`, `s` and `t`) for dates and times.
  `value` must fit the format; a time is `HH:MM[:SS]`, or a date with a time.
- A `note` node (`NoteNode`): a text annotation, `{"type":"note","note":{"contents"}}` with
  `author`, `color` (default yellow) and `open`. `contents` is 1 to 8000 characters and `author`
  at most 256. It takes `style`, `break`, `pages`, `debug` and `id`, but no `link` or
  `bookmark`, and no space in the layout. Its icon is 20 pt square at any zoom and stays upright
  under a transform.
- `TextRun.note` (`$defs/Note`): a note in the line, in place of the run's text, a run having
  exactly one of `text`, `placeholder`, `image` and `note`. It takes no room; its icon hangs from
  the top of the line where the run is, on the page where the line starts. The run's `font` and
  `size` are the line's strut, and line breaking keeps it with the character before it, or after
  a space, the one after it.
- SVG images may contain text (`<text>`, `<tspan>`). It is laid out with the document's fonts:
  `FontDecl.family`, `weight` and `style`, which were descriptive until now, are what its
  `font-family`, `font-weight` and `font-style` select a declared font by. A document whose SVG
  images have text must declare a font; one that declares none fails with `INVALID_IMAGE`, and
  so does text on a path, vertical text and text that an XML entity expands to. The text counts
  toward the limits of its SVG as it is drawn (its text nodes as elements). Documents with SVG
  images without text render as before.

### Changed

- `debug` on a text node also outlines its lines: each line box, its baseline, and the glyphs of
  each run on the line.

### Fixed

- `Field.options` holds at most 1000 options of at most 1000 characters each, and `Field.value`
  at most 8000 characters; longer ones are `INVALID_VALUE`. Both were unbounded, so a document
  with longer ones was valid before.

## 2.3.0 - 2026-10-13

Still document schema version 1.

### Added

- `Meta.producer`, `Meta.pageMode` and `Meta.pageLayout`: the PDF producer (default
  `Inkoxide <version>`), what the viewer shows when it opens the document (`useNone`,
  `useOutlines`, `useThumbs`, `fullScreen`; default `useOutlines` when the document has
  bookmarks, else `useNone`, as before), and how it lays out the pages (`singlePage`,
  `oneColumn`, `twoColumnLeft`, `twoColumnRight`, `twoPageLeft`, `twoPageRight`; default: not
  set). All optional; documents without them render as before.

### Fixed

- **Engine parity:** integers are bounded as the schema states. An integer past the engine's own
  integer type (`weight: 70000`, `level: 300`, `orphans: -1`, `maxLines: 5000000000`) is
  `OUT_OF_RANGE`, like one past a bound the schema states; it was `INVALID_VALUE`. The schema
  states those limits where it had none: `maxLines`, `Field.maxLength`, `colSpan`, `rowSpan` and
  `headerRows` are at most 4294967295, and `pages` numbers are within ±2147483647 (from
  -2147483648). The engine already rejected larger values.
- **Engine parity:** percentages and flexible sizes are read exactly as the `Percent` and `Fr`
  patterns state. The engine accepted `".5%"`, `"5.%"`, `".5fr"` and `"-0fr"`, which the
  schema rejects; they are `INVALID_VALUE`. A table column of `"0fr"` is rejected by both now:
  the schema states that an fr column width is positive, as a width in points is, and the
  engine reports it as `OUT_OF_RANGE` instead of `INVALID_VALUE` with a misleading message.
- An empty or too long `link` or bookmark `title` is `INVALID_VALUE`, like an `Id` or a field
  `name` of the wrong length; it was `OUT_OF_RANGE`, which is for numbers.
- **Forced breaks:** a `pageBreak` or `break.before/after: "page"` inside a row, grid, wrapping
  or reversed box, or inside a positioned, rotated, transformed or fixed node, is
  `INVALID_NODE` / `INVALID_VALUE`, and so is `before/after: "page"` on an absolutely
  positioned node: the engine accepted them and did nothing. Forced breaks elsewhere work at any
  depth and inside `inside: "avoid"`, and one at the end of the flow adds no page. `Break`
  documents how rows, grids and content taller than a page split.
- **Table cells:** `TableCell.style` says how a cell is sized. A cell's `height` is a minimum
  (the row grows to fit the content) instead of a fixed height that let the content overlap the
  next row. The styles a cell ignored or misapplied are `STYLE_NOT_APPLICABLE`: `width`,
  `minWidth`, `maxWidth`, `maxHeight`, `margin`, `position`, `inset`, `alignSelf`,
  `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gridRow`, `gridColumn`, `gridArea`,
  `overflow`, `opacity` and `rotate`, as `transform`, `transformOrigin` and `zIndex` already
  were.
- **Collapsed borders:** a cell's `borderStyle` applies in a table with `borderCollapse:
  "collapse"`; dashed and dotted borders were drawn solid. `Style.borderCollapse` says how two
  borders that meet are merged: the wider wins, then solid over dashed over dotted.
- **Pagination:** `Break.minPresenceAhead` is described: it counts from the node's top and
  includes its height. On a box's first in-flow child, it and `break.before: "page"` (and a
  `pageBreak`) move the box too, so the box no longer leaves its empty top on the page before.
  A table whose first row is taller than a page splits that row where the table starts, instead
  of moving to a new page. `inset` says that relative offsets count in pagination.
- **Lengths:** a percentage height of a page child is of the page content height; it was
  `auto`. `Length` and `LengthPercent` say what percentages resolve against: gaps against the
  box's own content box, as in CSS.
- **Text:** `whiteSpace` collapses only spaces, tabs and line breaks, as in CSS; other space
  characters keep their width (they became ordinary spaces). Tabs in `pre` and `pre-wrap`
  advance to the next tab stop, 8 spaces of the font apart, instead of becoming four spaces.
- **`GridPlacement`** with `start`, `end` and `span` places the item between `start` and
  `end`, and ignores `span`, as CSS does when both lines are set after a span. The engine used
  `start` and `span` and ignored `end`. The description also says that `span` can go with `end`
  alone, which the engine already supported.
- **Page numbers in flow text** (engine 0.5.0): a `placeholder` run in a page's flow is laid
  out with the digits of the page it lands on, as in headers and footers, and the flow is
  paginated again when those digits move it. Until then flow text kept the width of a
  three-digit stand-in, so a box sized by its text was too wide for a short number and a number
  of four digits or more could overflow. Text painted on several pages (a fixed node, a table's
  header rows) keeps the stand-in width.

### Documentation

- **Descriptions:** the schema describes rules the engine already applied. A radio button needs
  a non-empty `value` (`MISSING_FIELD`); a text field's `value` is at most `maxLength`
  characters; `gridArea` cannot be combined with `gridRow` or `gridColumn`;
  `gridTemplateAreas` has at most 10000 cells; `transformOrigin` does not apply to table cells;
  a `#id` link must name a displayed node in the flowing content, not one in a header or footer
  or under `display: none`.
- **`TransformOp`:** the description said angles are within ±1000000; `skew` angles are within
  (−90, 90), as the schema's own bounds and the engine already required.
- **SCHEMA.md** states every bound of the schema: `maxItems` and `exclusiveMaximum` (an array of
  a fixed length reads "exactly N items"), the bounds of array items and of `oneOf` branches,
  and every required key of an inline object (`TrackSize` is `{min, max}`, not `{min: …}`). Its
  table of validation codes is checked against the engine's, and `PARSE_ERROR` covers
  MessagePack too.
- **Types:** a `TransformOp`, `TrackSize` or `TemplateTrack` object forbids the keys of the
  other forms (`rotate?: never` beside `translate`), so `{ translate: [1, 2], rotate: 5 }` no
  longer type-checks; validation always rejected it.

## 2.2.0 - 2026-10-12

Still document schema version 1.

### Fixed

- **Engine parity:** the engine enforces every bound the schema states. An `Id` of 0 or more
  than 256 characters and an empty `FontDecl.family` are `INVALID_VALUE`, and a
  `GridPlacement.span` of 0 is `OUT_OF_RANGE`; the engine accepted them, and laid out
  `span: 0` as 1. The schema states the engine's existing bounds of `GridPlacement`: `start`
  and `end` are within ±32767 and not 0, and `span` is at most 65535.

### Documentation

- **Descriptions:** the schema describes rules the engine already applied. `break` with a value
  other than its defaults is `INVALID_VALUE` on a header or footer, on a node in a table cell
  and on any node inside these, and `pageBreak` is `INVALID_NODE` there; page margins that
  leave no room for content are `OUT_OF_RANGE` at `margin`; a form field `name` used twice is
  `DUPLICATE_ID`.

## 2.1.0 - 2026-10-12

Still document schema version 1.

### Added

- **Debugging:** `debug` on nodes (all but `pageBreak`), table rows and cells, and pages draws
  the element's margin, border, padding and content boxes and its size over the page; on a
  page, for the page and every element on it. It changes the output and is off by default.
- **Text runs:** `hyphenate` on a run overrides the text node's `hyphenate` for the run's
  words, in the node's language.
- **Images:** `objectPosition` places an image with `fit` `contain`, `cover` or `none` in its
  box, like CSS `object-position`: `{x, y}`, each a keyword (`left`/`center`/`right`,
  `top`/`center`/`bottom`), a percentage (within ±10000%) or points (within ±14400). The
  default is centred, as before.

### Fixed

- **Fixed nodes:** a fixed node (`break.fixed`) with `position: "absolute"` that is a child of
  the page is placed against the page content box, at the same place on every page, and takes
  no space. Before, its `inset` and percentages were resolved against the whole flow, several
  pages tall, so a `bottom` inset or a node after content that moved to a later page landed
  elsewhere, and a node below the content could add a page. One placed on the first page with
  `top`, `left` and `right` in points, before any content moves to another page, renders as
  before. `fixed` now has a description in the schema. Such a node with a negative `zIndex` is
  painted under the flow, the others over it. Its own `break` may hold only `fixed`, and inside
  it `break` and `pageBreak` are refused, as they did nothing there.
- **Number bounds:** every length and the other unbounded numbers have a range, so layout can
  no longer overflow and write `inf` or `NaN` into the PDF, which made it invalid. A percentage
  like `"9999999999999999999999999999999999999%"` in `width`, `margin` or a `translate` was
  accepted. Percentages are now within ±10000% everywhere; points (lengths, column widths, grid
  tracks, border widths and radii, `borderSpacing`, `minPresenceAhead`, row `minHeight`), `fr`,
  `flexGrow`, `flexShrink`, `rotate` and the scale, rotate and matrix values of `transform` are
  within ±1000000 (non-negative where they were); `aspectRatio` is within [0.000001, 1000000].
  A value past these is `OUT_OF_RANGE` at its path; errors on grid tracks and skew angles now
  point at the value (`gridTemplateRows[0].min`, `transform[0].skew[0]`). The schema also
  states the engine's existing bounds of `lineHeight` (at most 100), `letterSpacing` (±1000)
  and `textIndent` (±10000).

## 1.0.0 - 2026-10-11

Schema version 1, the first release.

- **Nodes:** `box`, `text` (with styled runs), `image`, `table`, `field` (text, checkbox, radio
  and select form fields), `pageBreak` and `spacer`.
- **Pages:** page templates with size, margin, header and footer; `pageNumber` and
  `totalPages` placeholders in text.
- **Layout:** flexbox and CSS grid (track sizing with `minmax`, `fitContent`, `repeat` with
  `auto-fill` and `auto-fit`, named areas), absolute positioning, `zIndex`, transforms.
- **Visual styles:** backgrounds, borders per side with per-side styles, per-corner radii,
  opacity, clipping with `overflow`.
- **Text:** font, size, weight and style per run; alignment including `justify`, `start` and
  `end`; hyphenation; `whiteSpace`, `textIndent`, `maxLines` with ellipsis; bidirectional text
  with `direction`; underline and strike-through with colour, thickness and style.
- **Tables:** column widths, `colSpan` and `rowSpan`, repeating header rows, collapsed or
  separate borders with `borderSpacing`; rows split across pages.
- **Pagination:** `break` (before, after, inside, `minPresenceAhead`), `orphans` and `widows`,
  and `pages` to show a node on some pages only.
- **Links and outline:** `link` (`#id` or a URI) on nodes and runs, and `bookmark` entries
  that build the document outline.
- **Images:** JPEG, PNG, GIF (first frame), WebP and SVG, by id or as a `data:` URL.
- **Validation:** the JSON Schema carries `discriminator: { propertyName: "type" }` on `Node`,
  so ajv reports errors for the matching node type only, with the engine's error paths.
