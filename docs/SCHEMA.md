# Document schema v1

> Generated from [`packages/schema/document.v1.json`](../packages/schema/document.v1.json) by
> `scripts/gen-schema-doc.py`. Do not edit by hand; edit the schema and
> regenerate. Behavioural notes live in the schema `description`s and in
> [ARCHITECTURE.md](ARCHITECTURE.md).

## Conventions

- **Units.** Every length is in PDF points (1/72 in). Percentages
  (`"50%"`) are allowed only where the type says `Length` or
  `LengthPercent`; they resolve against the containing block (width for
  padding and horizontal properties, as in CSS). Gaps resolve against the
  box's own content box, and a percentage height needs a containing block
  with a set height: the page content box, for a child of the page.
- **Colours.** `[r, g, b, a]` floats in `[0, 1]`, DeviceRGB.
- **Strictness.** Unknown properties are errors (`UNKNOWN_FIELD`), as are
  properties on the wrong node type (e.g. `runs` on a `box`) and text-only
  style properties on non-text nodes (`STYLE_NOT_APPLICABLE`).
- **Resolved input.** Nothing is inherited: every text run carries its own
  font, size and colour.
- **Defaults** follow CSS flexbox except `flexDirection`, which defaults to
  `column` (document flow is vertical).
- **Header/footer.** Laid out at the full page width: once, or again on
  every page when it shows page numbers. The header is painted at the top
  of every physical page (y = 0), the footer with its bottom edge on the
  bottom of the page. Use their padding to inset them and size the page
  margins so the flow does not overlap them.
- **Placeholders.** A run with `placeholder: "pageNumber" | "totalPages"`
  is filled in after pagination, and the text wraps and aligns at the
  width of the digits. A header, a footer or an overlay is laid out with
  each page's numbers. Flow text is laid out with the numbers of the page
  it lands on, and the flow is paginated again if they move it. Flow text
  painted on several pages (in a fixed node or a table's header rows)
  reserves the width of `999` in the run's font for each page's digits; a
  longer number can overflow that space.
- **Images.** Intrinsic size is the pixel size at 96 dpi (1 px = 0.75 pt),
  like CSS. `fit` defaults to `fill`, like CSS `object-fit`, and
  `objectPosition` places a fitted image in its box like `object-position`
  (centred by default; what falls outside the box is clipped).

## Validation error codes

| Code | Meaning |
|---|---|
| `PARSE_ERROR` | Input is not well-formed JSON or MessagePack |
| `UNSUPPORTED_VERSION` | `version` is not `1` |
| `UNKNOWN_FIELD` | Property not allowed at this position |
| `MISSING_FIELD` | Required property missing |
| `INVALID_TYPE` | Wrong JSON type |
| `INVALID_VALUE` | Value not in the allowed set / wrong format; a string too short or too long; an array with more items than its fixed length; `break` where pagination does not apply (headers, footers, table cells, fixed overlays), or a forced break where it would do nothing (see `Break.before`) |
| `OUT_OF_RANGE` | Number outside its allowed range; page margins that leave no room for content |
| `DUPLICATE_ID` | Font id declared twice, or form field `name` used twice (except by the buttons of one radio group) |
| `UNKNOWN_FONT` | Font id not declared in `fonts` |
| `FONT_NOT_REGISTERED` | Declared font has no registered bytes |
| `UNKNOWN_IMAGE` | Image `src` not provided in the render options |
| `INVALID_IMAGE` | Image data cannot be decoded (a JPEG is not decoded: its structure is checked) |
| `IMAGE_TOO_LARGE` | Image over 50 megapixels (width × height) |
| `STYLE_NOT_APPLICABLE` | Style property set on a node type it does not apply to |
| `INVALID_TABLE` | Inconsistent table structure (spans, columns, header rows) |
| `INVALID_NODE` | Node type not allowed here (e.g. `pageBreak` in a header) |
| `UNKNOWN_ANCHOR` | A `#id` link names no displayed node `id` in the page body, flow or overlays (ids in headers, footers and under `display: none` are not destinations) |

Each error is `{ "code", "path", "message" }`, with `path` like
`pages[0].children[3].runs[1].font`. Validation returns at most 100
errors. A problem found while reading the document's structure (input
that does not parse, an unknown or missing property, a wrong type, a
value the reader rejects) is returned alone, as the rest cannot be
checked.

