# Schema changelog

Changes to the document format. Within schema version 1, changes are backwards compatible:
documents that were valid stay valid and render the same way, except to fix behaviour that was
clearly wrong; such fixes are listed under **Fixed**. A breaking change would start schema
version 2.

## Unreleased

Still document schema version 1.

### Added

- `BoxNode.backgroundImage` (`$defs/BackgroundImage`): an image painted into the box's padding
  box, over its `backgroundColor` and under its borders and children, clipped to the padding box
  rounded by `borderRadius`. It takes `src` or `data` (exactly one, as on an `image` node), `fit`
  (default `fill`) and `objectPosition`, with the same bounds as on an `image` node. Optional;
  documents without it render as before.
- `Field.name` may contain dots, which make groups of fields: `address.street` is the field
  `street` in the group `address`. A name has at most 32 parts of 1 to 128 characters, and at
  most 512 characters in all (it was one part of at most 128). A name that is both a field and a
  group (`a` and `a.b`) is `DUPLICATE_ID`. Documents without dots render as before.
- `Field.kind: "list"`, a list box: `options` (required) and `font` (required) as for `select`,
  and `value`, the selected option. `Field.multiSelect` (list only) lets several options be
  selected; `value` is then an array of options, each at most once (at most 1000). An array
  `value` on any other field is `INVALID_VALUE`.
- A `note` node (`NoteNode`): a text annotation, `{"type":"note","note":{"contents"}}` with
  `author`, `color` (default yellow) and `open`. `contents` is 1 to 8000 characters and `author`
  at most 256. It takes `style`, `break`, `pages`, `debug` and `id`, but no `link` or
  `bookmark`, and no space in the layout.

### Changed

- `Field.options` holds at most 1000 options of at most 1000 characters each, and `Field.value`
  at most 8000 characters; longer ones are `INVALID_VALUE`.

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
