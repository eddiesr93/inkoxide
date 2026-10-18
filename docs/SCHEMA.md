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
- **Numbers.** An integer may be written with a zero fraction (`400.0`),
  as JSON Schema reads it. Other numbers are read as 32-bit floats, so a
  number past a bound by less than a float's precision there is read as
  the bound and accepted (`1000000.01` where the maximum is `1000000`).
  Percentages and `fr` sizes are bounded as written: `"10000.0001%"` is
  out of range.
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
  like CSS. An image with no `width` and no `height` that is wider than
  the room it has is scaled down to fit it, keeping its aspect ratio
  (CSS would let it overflow). `fit` defaults to `fill`, like CSS `object-fit`, and
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
| `DUPLICATE_ID` | Font id declared twice, attachment `name` or XMP schema `prefix` declared twice, or form field `name` used twice (except by the buttons of one radio group), or used both for a field and for a group of fields (`a` and `a.b`) |
| `UNKNOWN_FONT` | Font id not declared in `fonts` |
| `FONT_NOT_REGISTERED` | Declared font has no registered bytes |
| `UNKNOWN_IMAGE` | Image `src` not provided in the render options |
| `INVALID_IMAGE` | Image data cannot be decoded (a JPEG is not decoded: its structure is checked) |
| `IMAGE_TOO_LARGE` | Raster image (not SVG) over 50 megapixels (width × height). An SVG has its own limit: a width or height over 100,000 px is `INVALID_IMAGE` |
| `UNKNOWN_ATTACHMENT` | An `attachment` names a file not declared in `attachments` |
| `ATTACHMENT_NOT_REGISTERED` | An attachment's `src` not provided in the render options. Only a render reports it |
| `ATTACHMENT_TOO_LARGE` | An attachment over 64 MiB, or attachments over 256 MiB together |
| `STYLE_NOT_APPLICABLE` | Style property set on a node type it does not apply to |
| `INVALID_TABLE` | Inconsistent table structure (spans, columns, header rows) |
| `INVALID_NODE` | Node type not allowed here (e.g. `pageBreak` in a header) |
| `UNKNOWN_ANCHOR` | A `#id` link names no displayed node `id` in the page body, flow or overlays (ids in headers, footers and under `display: none` are not destinations) |
| `PDFA_CONFLICT` | The document uses something the PDF/A level does not allow: at any level, a page side under 3 points (at the page's `size`); under PDF/A-1b, transparency (a colour with alpha below 1, `opacity` below 1, `debug`, an image, inline image, emoji image or background image with an alpha channel, an SVG drawn with opacity, a mask, a blend mode, an isolated group or a clip path other than one path, or a colour glyph that needs transparency: a bitmap with an alpha channel or a layer with an opacity); under PDF/A-1b, 2b and 2a, `attachments` (at `attachments[0]`). Only a render reports it, at the first such use |
| `UA_CONFLICT` | With the render option `pdfUA`: the document lacks something PDF/UA-1 requires (`meta.title`, `meta.lang`, `alt` on an image or a `figure` box, `label` on a field, headings that go down one level at a time from `h1`, a `listItem` inside a `list` and a `listBody` inside a `listItem`). Only a render reports it, at the first such thing |

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
| `fallbackFonts` | array of [`Id`](#id) |  | max items 256; Font ids tried, in order, for characters missing from a run's font. Every id must be declared in `fonts`; an id listed again is skipped. |
| `emojiImages` | `object` |  | max properties 4096; Emoji drawn as images: each key is one emoji sequence (`😀`, `👍🏽`, `🇫🇷`, `👩‍💻`; U+FE0F is ignored) of at most 64 bytes in UTF-8, each value the id of an image in the render option `images`. Where text has the sequence with emoji presentation, the image is drawn in its place, 1em high (the run's `size`), its bottom 0.1em below the run's baseline (which its `verticalAlign` raises or lowers), and the sequence is its text for copying and search. A ZWJ sequence without an image tries its parts. |
| `attachments` | array of [`AttachmentDecl`](#attachmentdecl) |  | max items 1000; Files the PDF carries, each written once: viewers list them in their attachments panel, and `attachment` nodes and runs show icons that open them. At most 1000 files, of at most 64 MiB each and 256 MiB together. Under PDF/A, only PDF/A-3b and PDF/A-3a allow them; PDF/A-1b, PDF/A-2b and PDF/A-2a refuse them (`PDFA_CONFLICT`). |
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

A development aid: draws the element's boxes on top of the page, its margin (orange), border (yellow) and padding (green) as light fills and its content box as a blue outline, with its border-box size in points as a small label. On a text node it also outlines each line: its line box (dashed), its baseline, and the glyphs of each run on it, from the run font's ascender to its descender. The label is drawn with lines, so no text is added to the document. A box split across pages is drawn on each page it crosses. It changes the output: leave it off in production.

Type: `boolean` (default `false`)

### Role

What the node is in tagged output (render option `tagged` or `pdfUA`, or PDF/A-2a or 3a): a heading (`h1` to `h6`), a paragraph, a section, a quote, a caption, a figure, a list (`list`, whose items are `listItem` boxes, each with a `label` and a `listBody`), or decoration (`artifact`), which is not read. Without tagging it does nothing. A text takes `h1`–`h6`, `p`, `caption`, `label`, `blockQuote` and `artifact`; an image `figure` and `artifact`; a box any. By default a text is a paragraph, an image a figure, and a box is not an element: its children belong to its parent. Elements are read in document order, whatever `zIndex`, floats or positioning draw first.

Type: `"h1"` \| `"h2"` \| `"h3"` \| `"h4"` \| `"h5"` \| `"h6"` \| `"p"` \| `"section"` \| `"blockQuote"` \| `"caption"` \| `"figure"` \| `"list"` \| `"listItem"` \| `"label"` \| `"listBody"` \| `"artifact"`

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
| `pageMode` | `"useNone"` \| `"useOutlines"` \| `"useThumbs"` \| `"fullScreen"` \| `"useAttachments"` |  | What the viewer shows when it opens the document: no panel (`useNone`), the bookmarks (`useOutlines`), page thumbnails (`useThumbs`), the pages full screen (`fullScreen`) or the attachments (`useAttachments`). Default: `useOutlines` when the document has bookmarks, else `useNone`. |
| `pageLayout` | `"singlePage"` \| `"oneColumn"` \| `"twoColumnLeft"` \| `"twoColumnRight"` \| `"twoPageLeft"` \| `"twoPageRight"` |  | How the viewer lays out the pages: one at a time (`singlePage`), one scrolling column (`oneColumn`), two scrolling columns with odd pages on the left or right (`twoColumnLeft`, `twoColumnRight`), or two pages at a time with odd pages on the left or right (`twoPageLeft`, `twoPageRight`). Default: the viewer's own setting. |
| `xmpExtensions` | array of [`XmpSchema`](#xmpschema) |  | max items 16; XMP schemas the PDF/A metadata declares and fills in, such as Factur-X's (`fx`) for an e-invoice: PDF/A accepts properties outside its own schemas only when they are declared, which these are, as Text. At most 16 schemas of 64 properties. Without PDF/A there is no XMP to put them in, and `render` refuses them. |

### FontDecl

| Property | Type | Required | Notes |
|---|---|---|---|
| `id` | [`Id`](#id) | yes |  |
| `family` | `string` | yes | min length 1; The family name. Text runs name fonts by `id`; text in SVG images is matched to the declared fonts by its `font-family` against this (ignoring ASCII case), then `weight` and `style`, as CSS matches fonts. |
| `weight` | `integer` |  | ≥ 1, ≤ 1000, default `400`; The weight SVG text's `font-weight` is matched against. |
| `style` | `"normal"` \| `"italic"` |  | default `"normal"`; The style SVG text's `font-style` is matched against. |
| `source` | [`Id`](#id) |  | The registered font it draws with; `id` when absent. One variable font can then serve several declarations, each with its own `variations`. |
| `variations` | `object` |  | max properties 16; A variable font's axis values, by tag, as CSS `font-variation-settings` (`{"wght": 650, "wdth": 75}`): text in this font is drawn and measured at that instance, and the PDF embeds the instance, never the variable font. A value is clamped to its axis; an axis the font lacks is ignored, with a warning, as every axis of a static font is. Tags are four printable ASCII characters; at most 16 axes, and at most 64 declarations with `variations` in a document. |

### NonNegative

A non-negative number of at most 1000000.

Type: `number` (≥ 0, ≤ 1000000)

### Percent

A percentage within ±10000%, like `"50%"`.

Type: `string` matching `^-?0*([0-9]{1,4}(\.[0-9]+)?|10000(\.0+)?)%$`

### NonNegativePercent

A percentage within [0%, 10000%], like `"50%"`.

Type: `string` matching `^(0*([0-9]{1,4}(\.[0-9]+)?|10000(\.0+)?)|-0+(\.0+)?)%$`

### Fr

A flexible size, a share of the free space, of at most 1000000fr, like `"1fr"`.

Type: `string` matching `^0*([0-9]{1,6}(\.[0-9]+)?|1000000(\.0+)?)fr$`

### Length

Points (within ±1000000), a percentage of the containing block (within ±10000%), or "auto". A percentage height needs a containing block with a definite height, as in CSS: a child of the page takes it of the page content height; in a header or footer, or in a box whose height is not set, it is `auto`.

Type: `number` (≥ -1000000, ≤ 1000000) or [`Percent`](#percent) or `"auto"`

### LengthPercent

Points (within ±1000000) or a percentage of the containing block's width (within ±10000%). In `gap`, `rowGap` and `columnGap`, a percentage is of the box's own content box instead, as in CSS: a column gap of its width, a row gap of its height.

Type: `number` (≥ -1000000, ≤ 1000000) or [`Percent`](#percent)

### NonNegativeLength

A `Length` that is not negative: points (at most 1000000), a percentage of the containing block (at most 10000%), or "auto". A percentage height needs a containing block with a definite height, as for `Length`.

Type: `number` (≥ 0, ≤ 1000000) or [`NonNegativePercent`](#nonnegativepercent) or `"auto"`

### NonNegativeLengthPercent

A `LengthPercent` that is not negative: points (at most 1000000) or a percentage (at most 10000%), of what `LengthPercent` says.

Type: `number` (≥ 0, ≤ 1000000) or [`NonNegativePercent`](#nonnegativepercent)

### Color

[r, g, b, a], each in [0, 1].

Type: array of `number` (each ≥ 0, ≤ 1) (exactly 4 items)

### Size

Page size in points. PDF/A, at any level, takes sides of 3 points or more: a render of a smaller page as PDF/A fails with `PDFA_CONFLICT` at the page's `size`.

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

Padding: a `NonNegativeLengthPercent` per side.

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  |  |
| `right` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  |  |
| `bottom` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  |  |
| `left` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  |  |

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
| `background` | [`Color`](#color) |  | Fills the whole page behind its content. The fill is opaque: the alpha of the colour is ignored, so `[1, 0, 0, 0]` paints solid red. |
| `debug` | [`Debug`](#debug) |  | Draws the boxes of the page (its margins and content area) and of every element on it, the header and footer included. |
| `header` | [`Node`](#node) |  | Laid out at the full page width, once, or again on every page when it shows page numbers, and painted at the top of every physical page (y = 0). Use padding to inset it. Size the top margin to leave room for it. `break` (other than its defaults) and `pageBreak` are not allowed in it, at any depth. |
| `footer` | [`Node`](#node) |  | Laid out at the full page width, once, or again on every page when it shows page numbers, and painted with its bottom edge on the bottom of every physical page. `break` (other than its defaults) and `pageBreak` are not allowed in it, at any depth. |
| `children` | array of [`Node`](#node) |  |  |

### Break

Pagination controls. They apply to flowing content only: on a header or footer, on a node in a table cell, or on any node inside these, a `break` with a value other than the defaults is `INVALID_VALUE`. For fixed overlays, see `fixed`. A box laid out as a row, a grid, wrapping or reversed moves to the next page whole when it fits on one; taller than a page, it splits between its rows (children whose extents overlap), each child continuing from the row's top. Content that cannot split (an image, an absolutely positioned, rotated, transformed or fixed node, table rows tied by a row span) is cut across pages when it is taller than a page.

| Property | Type | Required | Notes |
|---|---|---|---|
| `before` | `"auto"` \| `"page"` |  | default `"auto"`; `page` starts a new page before the node, unless it is at the top of one already. `INVALID_VALUE` inside a row, grid, wrapping or reversed box, or inside an absolutely positioned, rotated, transformed, floated or fixed node, or on an absolutely positioned or floated node: there it would do nothing. |
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

Type: [`TrackBreadth`](#trackbreadth) or `{min, max}` (`min`: [`TrackBreadth`](#trackbreadth); `max`: [`TrackBreadth`](#trackbreadth)) or `{fitContent}` (`fitContent`: [`NonNegativeLengthPercent`](#nonnegativelengthpercent))

### BorderRadius

One radius for every corner, or one per corner (a missing corner is square). Radii that would overlap are scaled down together, as in CSS.

Type: [`NonNegative`](#nonnegative) or object with `topLeft`, `topRight`, `bottomRight`, `bottomLeft`

### BorderStyles

One style for every side, or one per side (an unset side is solid).

Type: `"solid"` \| `"dashed"` \| `"dotted"` or object with `top`, `right`, `bottom`, `left`

### TransformOp

One transform function, as an object with exactly one key. Lengths are points or percentages of the node's own border box; angles are degrees, clockwise. Scale factors, `rotate` and matrix entries are within ±1000000; `skew` angles are within (−90, 90).

Type: `{translate}` (`translate`: array of [`LengthPercent`](#lengthpercent) (exactly 2 items)) or `{scale}` (`scale`: array of `number` (each ≥ -1000000, ≤ 1000000) (exactly 2 items)) or `{rotate}` (`rotate`: `number` (≥ -1000000, ≤ 1000000)) or `{skew}` (`skew`: array of `number` (each > -90, < 90) (exactly 2 items)) or `{matrix}` (`matrix`: array of `number` (each ≥ -1000000, ≤ 1000000) (exactly 6 items))

### ShapeOutside

A float's shape, as an object with exactly one key, in its margin box: lengths are points, percentages are of the margin box's width (`x`, `rx`, `left`, `right`) or height (`y`, `ry`, `top`, `bottom`). The shape is clipped to the margin box.

Type: `{circle}` (`circle`: [`ShapeCircle`](#shapecircle)) or `{ellipse}` (`ellipse`: [`ShapeEllipse`](#shapeellipse)) or `{inset}` (`inset`: [`ShapeInset`](#shapeinset)) or `{polygon}` (`polygon`: array of array of [`LengthPercent`](#lengthpercent) (each exactly 2 items) (min items 3, max items 1000))

### ShapeCircle

CSS `circle()`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `radius` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  | A percentage is of the margin box's diagonal over √2, as in CSS. By default, to the margin box's closest side. |
| `at` | array of [`LengthPercent`](#lengthpercent) |  | exactly 2 items; The centre, [x, y]. By default, the middle of the margin box. |

### ShapeEllipse

CSS `ellipse()`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `rx` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  | The horizontal radius. By default, to the closest of the left and right sides. |
| `ry` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  | The vertical radius. By default, to the closest of the top and bottom sides. |
| `at` | array of [`LengthPercent`](#lengthpercent) |  | exactly 2 items; The centre, [x, y]. By default, the middle of the margin box. |

### ShapeInset

CSS `inset()`: a rectangle inset from the margin box's sides, by 0 on a side not set.

| Property | Type | Required | Notes |
|---|---|---|---|
| `top` | [`LengthPercent`](#lengthpercent) |  |  |
| `right` | [`LengthPercent`](#lengthpercent) |  |  |
| `bottom` | [`LengthPercent`](#lengthpercent) |  |  |
| `left` | [`LengthPercent`](#lengthpercent) |  |  |
| `radius` | [`NonNegative`](#nonnegative) |  | default `0`; Points the corners are rounded by; at most half the rectangle's width and height. |

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
| `flexBasis` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `gap` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  | Sets both rowGap and columnGap; either of those, when present, wins. |
| `rowGap` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  | A percentage is of the box's own content height, as in CSS. In a box whose height is not set, it is of the height of the content without the gaps, so it adds little: use points. |
| `columnGap` | [`NonNegativeLengthPercent`](#nonnegativelengthpercent) |  |  |
| `width` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `height` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `minWidth` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `minHeight` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `maxWidth` | [`NonNegativeLength`](#nonnegativelength) |  |  |
| `maxHeight` | [`NonNegativeLength`](#nonnegativelength) |  |  |
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
| `float` | `"left"` \| `"right"` \| `"none"` |  | default `"none"`; Children of a column box (not of a row, grid, wrapping or reversed box, and not a table cell itself) only; not with `position: absolute`. Floats the node to that side of its parent's content box, shrink-to-fit, at or below where it occurs, as in CSS. Lines of text after it wrap beside it: beside its margin box, or around its `shapeOutside`. A column box after it with children and no floats of its own, without `overflow: hidden` or `aspectRatio`, keeps its width and lays its children out beside it, so its lines wrap beside it too (to a depth of 8 such boxes). Other boxes after it are narrowed to the room beside its margin box, or move below it when their min-content width does not fit there, as CSS does with boxes that make a block formatting context. The parent's height includes its floats, and the room the floats leave goes to `flexGrow` and `justifyContent` as in a column box. A float takes no `flexGrow`; its parent takes no `alignContent`, and the parent's children no `flexBasis`. At most 1000 floated children per box. A float forces no page break: `break.before` or `break.after` `"page"` on it, or on a node inside it, and a `pageBreak` node inside it, are rejected. A float moves to the next page with the lines beside it, and is sliced when taller than a page. |
| `clear` | `"left"` \| `"right"` \| `"both"` \| `"none"` |  | default `"none"`; Puts the node's border box below the margin boxes of the earlier floats of its parent on these sides. |
| `shapeOutside` | [`ShapeOutside`](#shapeoutside) |  | Floats only. The shape lines beside the float wrap around, instead of its margin box, as CSS `shape-outside` with a basic shape: a line takes the room of the shape's widest part beside it, so lines come closer where the shape narrows. Only lines: the float is placed, and boxes beside it are narrowed, by its margin box. The shape is followed in strips at least 2 points high, at most 128 a float; past 4096 strips in one box, a float's lines wrap beside its margin box. |
| `shapeMargin` | [`NonNegative`](#nonnegative) |  | default `0`; Floats with a `shapeOutside` only. Points the lines beside the float keep from its shape, within its margin box. |
| `backgroundColor` | [`Color`](#color) |  |  |
| `opacity` | `number` |  | ≥ 0, ≤ 1; Multiplies the alpha of what the node paints and of everything inside it, element by element: unlike CSS, it does not fade the node and its children as one group, so overlapping children show through each other. `0` draws nothing for the node's children and leaves out the links, form fields and notes inside them. |
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
| `hyphenate` | `boolean` |  | text nodes only. Hyphenates the words in the language of the node (`lang`, or `meta.lang`); with no language, nothing is hyphenated and the render logs a warning. A word wider than its line breaks between any two characters, hyphenated or not. |
| `lang` | `string` |  | text nodes only; BCP-47, selects hyphenation patterns |
| `whiteSpace` | `"normal"` \| `"pre"` \| `"nowrap"` \| `"pre-wrap"` \| `"pre-line"` |  | text nodes only. `normal` collapses spaces and keeps newlines as breaks (as `pre-line`); `pre-wrap` keeps spaces and tabs and wraps. As in CSS, only spaces, tabs and line breaks collapse: other space characters (thin, em, figure, no-break, …) keep their width, and only the breaking ones are break opportunities. `pre` and `pre-wrap` advance a tab to the next tab stop (CSS `tab-size: 8`): stops are 8 spaces of the tab's font apart, from the start of the line (a line `pre-wrap` wraps keeps the stops of the line it continues), and a tab narrower than half a `0` goes on to the next stop. |
| `textWrap` | `"wrap"` \| `"pretty"` \| `"balance"` |  | default `"wrap"`; text nodes only. How lines break: `wrap` fills each line in turn (first fit); `pretty` chooses the breaks over each stretch of the paragraph between forced breaks to make its lines even (Knuth and Plass's total fit, as TeX), with fewer hyphens, and lets the spaces of justified lines shrink by up to a third. A stretch with more than 20 000 break opportunities, or past 512 open breakpoints or 2 097 152 lines tried in the paragraph, falls back to `wrap`, as do `pre` and `nowrap` text, justified text without spaces, and a word wider than its line. `balance` (for headings and captions) breaks each stretch first fit into as many lines as `wrap` does, in the narrowest width that keeps that many, so its lines are of about the same length; a stretch of one line or more than 10, beside a float, or with a word wider than its line breaks as `wrap`. |
| `hyphenationPenalty` | `number` |  | ≥ 0, ≤ 10000, default `50`; text nodes only. How much `textWrap: pretty` avoids breaking at a hyphenation point (a soft hyphen, or one `hyphenate` finds): the demerits of such a break, as TeX's `\hyphenpenalty`. 0 hyphenates as readily as it breaks at a space; higher values give fewer hyphens and looser lines. From 10 000, a hyphenation point is no break, whatever `textWrap`; below it, `wrap` and `balance` ignore it. |
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
| `text` | `string` |  | Required unless `placeholder`, `image` or `note` is set; forbidden when one is. |
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
| `verticalAlign` | `"baseline"` \| `"super"` \| `"sub"` \| `"middle"` \| `"textTop"` \| `"textBottom"` \| `"top"` \| `"bottom"` or `number` (≥ -14400, ≤ 14400) |  | default `"baseline"`; Moves the run's baseline: `super` raises it and `sub` lowers it, by the offsets the font gives for superscripts and subscripts (OS/2), for text of the run's `size`. The size stays the run's own: give a superscript a smaller `size`. The line grows to fit the moved glyphs. An `image` run also takes `middle` (its middle at the baseline plus half the x-height of the run's font), `textTop` and `textBottom` (its top at the font's ascender, its bottom at its descender), `top` and `bottom` (its top or bottom at the line's), and a number: points its bottom sits above the baseline (negative: below). |
| `fontFeatures` | `object` |  | max properties 32; OpenType features by tag, as CSS `font-feature-settings`: 1 turns a feature on, 0 off, and a larger number picks an alternate (`{"tnum": 1, "liga": 0}`). Tags are four printable ASCII characters, and a value is 0 to 65535. A run may have at most 32 features, and a document at most 64 different sets of features. |
| `image` | [`InlineImage`](#inlineimage) |  |  |
| `note` | [`Note`](#note) |  | A note in the line, in place of text: it takes no room, and its icon (20 pt square, the same at any zoom) hangs from the top of the line where the run is, on the page where the line starts. The run's `font` and `size` are the line's strut. Line breaking keeps it with the character before it, or after a space, the one after it. |
| `attachment` | [`Attachment`](#attachment) |  | An attachment's icon in the line, in place of text, as a `note` is: it takes no room, and its icon hangs from the top of the line where the run is. |

### InlineImage

An image in a line of text, as wide as `width` and as high as `height`, in points. With one of them, the other follows the image's aspect ratio; with neither, it is as high as the run's `size` (1em). The run's `font` and `size` still set the line's strut, and its `verticalAlign` places the image.

| Property | Type | Required | Notes |
|---|---|---|---|
| `src` | [`Id`](#id) |  | Key into the render option `images`. |
| `data` | `string` |  | Inline image bytes (JPEG, PNG, GIF, WebP or SVG; the first frame of an animation), base64-encoded; a `data:<mime>;base64,` URL is also accepted. |
| `width` | `number` |  | > 0, ≤ 14400 |
| `height` | `number` |  | > 0, ≤ 14400 |
| `fit` | `"fill"` \| `"contain"` \| `"cover"` \| `"none"` |  | default `"fill"` |
| `objectPosition` | [`ObjectPosition`](#objectposition) |  | Where the image sits in its box with `fit` `contain`, `cover` or `none` (CSS `object-position`); `fill` ignores it. Default centred. |
| `alt` | `string` |  | max length 1024; Alternative text: what copying, searching and screen readers get for the image. In tagged output, `""` makes the image decoration; PDF/UA requires `alt` on every image that is not. |

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
| `style` | [`Style`](#style) |  | The table places and sizes the cell: its columns set the width, and `height` and `minHeight` are minimums (the row grows to fit the content). The cell's own placement, size and effects are `STYLE_NOT_APPLICABLE`: `width`, `minWidth`, `maxWidth`, `maxHeight`, `margin`, `position`, `inset`, `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gridRow`, `gridColumn`, `gridArea`, `overflow`, `opacity`, `rotate`, `transform`, `transformOrigin`, `zIndex`, `float` and `clear`. |
| `children` | array of [`Node`](#node) |  |  |
| `header` | `boolean` |  | default `false`; In tagged output, a header of its row (`TH`, scope row); the cells of the header rows are headers of their columns. |

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
| `type` | `"box"` \| `"text"` \| `"image"` \| `"table"` \| `"pageBreak"` \| `"spacer"` \| `"field"` \| `"note"` \| `"attachment"` | yes |  |

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
| `role` | [`Role`](#role) |  |  |
| `alt` | `string` |  | max length 1024; A `figure` box's alternative text: what screen readers say for it. Only with the role `figure`. |
| `children` | array of [`Node`](#node) |  |  |
| `backgroundImage` | [`BackgroundImage`](#backgroundimage) |  |  |

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
| `role` | [`Role`](#role) |  |  |
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
| `role` | [`Role`](#role) |  |  |
| `src` | [`Id`](#id) |  | Key into the render option `images`. |
| `data` | `string` |  | Inline image bytes (JPEG, PNG, GIF, WebP or SVG; the first frame of an animation), base64-encoded; a `data:<mime>;base64,` URL is also accepted. |
| `fit` | `"fill"` \| `"contain"` \| `"cover"` \| `"none"` |  | default `"fill"` |
| `objectPosition` | [`ObjectPosition`](#objectposition) |  | Where the image sits in its box with `fit` `contain`, `cover` or `none` (CSS `object-position`); `fill` ignores it. Default centred. |
| `alt` | `string` |  | max length 1024; Alternative text: what copying, searching and screen readers get for the image. In tagged output, `""` makes the image decoration; PDF/UA requires `alt` on every image that is not. |

### ObjectPosition

Position of a fitted image in its box, per axis: a keyword, points from the left or top edge, or a percentage (that point of the image goes to that point of the box: `"0%"` is the start edge, `"100%"` the end edge). Points are within ±14400 and percentages within ±10000%. What falls outside the box is clipped.

| Property | Type | Required | Notes |
|---|---|---|---|
| `x` | `"left"` \| `"center"` \| `"right"` or `number` (≥ -14400, ≤ 14400) or [`Percent`](#percent) |  | default `"center"` |
| `y` | `"top"` \| `"center"` \| `"bottom"` or `number` (≥ -14400, ≤ 14400) or [`Percent`](#percent) |  | default `"center"` |

### BackgroundImage

An image painted behind the box's children and over its `backgroundColor`, under its borders. It fills the padding box (inside the borders), placed by `fit` and `objectPosition`, and is clipped to that box, rounded by `borderRadius`. A box split across pages is sliced, like its background colour.

| Property | Type | Required | Notes |
|---|---|---|---|
| `src` | [`Id`](#id) |  | Key into the render option `images`. |
| `data` | `string` |  | Inline image bytes (JPEG, PNG, GIF, WebP or SVG; the first frame of an animation), base64-encoded; a `data:<mime>;base64,` URL is also accepted. |
| `fit` | `"fill"` \| `"contain"` \| `"cover"` \| `"none"` |  | default `"fill"` |
| `objectPosition` | [`ObjectPosition`](#objectposition) |  | Where the image sits in the padding box with `fit` `contain`, `cover` or `none` (CSS `object-position`); `fill` ignores it. Default centred. |

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

Starts a new page when in-flow content follows it: at the end of the flow it adds no page. Flowing content only: in a header, a footer, a table cell or a fixed overlay, and inside a row, grid, wrapping or reversed box, or inside an absolutely positioned, rotated, transformed, floated or fixed node, it is `INVALID_NODE`.

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

An interactive AcroForm field. `style` sizes and decorates its box (background, border, padding); the widget covers the content box. Without a width or height it takes an intrinsic size (text and select: 150 pt wide, one line, or three when multiline; list: 150 pt wide, one row per option up to four; checkbox and radio: `size` + 2 pt square).

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
| `name` | `string` matching `^[^.]{1,128}(\.[^.]{1,128}){0,31}$` | yes | min length 1, max length 512; Field name; unique in the document except for the buttons of one radio group. Dots make groups: `address.street` is the field `street` in the group `address`, as in PDF's own field names. At most 32 parts of 1 to 128 characters, and 512 characters in all. A name used again, or used both for a field and for a group (`a` and `a.b`), is `DUPLICATE_ID`. |
| `kind` | `"text"` \| `"checkbox"` \| `"radio"` \| `"select"` \| `"list"` | yes | `select` is a drop-down list, `list` a list box that shows several options at once |
| `value` | `string` (max length 8000) or array of `string` (each max length 1000) (max items 1000) |  | text: the initial text, at most `maxLength` characters (`OUT_OF_RANGE`); select and list: the selected option (one of `options`), or for a list with `multiSelect` an array of them, each at most once; radio: the value this button exports when selected, required and non-empty (`MISSING_FIELD`). A string is at most 8000 characters. |
| `checked` | `boolean` |  | default `false`; checkbox and radio: initially selected; at most one button of a radio group (`INVALID_VALUE`), and buttons of the group with the same value are selected together |
| `options` | array of `string` (each max length 1000) |  | min items 1, max items 1000; select and list only (required there): the choices, at most 1000 of at most 1000 characters each |
| `multiSelect` | `boolean` |  | default `false`; list only: several options can be selected at once |
| `multiline` | `boolean` |  | default `false`; text only |
| `password` | `boolean` |  | default `false`; text only; the value is neither shown nor stored |
| `maxLength` | `integer` |  | ≥ 1, ≤ 4294967295; text only; maximum number of characters |
| `readOnly` | `boolean` |  | default `false` |
| `required` | `boolean` |  | default `false` |
| `font` | [`Id`](#id) |  | text, select and list (required there): a declared font for the value, with outlines (a font with colour bitmaps only is `INVALID_VALUE`). Its glyphs for the common Latin range are embedded so edits display. The value and the options are drawn with this font only, a glyph per character: no fallback fonts, no shaping and no right-to-left ordering, as viewers redraw them. A character the font has no glyph for is `INVALID_VALUE` at the value or the option. |
| `size` | `number` |  | > 0, ≤ 1000, default `10`; font size; also the mark size of checkboxes and radio buttons |
| `color` | [`Color`](#color) |  | text and mark colour; default black |
| `format` | [`FieldFormat`](#fieldformat) |  |  |
| `label` | `string` |  | min length 1, max length 1024; What the field is for, which screen readers say and viewers show as its tooltip (`/TU`). PDF/UA requires it. |

### FieldFormat

Single-line, non-password text fields only: how the field shows and takes its value. The field shows `value` formatted; viewers format what the user types with the standard form scripts (`AFNumber_Format` and the like), which PDF/A leaves out (the formatted text and value stay). `value` must fit the format (`INVALID_VALUE`): a number or percent is `-?digits[.digits]` (at most 15 digits on each side; a percent is a fraction, `0.125` shows `12.50%`), a date `YYYY-MM-DD` with an optional `THH:MM[:SS]`, a time `HH:MM[:SS]` or a date with a time, `zip` 5 digits, `zipPlus4` 9, `phone` 10 and `ssn` 9. A key that does not apply to the `type` is `INVALID_VALUE`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"number"` \| `"percent"` \| `"date"` \| `"time"` \| `"zip"` \| `"zipPlus4"` \| `"phone"` \| `"ssn"` | yes |  |
| `decimals` | `integer` |  | ≥ 0, ≤ 10; number and percent: digits after the decimal separator, rounded half away from zero; default 2 |
| `separators` | `"1,234.56"` \| `"1234.56"` \| `"1.234,56"` \| `"1234,56"` |  | number and percent: the thousands and decimal separators, by example; default `1,234.56` |
| `negative` | `"minus"` \| `"red"` \| `"parens"` \| `"parensRed"` |  | number: how a negative number shows: `-1.00`, `1.00` in red, `(1.00)` or `(1.00)` in red; default `minus` |
| `currency` | `string` matching `^[^\u0000-\u001f\u007f-\u009f"'\\\u2028\u2029]*$` |  | max length 8; number: a currency symbol, shown as it is (add a space to separate it); no control characters, quotes or backslashes |
| `currencyBefore` | `boolean` |  | default `true`; number: the currency goes before the number |
| `pattern` | `string` |  | min length 1, max length 64; date and time (required there): how the value shows, with `d`, `dd`, `ddd` (Mon), `dddd` (Monday), `m`, `mm`, `mmm` (Jan), `mmmm` (January), `yy`, `yyyy`, `H`, `HH` (0–23), `h`, `hh` (1–12), `M`, `MM` (minutes), `s`, `ss`, `t` (a/p), `tt` (am/pm) and the characters ` .,/:-`; a time shows only `H`, `h`, `M`, `s` and `t`. Names are English. |

### NoteNode

A text annotation: a sticky-note icon that opens its text in the viewer. It takes no space; its icon (20 pt square, the same at any zoom) sits at the top-left corner of its box, inside the page, on the page where the box starts. Under a transform, the icon stays upright and the same size: only its corner moves. In a header, a footer or a fixed element it is on every page.

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"note"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `note` | [`Note`](#note) | yes |  |

### Note

| Property | Type | Required | Notes |
|---|---|---|---|
| `contents` | `string` | yes | min length 1, max length 8000; The text of the note. |
| `author` | `string` |  | max length 256; Who wrote it; viewers show it as the note's title. |
| `color` | [`Color`](#color) |  | The icon's colour, also the colour viewers give the note (its alpha is not used); default yellow, [1, 0.82, 0.2, 1]. |
| `open` | `boolean` |  | default `false`; Whether viewers show the text when the document opens. |

### AttachmentNode

An icon that opens a file of `attachments` (a file attachment annotation). It takes no space, and sits as a note does: at the top-left corner of its box, inside the page, on the page where the box starts, 20 pt square at any zoom. In a header, a footer or a fixed element it is on every page, each icon opening the one file.

| Property | Type | Required | Notes |
|---|---|---|---|
| `type` | `"attachment"` | yes |  |
| `id` | [`Id`](#id) |  |  |
| `style` | [`Style`](#style) |  |  |
| `break` | [`Break`](#break) |  |  |
| `pages` | [`PageFilter`](#pagefilter) |  |  |
| `debug` | [`Debug`](#debug) |  |  |
| `attachment` | [`Attachment`](#attachment) | yes |  |

### Attachment

| Property | Type | Required | Notes |
|---|---|---|---|
| `file` | `string` | yes | min length 1, max length 255; The `name` of a file in `attachments`. |
| `icon` | `"pushPin"` \| `"paperclip"` \| `"graph"` \| `"tag"` |  | default `"pushPin"`; The icon: a push pin, a paper clip, a bar chart or a tag. |
| `color` | [`Color`](#color) |  | The icon's colour (its alpha is not used); default blue, [0.3, 0.55, 0.9, 1]. |
| `contents` | `string` |  | min length 1, max length 8000; The text viewers show for the icon; default the file's `description`, else its name. |

### AttachmentDecl

A file the PDF carries. Exactly one of `src` and `data`.

| Property | Type | Required | Notes |
|---|---|---|---|
| `name` | `string` matching `^[^/\\\u0000-\u001f\u007f-\u009f]+$` | yes | min length 1, max length 255; The file name viewers show and save it under, unique in the document: no `/`, `\\` or control characters. `attachment` icons name the file by it. |
| `src` | [`Id`](#id) |  | Key into the render option `attachments`. |
| `data` | `string` |  | The file's bytes, base64-encoded. |
| `mimeType` | `string` matching `^[A-Za-z0-9][A-Za-z0-9!#$&^_.+-]{0,62}/[A-Za-z0-9][A-Za-z0-9!#$&^_.+-]{0,62}$` |  | max length 127, default `"application/octet-stream"`; The file's MIME type (`text/xml`, `application/pdf`). |
| `description` | `string` |  | max length 8000; What the file is; viewers show it in their attachments panel. |
| `relationship` | `"source"` \| `"data"` \| `"alternative"` \| `"supplement"` \| `"unspecified"` |  | default `"unspecified"`; How the file relates to the document (`/AFRelationship`, which PDF/A-3 requires): the original it was made from (`source`), the data it shows (`data`, as an e-invoice's XML), another form of the same content (`alternative`), or something that adds to it (`supplement`). |
| `modified` | `string` |  | When the file was last changed (RFC 3339). Default: the creation date, when there is one; otherwise none is written. |

### XmpSchema

| Property | Type | Required | Notes |
|---|---|---|---|
| `schema` | `string` | yes | min length 1, max length 512; What the schema is (`Factur-X PDFA Extension Schema`). |
| `namespace` | `string` matching `^\S+$` | yes | min length 1, max length 512; The namespace URI of its properties. |
| `prefix` | `string` matching `^[A-Za-z_][A-Za-z0-9_.-]*$` | yes | The prefix of its properties (`fx`): not one the metadata uses already (`pdfaid`, `dc`, `xmp`, `pdf`, `rdf`, `x`, `xml`, `xmlns`, `pdfaExtension`, `pdfaSchema`, `pdfaProperty`, `pdfuaid`). |
| `properties` | array of [`XmpProperty`](#xmpproperty) | yes | max items 64 |

### XmpProperty

| Property | Type | Required | Notes |
|---|---|---|---|
| `name` | `string` matching `^[A-Za-z_][A-Za-z0-9_.-]*$` | yes | max length 512; The property name (`DocumentType`). |
| `value` | `string` | yes | max length 8000; Its value, a Text. |
| `description` | `string` | yes | min length 1, max length 512; What the property is. |

### TrackBreadth

Grid track breadth: points, a percentage, "<n>fr", "auto", "min-content" or "max-content". Points and fr are at most 1000000.

Type: `number` (≥ 0, ≤ 1000000) or [`NonNegativePercent`](#nonnegativepercent) or [`Fr`](#fr) or `"auto"` \| `"min-content"` \| `"max-content"`

### TemplateTrack

Entry of a track template: a track, or CSS `repeat()` as `{repeat, tracks}`. `auto-fill`/`auto-fit` may appear once per list, and then every track in the list needs a fixed (points or percentage) minimum or maximum.

Type: [`TrackSize`](#tracksize) or `{repeat, tracks}` (`repeat`: `integer` (≥ 1, ≤ 1000) or `"auto-fill"` \| `"auto-fit"`; `tracks`: array of [`TrackSize`](#tracksize) (min items 1))

### PageFilter

Pages that show the node, by physical page number (as the `pageNumber` placeholder). The node keeps its place in the layout on every page; it is only painted on the matching ones.

Type: `"first"` \| `"last"` \| `"notFirst"` \| `"notLast"` \| `"odd"` \| `"even"` or array of `integer` (each ≥ -2147483648, ≤ 2147483647, ≠ 0) (min items 1)
