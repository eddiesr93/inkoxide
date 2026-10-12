# Schema changelog

Changes to the document format. Within schema version 1, changes are backwards compatible:
documents that were valid stay valid and render the same way, except to fix behaviour that was
clearly wrong; such fixes are listed under **Fixed**. A breaking change would start schema
version 2.

## Unreleased

### Fixed

- **Table cells:** `TableCell.style` says how a cell is sized. A cell's `height` is a minimum
  (the row grows to fit the content) instead of a fixed height that let the content overlap the
  next row. The styles a cell ignored are `STYLE_NOT_APPLICABLE`: `width`, `maxHeight`,
  `margin`, `position`, `inset`, `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`,
  `flexBasis`, `gridRow`, `gridColumn`, `gridArea`, `overflow`, `opacity` and `rotate`, as
  `transform`, `transformOrigin` and `zIndex` already were.
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
  advance to the next multiple of 8 characters instead of becoming four spaces.

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
