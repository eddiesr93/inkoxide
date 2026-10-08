# Document schema v1

> Generated from [`schema/document.v1.json`](../schema/document.v1.json) by
> `scripts/gen-schema-doc.py`. Do not edit by hand; edit the schema and
> regenerate. Behavioural notes live in the schema `description`s and in
> [ARCHITECTURE.md](ARCHITECTURE.md).

## Conventions

- **Units.** Every length is in PDF points (1/72 in). Percentages
  (`"50%"`) are allowed only where the type says `Length` or
  `LengthPercent`; they resolve against the containing block (width for
  padding and horizontal properties, as in CSS).
- **Colours.** `[r, g, b, a]` floats in `[0, 1]`, DeviceRGB.
- **Strictness.** Unknown properties are errors (`UNKNOWN_FIELD`), as are
  properties on the wrong node type (e.g. `runs` on a `box`) and text-only
  style properties on non-text nodes (`STYLE_NOT_APPLICABLE`).
- **Resolved input.** Nothing is inherited: every text run carries its own
  font, size and colour.
- **Defaults** follow CSS flexbox except `flexDirection`, which defaults to
  `column` (document flow is vertical).
- **Header/footer.** Laid out once at the full page width. The header is
  painted at the top of every physical page (y = 0), the footer with its
  bottom edge on the bottom of the page. Use their padding to inset them and
  size the page margins so the flow does not overlap them.
- **Placeholders.** A run with `placeholder: "pageNumber" | "totalPages"`
  is filled in after pagination. Layout reserves the width of `999` in the
  run's font, so the surrounding text never reflows.
- **Images.** Intrinsic size is the pixel size at 96 dpi (1 px = 0.75 pt),
  like CSS. `fit` defaults to `fill`, like CSS `object-fit`.

## Validation error codes

| Code | Meaning |
|---|---|
| `PARSE_ERROR` | Input is not well-formed JSON |
| `UNSUPPORTED_VERSION` | `version` is not `1` |
| `UNKNOWN_FIELD` | Property not allowed at this position |
| `MISSING_FIELD` | Required property missing |
| `INVALID_TYPE` | Wrong JSON type |
| `INVALID_VALUE` | Value not in the allowed set / wrong format |
| `OUT_OF_RANGE` | Number outside its allowed range |
| `DUPLICATE_ID` | Font id declared twice |
| `UNKNOWN_FONT` | Font id not declared in `fonts` |
| `FONT_NOT_REGISTERED` | Declared font has no registered bytes |
| `UNKNOWN_IMAGE` | Image `src` not provided in the render options |
| `INVALID_IMAGE` | Image data cannot be decoded |
| `IMAGE_TOO_LARGE` | Decoded image exceeds the size guard |
| `STYLE_NOT_APPLICABLE` | Style property set on a node type it does not apply to |
| `INVALID_TABLE` | Inconsistent table structure (spans, columns, header rows) |
| `INVALID_NODE` | Node type not allowed here (e.g. `pageBreak` in a header) |
| `UNKNOWN_ANCHOR` | A `#id` link names no node `id` in the document |

Each error is `{ "code", "path", "message" }`, with `path` like
`pages[0].children[3].runs[1].font`.

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

Type: `string`

### Link

Link target. `#id` jumps to the top of the first node with that `id` (`UNKNOWN_ANCHOR` if none); anything else must be an absolute URI with a scheme (`https:`, `mailto:`, …) and opens in the viewer. `javascript:` is rejected.

Type: `string`

### Bookmark

Document outline entry pointing at the top of its node. Entries appear in document order; an entry nests under the nearest earlier entry with a lower `level`. Flowing content only (not headers or footers).

| Property | Type | Required | Notes |
|---|---|---|---|
| `title` | `string` | yes |  |
| `level` | `integer` |  | ≥ 1, ≤ 32, default `1` |

### Meta

| Property | Type | Required | Notes |
|---|---|---|---|
| `title` | `string` |  |  |
| `author` | `string` |  |  |
| `subject` | `string` |  |  |
| `keywords` | `string` |  |  |
| `creator` | `string` |  |  |
| `lang` | `string` |  | BCP-47 language tag of the document, e.g. "ro" or "en-GB". |
| `creationDate` | `string` |  | RFC 3339 timestamp written to the PDF Info dictionary. Overridden by the creationDate render option. When neither is set, no date is written (output stays deterministic). |