## Document (top level)

A fully resolved, declarative document tree rendered to PDF by the Inkoxide engine. All lengths are PDF points (1/72 in); all colours are RGBA floats in [0, 1]. Unknown properties are errors.

| Property | Type | Required | Notes |
|---|---|---|---|
| `version` | `1` | yes |  |
| `meta` | [`Meta`](#meta) |  |  |
| `fonts` | array of [`FontDecl`](#fontdecl) |  | Fonts the document references. Bytes are registered through the API (registerFont / options); this list lets validation fail early. |
| `fallbackFonts` | array of [`Id`](#id) |  | Font ids tried, in order, for characters missing from a run's font. Every id must be declared in `fonts`. |
| `pages` | array of [`PageSpec`](#pagespec) | yes | min items 1 |

## Definitions

### Id

Type: `string` (min length 1, max length 256)

### Link

Link target. `#id` jumps to the top of the first node with that `id` in the page body (the flow or an overlay): a node in a header or footer, or hidden by `display: none`, is not a destination (`UNKNOWN_ANCHOR` if there is none). Anything else must be an absolute URI with a scheme (`https:`, `mailto:`, …) and opens in the viewer. `javascript:` is rejected.

Type: `string` (min length 1, max length 2048)

### Bookmark

Document outline entry pointing at the top of its node. Entries appear in document order; an entry nests under the nearest earlier entry with a lower `level`. Flowing content only (not headers or footers).

| Property | Type | Required | Notes |
|---|---|---|---|
| `title` | `string` | yes | min length 1, max length 1024 |
| `level` | `integer` |  | ≥ 1, ≤ 32, default `1` |

### Debug

A development aid: draws the element's boxes on top of the page, its margin (orange), border (yellow) and padding (green) as light fills and its content box as a blue outline, with its border-box size in points as a small label. The label is drawn with lines, so no text is added to the document. A box split across pages is drawn on each page it crosses. It changes the output: leave it off in production.

Type: `boolean` (default `false`)

### Meta

| Property | Type | Required | Notes |
|---|---|---|---|
| `title` | `string` |  |  |
| `author` | `string` |  |  |
| `subject` | `string` |  |  |
| `keywords` | `string` |  |  |
| `creator` | `string` |  |  |
| `producer` | `string` |  | The PDF producer, written to the PDF Info dictionary (and to the XMP metadata of PDF/A). Default `Inkoxide <version>`. |
| `lang` | `string` |  | BCP-47 language tag of the document, e.g. "en" or "en-GB". |
| `creationDate` | `string` |  | format `date-time`; RFC 3339 timestamp written to the PDF Info dictionary. Overridden by the creationDate render option. When neither is set, no date is written (output stays deterministic). |
| `pageMode` | `"useNone"` \| `"useOutlines"` \| `"useThumbs"` \| `"fullScreen"` |  | What the viewer shows when it opens the document: no panel (`useNone`), the bookmarks (`useOutlines`), page thumbnails (`useThumbs`) or the pages full screen (`fullScreen`). Default: `useOutlines` when the document has bookmarks, else `useNone`. |
| `pageLayout` | `"singlePage"` \| `"oneColumn"` \| `"twoColumnLeft"` \| `"twoColumnRight"` \| `"twoPageLeft"` \| `"twoPageRight"` |  | How the viewer lays out the pages: one at a time (`singlePage`), one scrolling column (`oneColumn`), two scrolling columns with odd pages on the left or right (`twoColumnLeft`, `twoColumnRight`), or two pages at a time with odd pages on the left or right (`twoPageLeft`, `twoPageRight`). Default: the viewer's own setting. |

### FontDecl

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) | yes |  |
| `family` | `string` | yes | min length 1 |
| `weight` | `integer` |  | ≥ 1, ≤ 1000, default `400` |
| `style` | `"normal"` \| `"italic"` |  | default `"normal"` |

### NonNegative

A non-negative number of at most 1000000.

Type: `number` (≥ 0, ≤ 1000000)

### Percent

A percentage within ±10000%, like `"50%"`.

Type: `string` matching `^-?0*([0-9]{1,4}(\.[0-9]+)?|10000(\.0+)?)%$`

### Fr

A flexible size, a share of the free space, of at most 1000000fr, like `"1fr"`.

Type: `string` matching `^0*([0-9]{1,6}(\.[0-9]+)?|1000000(\.0+)?)fr$`

### Length

Points (within ±1000000), a percentage of the containing block (within ±10000%), or "auto". A percentage height needs a containing block with a definite height, as in CSS: a child of the page takes it of the page content height; in a header or footer, or in a box whose height is not set, it is `auto`.

Type: `number` (≥ -1000000, ≤ 1000000) or [`Percent`](#percent) or `"auto"`

### LengthPercent

Points (within ±1000000) or a percentage of the containing block's width (within ±10000%). In `gap`, `rowGap` and `columnGap`, a percentage is of the box's own content box instead, as in CSS: a column gap of its width, a row gap of its height.

Type: `number` (≥ -1000000, ≤ 1000000) or [`Percent`](#percent)

### Color

[r, g, b, a], each in [0, 1].

Type: array of `number` (each ≥ 0, ≤ 1) (exactly 4 items)

### Size

| Property | Type | Required | Notes |
|---|---|---|---|
| `width` | `number` | yes | > 0, ≤ 14400 |
| `height` | `number` | yes | > 0, ≤ 14400 |

### EdgesNumber

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`NonNegative`](#nonnegative) |  |  |
| `right` | [`NonNegative`](#nonnegative) |  |  |
| `bottom` | [`NonNegative`](#nonnegative) |  |  |
| `left` | [`NonNegative`](#nonnegative) |  |  |

### EdgesLengthPercent

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`LengthPercent`](#lengthpercent) |  |  |
| `right` | [`LengthPercent`](#lengthpercent) |  |  |
| `bottom` | [`LengthPercent`](#lengthpercent) |  |  |
| `left` | [`LengthPercent`](#lengthpercent) |  |  |

### EdgesLength

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`Length`](#length) |  |  |
| `right` | [`Length`](#length) |  |  |
| `bottom` | [`Length`](#length) |  |  |
| `left` | [`Length`](#length) |  |  |

### EdgesColor

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`Color`](#color) |  |  |
| `right` | [`Color`](#color) |  |  |
| `bottom` | [`Color`](#color) |  |  |
| `left` | [`Color`](#color) |  |  |

### PageSpec

A page template. Its content flows across as many physical pages as needed.

| Property | Type | Required | Notes |
|---|---|---|---|
| `size` | [`Size`](#size) | yes |  |
| `margin` | [`EdgesNumber`](#edgesnumber) |  | `left + right` must be less than the page width and `top + bottom` less than its height; margins that leave no room for content are `OUT_OF_RANGE` at `margin`. |
| `background` | [`Color`](#color) |  |  |
| `debug` | [`Debug`](#debug) |  | Draws the boxes of the page (its margins and content area) and of every element on it, the header and footer included. |
| `header` | [`Node`](#node) |  | Laid out once at the full page width and painted at the top of every physical page (y = 0). Use padding to inset it. Size the top margin to leave room for it. `break` (other than its defaults) and `pageBreak` are not allowed in it, at any depth. |
| `footer` | [`Node`](#node) |  | Laid out once at the full page width and painted with its bottom edge on the bottom of every physical page. `break` (other than its defaults) and `pageBreak` are not allowed in it, at any depth. |
| `children` | array of [`Node`](#node) |  |  |

### Break

Pagination controls. They apply to flowing content only: on a header or footer, on a node in a table cell, or on any node inside these, a `break` with a value other than the defaults is `INVALID_VALUE`. For fixed overlays, see `fixed`. A box laid out as a row, a grid, wrapping or reversed moves to the next page whole when it fits on one; taller than a page, it splits between its rows (children whose extents overlap), each child continuing from the row's top. Content that cannot split (an image, a positioned, rotated, transformed or fixed node, table rows tied by a row span) is cut across pages when it is taller than a page.

| Property | Type | Required | Notes |
|---|---|---|---|
| `before` | `"auto"` \| `"page"` |  | default `"auto"`; `page` starts a new page before the node, unless it is at the top of one already. `INVALID_VALUE` inside a row, grid, wrapping or reversed box, or inside a positioned, rotated, transformed or fixed node, or on an absolutely positioned node: there it would do nothing. |
| `after` | `"auto"` \| `"page"` |  | default `"auto"`; `page` starts a new page after the node when in-flow content follows it: a break at the end of the flow adds no page. Allowed where `before` is. |
| `inside` | `"auto"` \| `"avoid"` |  | default `"auto"`; `avoid` keeps the node on one page: it moves to the next page whole when it does not fit. A node taller than a page splits anyway, and a forced page break inside it still starts a new page; the part before the break moves whole. |
| `minPresenceAhead` | [`NonNegative`](#nonnegative) |  | Points of room the node needs from its top: when less is left on the page than this (or than its first unbreakable piece, when taller), the node moves to the next page. The node's own height counts, and nothing needs to follow it. On a box's first in-flow child it applies to the box too, so a box never leaves its top padding behind. `break.before: "page"` on a first child moves the box the same way. |
| `fixed` | `boolean` |  | default `false`; Paint the node on every page of its flow. A child of the page with `position: "absolute"` is laid out on its own, against the page content box (`inset` and percentages are relative to one page's content box, inside the margins), takes no space and is painted over the flow, or under it when its `zIndex` is negative; no other pagination control applies to it or inside it, and it cannot hold a `pageBreak`. Any other fixed node is laid out in the flow, takes its space there once, and is painted at the same place on every page. |

### GridPlacement

1-based CSS grid line placement. Negative lines count from the end; there is no line 0. Use `span` alone, with `start` or with `end`. With both `start` and `end`, `span` is ignored.

| Property | Type | Required | Notes |
|---|---|---|---|
| `start` | `integer` |  | ≥ -32767, ≤ 32767, ≠ 0 |
| `end` | `integer` |  | ≥ -32767, ≤ 32767, ≠ 0 |
| `span` | `integer` |  | ≥ 1, ≤ 65535 |

### TrackSize

Grid track: a breadth, `{min, max}` (CSS `minmax()`; `min` cannot be fr) or `{fitContent}` (CSS `fit-content()`).

Type: [`TrackBreadth`](#trackbreadth) or `{min, max}` (`min`: [`TrackBreadth`](#trackbreadth); `max`: [`TrackBreadth`](#trackbreadth)) or `{fitContent}` (`fitContent`: [`LengthPercent`](#lengthpercent))

### BorderRadius

One radius for every corner, or one per corner (a missing corner is square). Radii that would overlap are scaled down together, as in CSS.

Type: [`NonNegative`](#nonnegative) or object with `topLeft`, `topRight`, `bottomRight`, `bottomLeft`

### BorderStyles

One style for every side, or one per side (an unset side is solid).

Type: `"solid"` \| `"dashed"` \| `"dotted"` or object with `top`, `right`, `bottom`, `left`

### TransformOp

One transform function, as an object with exactly one key. Lengths are points or percentages of the node's own border box; angles are degrees, clockwise. Scale factors, `rotate` and matrix entries are within ±1000000; `skew` angles are within (−90, 90).

Type: `{translate}` (`translate`: array of [`LengthPercent`](#lengthpercent) (exactly 2 items)) or `{scale}` (`scale`: array of `number` (each ≥ -1000000, ≤ 1000000) (exactly 2 items)) or `{rotate}` (`rotate`: `number` (≥ -1000000, ≤ 1000000)) or `{skew}` (`skew`: array of `number` (each > -90, < 90) (exactly 2 items)) or `{matrix}` (`matrix`: array of `number` (each ≥ -1000000, ≤ 1000000) (exactly 6 items))

### Style

| Property | Type | Required | Notes |
|---|---|---|---|
| `display` | `"flex"` \| `"grid"` \| `"none"` |  | default `"flex"` |
| `flexDirection` | `"row"` \| `"column"` \| `"row-reverse"` \| `"column-reverse"` |  | default `"column"` |
| `flexWrap` | `"nowrap"` \| `"wrap"` \| `"wrap-reverse"` |  | default `"nowrap"` |
| `justifyContent` | `"flex-start"` \| `"flex-end"` \| `"center"` \| `"space-between"` \| `"space-around"` \| `"space-evenly"` \| `"start"` \| `"end"` \| `"stretch"` |  |  |
| `alignItems` | `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  |  |
| `alignSelf` | `"auto"` \| `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  |  |
| `alignContent` | `"flex-start"` \| `"flex-end"` \| `"center"` \| `"space-between"` \| `"space-around"` \| `"space-evenly"` \| `"stretch"` \| `"start"` \| `"end"` |  |  |
| `flexGrow` | [`NonNegative`](#nonnegative) |  |  |
| `flexShrink` | [`NonNegative`](#nonnegative) |  |  |
| `flexBasis` | [`Length`](#length) |  |  |
| `gap` | [`LengthPercent`](#lengthpercent) |  | Sets both rowGap and columnGap; either of those, when present, wins. |
| `rowGap` | [`LengthPercent`](#lengthpercent) |  | A percentage is of the box's own content height, as in CSS. In a box whose height is not set, it is of the height of the content without the gaps, so it adds little: use points. |
| `columnGap` | [`LengthPercent`](#lengthpercent) |  |  |
| `width` | [`Length`](#length) |  |  |
| `height` | [`Length`](#length) |  |  |
| `minWidth` | [`Length`](#length) |  |  |
| `minHeight` | [`Length`](#length) |  |  |
| `maxWidth` | [`Length`](#length) |  |  |
| `maxHeight` | [`Length`](#length) |  |  |
| `aspectRatio` | `number` |  | ≥ 0.000001, ≤ 1000000; Width divided by height. |
| `padding` | [`EdgesLengthPercent`](#edgeslengthpercent) |  |  |
| `margin` | [`EdgesLength`](#edgeslength) |  |  |
| `position` | `"relative"` \| `"absolute"` |  | default `"relative"` |
| `inset` | [`EdgesLength`](#edgeslength) |  | top/right/bottom/left offsets for positioned boxes. A relative box (the default `position`) is moved by them before pagination, so pagination sees it where it is drawn: moved past the foot of a page, it moves to the next page or runs onto it. |
| `gridTemplateColumns` | array of [`TemplateTrack`](#templatetrack) |  |  |
| `gridTemplateRows` | array of [`TemplateTrack`](#templatetrack) |  |  |
| `gridTemplateAreas` | array of `string` |  | min items 1; Rows of whitespace-separated area names, as in CSS; a token of dots is an unnamed cell. Every row has the same number of cells and each area is a rectangle. At most 10000 cells (rows × columns; `OUT_OF_RANGE`). |
| `gridAutoColumns` | array of [`TrackSize`](#tracksize) |  |  |
| `gridAutoRows` | array of [`TrackSize`](#tracksize) |  |  |
| `gridAutoFlow` | `"row"` \| `"column"` \| `"row dense"` \| `"column dense"` |  |  |
| `gridColumn` | [`GridPlacement`](#gridplacement) |  |  |
| `gridRow` | [`GridPlacement`](#gridplacement) |  |  |
| `gridArea` | `string` |  | min length 1; Name of an area in the parent's `gridTemplateAreas`; replaces `gridRow` and `gridColumn`, and cannot be combined with them (`INVALID_VALUE`). |
| `justifyItems` | `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  | Grid only: default inline-axis alignment of the items. |
| `justifySelf` | `"auto"` \| `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  | Grid only: inline-axis alignment of this item. |
| `overflow` | `"visible"` \| `"hidden"` |  | default `"visible"` |
| `backgroundColor` | [`Color`](#color) |  |  |
| `opacity` | `number` |  | ≥ 0, ≤ 1 |
| `borderWidth` | [`EdgesNumber`](#edgesnumber) |  |  |
| `borderColor` | [`EdgesColor`](#edgescolor) |  |  |
| `borderRadius` | [`BorderRadius`](#borderradius) |  |  |
| `borderStyle` | [`BorderStyles`](#borderstyles) |  | default `"solid"` |
| `rotate` | `number` |  | ≥ -1000000, ≤ 1000000; Degrees, clockwise, around the box centre. |
| `transform` | array of [`TransformOp`](#transformop) |  | Transform functions, as in CSS: the list applies right to left, after `rotate`. Not on table cells. A transformed node moves to the next page as a whole. |
| `transformOrigin` | array of [`LengthPercent`](#lengthpercent) |  | exactly 2 items; x and y of the origin of `rotate` and `transform` within the border box. Default `["50%", "50%"]`. Not on table cells. |
| `zIndex` | `integer` |  | ≥ -2147483648, ≤ 2147483647; Paint order among siblings: higher paints later (above); equal values keep document order. Default 0. Not on table cells. |
| `textAlign` | `"left"` \| `"center"` \| `"right"` \| `"justify"` \| `"start"` \| `"end"` |  | text nodes only; `start`/`end` follow the paragraph direction. Default `start`. |
| `lineHeight` | `number` |  | > 0, ≤ 100; text nodes only; multiplier of the largest run size on the line |
| `maxLines` | `integer` |  | ≥ 1, ≤ 4294967295; text nodes only |
| `ellipsis` | `boolean` |  | text nodes only; requires maxLines |
| `hyphenate` | `boolean` |  | text nodes only |
| `lang` | `string` |  | text nodes only; BCP-47, selects hyphenation patterns |
| `whiteSpace` | `"normal"` \| `"pre"` \| `"nowrap"` \| `"pre-wrap"` \| `"pre-line"` |  | text nodes only. `normal` collapses spaces and keeps newlines as breaks (as `pre-line`); `pre-wrap` keeps spaces and tabs and wraps. As in CSS, only spaces, tabs and line breaks collapse: other space characters (thin, em, figure, no-break, …) keep their width, and only the breaking ones are break opportunities. `pre` and `pre-wrap` advance a tab to the next tab stop (CSS `tab-size: 8`): stops are 8 spaces of the tab's font apart, from the start of the line (a line `pre-wrap` wraps keeps the stops of the line it continues), and a tab narrower than half a `0` goes on to the next stop. |
| `textIndent` | `number` |  | ≥ -10000, ≤ 10000; text nodes only; indent of the first line in points (negative: hanging indent) |
| `direction` | `"ltr"` \| `"rtl"` \| `"auto"` |  | default `"ltr"`; text nodes only; base direction of the paragraph (`auto`: from its first strong character). Right-to-left runs are reordered with the Unicode bidi algorithm in any direction. |
| `orphans` | `integer` |  | ≥ 1, ≤ 100, default `1`; text nodes only; minimum number of lines left at the bottom of a page when the paragraph breaks |
| `widows` | `integer` |  | ≥ 1, ≤ 100, default `1`; text nodes only; minimum number of lines carried to the top of the next page when the paragraph breaks |
| `verticalAlign` | `"top"` \| `"middle"` \| `"bottom"` |  | table cells only |
| `borderCollapse` | `"collapse"` \| `"separate"` |  | default `"collapse"`; Tables only. `collapse`: adjacent cell borders merge into one, drawn in the winner's style and colour: the wider wins, and at equal widths solid wins over dashed and dashed over dotted. `separate`: every cell draws its own borders, background and `borderRadius`, `borderSpacing` apart. |
| `borderSpacing` | [`NonNegative`](#nonnegative) |  | Tables with `borderCollapse: separate` only: space between cells and around them, in points. Default 0. |

### TextRun

| Property | Type | Required | Notes |
|---|---|---|---|
| `text` | `string` |  | Required unless `placeholder` is set; forbidden when it is. |
| `font` | [`Id`](#id) | yes |  |
| `size` | `number` | yes | > 0, ≤ 1000 |
| `color` | [`Color`](#color) |  |  |
| `letterSpacing` | `number` |  | ≥ -1000, ≤ 1000 |
| `decoration` | `"none"` \| `"underline"` \| `"lineThrough"` |  |  |
| `decorationColor` | [`Color`](#color) |  | Default: the run colour. |
| `decorationThickness` | `number` |  | > 0, ≤ 100; Points. Default: from the font. |
| `decorationStyle` | `"solid"` \| `"double"` \| `"dotted"` \| `"dashed"` \| `"wavy"` |  | default `"solid"` |
| `placeholder` | `"pageNumber"` \| `"totalPages"` |  |  |
| `link` | [`Link`](#link) |  | Makes the run a link; the clickable area covers the run's glyphs on each line. |
| `hyphenate` | `boolean` |  | Overrides the text node's `hyphenate` for this run's words, in the node's language. |

### ColumnSpec

| Property | Type | Required | Notes |
|---|---|---|---|
| `width` | `number` (> 0, ≤ 1000000) or [`Fr`](#fr) (not matching `^0+(\.0+)?fr$`) or `"auto"` | yes |  |

### TableCell

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `colSpan` | `integer` |  | ≥ 1, ≤ 4294967295, default `1` |
| `rowSpan` | `integer` |  | ≥ 1, ≤ 4294967295, default `1` |
| `style` | [`Style`](#style) |  | The table places and sizes the cell: its columns set the width, and `height` and `minHeight` are minimums (the row grows to fit the content). The cell's own placement, size and effects are `STYLE_NOT_APPLICABLE`: `width`, `minWidth`, `maxWidth`, `maxHeight`, `margin`, `position`, `inset`, `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gridRow`, `gridColumn`, `gridArea`, `overflow`, `opacity`, `rotate`, `transform`, `transformOrigin` and `zIndex`. |
| `children` | array of [`Node`](#node) |  |  |

### TableRow

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `minHeight` | [`NonNegative`](#nonnegative) |  |  |
| `backgroundColor` | [`Color`](#color) |  |  |
| `cells` | array of [`TableCell`](#tablecell) | yes |  |

### Node

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"box"` \| `"text"` \| `"image"` \| `"table"` \| `"pageBreak"` \| `"spacer"` \| `"field"` | yes |  |

### BoxNode

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"box"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `link` | [`Link`](#link) |  |  |
| `bookmark` | [`Bookmark`](#bookmark) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `children` | array of [`Node`](#node) |  |  |

### TextNode

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"text"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `link` | [`Link`](#link) |  |  |
| `bookmark` | [`Bookmark`](#bookmark) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `runs` | array of [`TextRun`](#textrun) | yes |  |

### ImageNode

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"image"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `link` | [`Link`](#link) |  |  |
| `bookmark` | [`Bookmark`](#bookmark) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `src` | [`Id`](#id) |  | Key into the render option `images`. |
| `data` | `string` |  | Inline image bytes (JPEG, PNG, GIF, WebP or SVG; the first frame of an animation), base64-encoded; a `data:<mime>;base64,` URL is also accepted. |
| `fit` | `"fill"` \| `"contain"` \| `"cover"` \| `"none"` |  | default `"fill"` |
| `objectPosition` | [`ObjectPosition`](#objectposition) |  | Where the image sits in its box with `fit` `contain`, `cover` or `none` (CSS `object-position`); `fill` ignores it. Default centred. |

### ObjectPosition

Position of a fitted image in its box, per axis: a keyword, points from the left or top edge, or a percentage (that point of the image goes to that point of the box: `"0%"` is the start edge, `"100%"` the end edge). Points are within ±14400 and percentages within ±10000%. What falls outside the box is clipped.

| Property | Type | Required | Notes |
|---|---|---|---|
| `x` | `"left"` \| `"center"` \| `"right"` or `number` (≥ -14400, ≤ 14400) or [`Percent`](#percent) |  | default `"center"` |
| `y` | `"top"` \| `"center"` \| `"bottom"` or `number` (≥ -14400, ≤ 14400) or [`Percent`](#percent) |  | default `"center"` |

### TableNode

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"table"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `link` | [`Link`](#link) |  |  |
| `bookmark` | [`Bookmark`](#bookmark) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `columns` | array of [`ColumnSpec`](#columnspec) | yes | min items 1 |
| `headerRows` | `integer` |  | ≥ 0, ≤ 4294967295, default `0` |
| `rows` | array of [`TableRow`](#tablerow) | yes |  |

### PageBreakNode

Starts a new page when in-flow content follows it: at the end of the flow it adds no page. Flowing content only: in a header, a footer, a table cell or a fixed overlay, and inside a row, grid, wrapping or reversed box, or inside a positioned, rotated, transformed or fixed node, it is `INVALID_NODE`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"pageBreak"` | yes |  |
| `id` | [`Id`](#id) |  |  |

### SpacerNode

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"spacer"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `link` | [`Link`](#link) |  |  |
| `bookmark` | [`Bookmark`](#bookmark) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |

### FieldNode

An interactive AcroForm field. `style` sizes and decorates its box (background, border, padding); the widget covers the content box. Without a width or height it takes an intrinsic size (text: 150 pt wide, one line, or three when multiline; checkbox and radio: `size` + 2 pt square).

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"field"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `field` | [`Field`](#field) | yes |  |

### Field

Fields with the same `name` are one field with several widgets (for example a field in a header, repeated on every page); only radio buttons may share a name with different values.

| Property | Type | Required | Notes |
|---|---|---|---|
| `name` | `string` matching `^[^.]+$` | yes | min length 1, max length 128; Field name (no `.`); unique in the document except for the buttons of one radio group. A name used again is `DUPLICATE_ID`. |
| `kind` | `"text"` \| `"checkbox"` \| `"radio"` \| `"select"` | yes |  |
| `value` | `string` |  | max length 8000; text: the initial text, at most `maxLength` characters (`OUT_OF_RANGE`); select: the selected option (one of `options`); radio: the value this button exports when selected, required and non-empty (`MISSING_FIELD`). At most 8000 characters. |
| `checked` | `boolean` |  | default `false`; checkbox and radio: initially selected |
| `options` | array of `string` (each max length 1000) |  | min items 1, max items 1000; select only: the choices, at most 1000 of at most 1000 characters each |
| `multiline` | `boolean` |  | default `false`; text only |
| `password` | `boolean` |  | default `false`; text only; the value is neither shown nor stored |
| `maxLength` | `integer` |  | ≥ 1, ≤ 4294967295; text only; maximum number of characters |
| `readOnly` | `boolean` |  | default `false` |
| `required` | `boolean` |  | default `false` |
| `font` | [`Id`](#id) |  | text and select (required there): a declared font for the value. Its glyphs for the common Latin range are embedded so edits display. |
| `size` | `number` |  | > 0, ≤ 1000, default `10`; font size; also the mark size of checkboxes and radio buttons |
| `color` | [`Color`](#color) |  | text and mark colour; default black |

### TrackBreadth

Grid track breadth: points, a percentage, "<n>fr", "auto", "min-content" or "max-content". Points and fr are at most 1000000.

Type: `number` (≥ 0, ≤ 1000000) or [`Percent`](#percent) or [`Fr`](#fr) or `"auto"` \| `"min-content"` \| `"max-content"`

### TemplateTrack

Entry of a track template: a track, or CSS `repeat()` as `{repeat, tracks}`. `auto-fill`/`auto-fit` may appear once per list, and then every track in the list needs a fixed (points or percentage) minimum or maximum.

Type: [`TrackSize`](#tracksize) or `{repeat, tracks}` (`repeat`: `integer` (≥ 1, ≤ 1000) or `"auto-fill"` \| `"auto-fit"`; `tracks`: array of [`TrackSize`](#tracksize) (min items 1))

### PageFilter

Pages that show the node, by physical page number (as the `pageNumber` placeholder). The node keeps its place in the layout on every page; it is only painted on the matching ones.

Type: `"first"` \| `"last"` \| `"notFirst"` \| `"notLast"` \| `"odd"` \| `"even"` or array of `integer` (each ≥ -2147483648, ≤ 2147483647, ≠ 0) (min items 1)
