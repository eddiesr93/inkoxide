# Schema changelog

Changes to the document format. Within schema version 1, changes are backwards compatible:
documents that were valid stay valid and render the same way, except to fix behaviour that was
clearly wrong; such fixes are listed under **Fixed**. A breaking change would start schema
version 2.

## Unreleased

### Fixed

- **Engine parity:** the engine enforces every bound the schema states. An `Id` of 0 or more
  than 256 characters and an empty `FontDecl.family` are `INVALID_VALUE`, and a
  `GridPlacement.span` of 0 is `OUT_OF_RANGE`; the engine accepted them, and laid out
  `span: 0` as 1. The schema states the engine's existing bounds of `GridPlacement`: `start`
  and `end` are within ±32767 and not 0, and `span` is at most 65535.
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