### FontDecl

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) | yes |  |
| `family` | `string` | yes |  |
| `weight` | `integer` |  | ≥ 1, ≤ 1000, default `400` |
| `style` | `"normal"` \| `"italic"` |  | default `"normal"` |

### NonNegative

Type: `number` (≥ 0)

### Percent

Type: `string` matching `^-?[0-9]+(\.[0-9]+)?%$`

### Length

Points, a percentage of the containing block, or "auto".

Type: `number` or [`Percent`](#percent) or `"auto"`

### LengthPercent

Points or a percentage of the containing block's width.

Type: `number` or [`Percent`](#percent)

### Color

[r, g, b, a], each in [0, 1].

Type: array of `number` (min items 4)

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
| `margin` | [`EdgesNumber`](#edgesnumber) |  |  |
| `background` | [`Color`](#color) |  |  |
| `header` | [`Node`](#node) |  | Laid out once at the full page width and painted at the top of every physical page (y = 0). Use padding to inset it. Size the top margin to leave room for it. |
| `footer` | [`Node`](#node) |  | Laid out once at the full page width and painted with its bottom edge on the bottom of every physical page. |
| `children` | array of [`Node`](#node) |  |  |

### Break

| Property | Type | Required | Notes |
|---|---|---|---|
| `before` | `"auto"` \| `"page"` |  | default `"auto"` |
| `after` | `"auto"` \| `"page"` |  | default `"auto"` |
| `inside` | `"auto"` \| `"avoid"` |  | default `"auto"` |
| `minPresenceAhead` | [`NonNegative`](#nonnegative) |  |  |
| `fixed` | `boolean` |  | default `false` |

### GridPlacement

1-based CSS grid line placement. Negative lines count from the end. Use `span` alone or with `start`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `start` | `integer` |  |  |
| `end` | `integer` |  |  |
| `span` | `integer` |  | ≥ 1 |

### TrackSize

Grid track: a breadth, `{min, max}` (CSS `minmax()`; `min` cannot be fr) or `{fitContent}` (CSS `fit-content()`).

Type: [`TrackBreadth`](#trackbreadth) or `{min: …}` ([`TrackBreadth`](#trackbreadth)) or `{fitContent: …}` ([`LengthPercent`](#lengthpercent))

### BorderRadius

One radius for every corner, or one per corner (a missing corner is square). Radii that would overlap are scaled down together, as in CSS.

Type: [`NonNegative`](#nonnegative) or object with `topLeft`, `topRight`, `bottomRight`, `bottomLeft`

### BorderStyles

One style for every side, or one per side (an unset side is solid).

Type: `"solid"` \| `"dashed"` \| `"dotted"` or object with `top`, `right`, `bottom`, `left`

### TransformOp

One transform function, as an object with exactly one key. Lengths are points or percentages of the node's own border box; angles are degrees, clockwise.

Type: `{translate: …}` (array of [`LengthPercent`](#lengthpercent)) or `{scale: …}` (array of `number`) or `{rotate: …}` (`number`) or `{skew: …}` (array of `number`) or `{matrix: …}` (array of `number`)

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
| `rowGap` | [`LengthPercent`](#lengthpercent) |  |  |
| `columnGap` | [`LengthPercent`](#lengthpercent) |  |  |
| `width` | [`Length`](#length) |  |  |
| `height` | [`Length`](#length) |  |  |
| `minWidth` | [`Length`](#length) |  |  |
| `minHeight` | [`Length`](#length) |  |  |
| `maxWidth` | [`Length`](#length) |  |  |
| `maxHeight` | [`Length`](#length) |  |  |
| `aspectRatio` | `number` |  | > 0 |
| `padding` | [`EdgesLengthPercent`](#edgeslengthpercent) |  |  |
| `margin` | [`EdgesLength`](#edgeslength) |  |  |
| `position` | `"relative"` \| `"absolute"` |  | default `"relative"` |
| `inset` | [`EdgesLength`](#edgeslength) |  | top/right/bottom/left offsets for positioned boxes. |
| `gridTemplateColumns` | array of [`TemplateTrack`](#templatetrack) |  |  |
| `gridTemplateRows` | array of [`TemplateTrack`](#templatetrack) |  |  |
| `gridTemplateAreas` | array of `string` |  | min items 1; Rows of whitespace-separated area names, as in CSS; a token of dots is an unnamed cell. Every row has the same number of cells and each area is a rectangle. |
| `gridAutoColumns` | array of [`TrackSize`](#tracksize) |  |  |
| `gridAutoRows` | array of [`TrackSize`](#tracksize) |  |  |
| `gridAutoFlow` | `"row"` \| `"column"` \| `"row dense"` \| `"column dense"` |  |  |
| `gridColumn` | [`GridPlacement`](#gridplacement) |  |  |
| `gridRow` | [`GridPlacement`](#gridplacement) |  |  |
| `gridArea` | `string` |  | Name of an area in the parent's `gridTemplateAreas`; replaces `gridRow` and `gridColumn`. |
| `justifyItems` | `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  | Grid only: default inline-axis alignment of the items. |
| `justifySelf` | `"auto"` \| `"flex-start"` \| `"flex-end"` \| `"center"` \| `"baseline"` \| `"stretch"` \| `"start"` \| `"end"` |  | Grid only: inline-axis alignment of this item. |
| `overflow` | `"visible"` \| `"hidden"` |  | default `"visible"` |
| `backgroundColor` | [`Color`](#color) |  |  |
| `opacity` | `number` |  | ≥ 0, ≤ 1 |
| `borderWidth` | [`EdgesNumber`](#edgesnumber) |  |  |
| `borderColor` | [`EdgesColor`](#edgescolor) |  |  |
| `borderRadius` | [`BorderRadius`](#borderradius) |  |  |
| `borderStyle` | [`BorderStyles`](#borderstyles) |  | default `"solid"` |
| `rotate` | `number` |  | Degrees, clockwise, around the box centre. |
| `transform` | array of [`TransformOp`](#transformop) |  | Transform functions, as in CSS: the list applies right to left, after `rotate`. Not on table cells. A transformed node moves to the next page as a whole. |
| `transformOrigin` | array of [`LengthPercent`](#lengthpercent) |  | min items 2; x and y of the origin of `rotate` and `transform` within the border box. Default `["50%", "50%"]`. |
| `zIndex` | `integer` |  | ≥ -2147483648, ≤ 2147483647; Paint order among siblings: higher paints later (above); equal values keep document order. Default 0. Not on table cells. |
| `textAlign` | `"left"` \| `"center"` \| `"right"` \| `"justify"` \| `"start"` \| `"end"` |  | text nodes only; `start`/`end` follow the paragraph direction. Default `start`. |
| `lineHeight` | `number` |  | > 0; text nodes only; multiplier of the largest run size on the line |
| `maxLines` | `integer` |  | ≥ 1; text nodes only |
| `ellipsis` | `boolean` |  | text nodes only; requires maxLines |
| `hyphenate` | `boolean` |  | text nodes only |
| `lang` | `string` |  | text nodes only; BCP-47, selects hyphenation patterns |
| `whiteSpace` | `"normal"` \| `"pre"` \| `"nowrap"` \| `"pre-wrap"` \| `"pre-line"` |  | text nodes only. `normal` collapses spaces and keeps newlines as breaks (as `pre-line`); `pre-wrap` keeps spaces and tabs and wraps. |
| `textIndent` | `number` |  | text nodes only; indent of the first line in points (negative: hanging indent) |
| `direction` | `"ltr"` \| `"rtl"` \| `"auto"` |  | default `"ltr"`; text nodes only; base direction of the paragraph (`auto`: from its first strong character). Right-to-left runs are reordered with the Unicode bidi algorithm in any direction. |
| `orphans` | `integer` |  | ≥ 1, ≤ 100, default `1`; text nodes only; minimum number of lines left at the bottom of a page when the paragraph breaks |
| `widows` | `integer` |  | ≥ 1, ≤ 100, default `1`; text nodes only; minimum number of lines carried to the top of the next page when the paragraph breaks |
| `verticalAlign` | `"top"` \| `"middle"` \| `"bottom"` |  | table cells only |
| `borderCollapse` | `"collapse"` \| `"separate"` |  | default `"collapse"`; Tables only. `collapse`: adjacent cell borders merge (the wider wins). `separate`: every cell draws its own borders, background and `borderRadius`, `borderSpacing` apart. |
| `borderSpacing` | [`NonNegative`](#nonnegative) |  | Tables with `borderCollapse: separate` only: space between cells and around them, in points. Default 0. |

### TextRun

| Property | Type | Required | Notes |
|---|---|---|---|
| `text` | `string` |  | Required unless `placeholder` is set; forbidden when it is. |
| `font` | [`Id`](#id) | yes |  |
| `size` | `number` | yes | > 0, ≤ 1000 |
| `color` | [`Color`](#color) |  |  |
| `letterSpacing` | `number` |  |  |
| `decoration` | `"none"` \| `"underline"` \| `"lineThrough"` |  |  |
| `decorationColor` | [`Color`](#color) |  | Default: the run colour. |
| `decorationThickness` | `number` |  | > 0, ≤ 100; Points. Default: from the font. |
| `decorationStyle` | `"solid"` \| `"double"` \| `"dotted"` \| `"dashed"` \| `"wavy"` |  | default `"solid"` |
| `placeholder` | `"pageNumber"` \| `"totalPages"` |  |  |
| `link` | [`Link`](#link) |  | Makes the run a link; the clickable area covers the run's glyphs on each line. |

### ColumnSpec

| Property | Type | Required | Notes |
|---|---|---|---|
| `width` | `number` or `string` matching `^[0-9]+(\.[0-9]+)?fr$` or `"auto"` | yes |  |

### TableCell

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) |  |  |
| `colSpan` | `integer` |  | ≥ 1, default `1` |
| `rowSpan` | `integer` |  | ≥ 1, default `1` |
| `style` | [`Style`](#style) |  |  |
| `children` | array of [`Node`](#node) |  |  |

### TableRow

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) |  |  |
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
| `src` | [`Id`](#id) |  | Key into the render option `images`. |
| `data` | `string` |  | Inline image bytes (JPEG, PNG, GIF, WebP or SVG; the first frame of an animation), base64-encoded; a `data:<mime>;base64,` URL is also accepted. |
| `fit` | `"fill"` \| `"contain"` \| `"cover"` \| `"none"` |  | default `"fill"` |

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
| `columns` | array of [`ColumnSpec`](#columnspec) | yes | min items 1 |
| `headerRows` | `integer` |  | ≥ 0, default `0` |
| `rows` | array of [`TableRow`](#tablerow) | yes |  |

### PageBreakNode

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

### FieldNode

An interactive AcroForm field. `style` sizes and decorates its box (background, border, padding); the widget covers the content box. Without a width or height it takes an intrinsic size (text: 150 pt wide, one line, or three when multiline; checkbox and radio: `size` + 2 pt square).

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"field"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `field` | [`Field`](#field) | yes |  |

### Field

Fields with the same `name` are one field with several widgets (for example a field in a header, repeated on every page); only radio buttons may share a name with different values.

| Property | Type | Required | Notes |
|---|---|---|---|
| `name` | `string` matching `^[^.]+$` | yes | Field name (no `.`); unique in the document except for the buttons of one radio group. |
| `kind` | `"text"` \| `"checkbox"` \| `"radio"` \| `"select"` | yes |  |
| `value` | `string` |  | text: the initial text; select: the selected option (one of `options`); radio: the value this button exports when selected. |
| `checked` | `boolean` |  | default `false`; checkbox and radio: initially selected |
| `options` | array of `string` |  | min items 1; select only: the choices |
| `multiline` | `boolean` |  | default `false`; text only |
| `password` | `boolean` |  | default `false`; text only; the value is neither shown nor stored |
| `maxLength` | `integer` |  | ≥ 1; text only; maximum number of characters |
| `readOnly` | `boolean` |  | default `false` |
| `required` | `boolean` |  | default `false` |
| `font` | [`Id`](#id) |  | text and select (required there): a declared font for the value. Its glyphs for the common Latin range are embedded so edits display. |
| `size` | `number` |  | > 0, ≤ 1000, default `10`; font size; also the mark size of checkboxes and radio buttons |
| `color` | [`Color`](#color) |  | text and mark colour; default black |

### TrackBreadth

Grid track breadth: points, a percentage, "<n>fr", "auto", "min-content" or "max-content".

Type: `number` or [`Percent`](#percent) or `string` matching `^[0-9]+(\.[0-9]+)?fr$` or `"auto"` \| `"min-content"` \| `"max-content"`

### TemplateTrack

Entry of a track template: a track, or CSS `repeat()` as `{repeat, tracks}`. `auto-fill`/`auto-fit` may appear once per list, and then every track in the list needs a fixed (points or percentage) minimum or maximum.

Type: [`TrackSize`](#tracksize) or `{repeat: …}` (`integer` or `"auto-fill"` \| `"auto-fit"`)

### PageFilter

Pages that show the node, by physical page number (as the `pageNumber` placeholder). The node keeps its place in the layout on every page; it is only painted on the matching ones.

Type: `"first"` \| `"last"` \| `"notFirst"` \| `"notLast"` \| `"odd"` \| `"even"` or array of `integer`
