# Migrating from react-pdf

@inkoxide/render implements the react-pdf component model: `Document`, `Page`, `View`, `Text`,
`Image`, `StyleSheet.create`, `Font.register` and style objects in points. Most documents move
over by changing the import. This page lists what has another name, and what is not available.

| react-pdf | @inkoxide/render | Status |
|---|---|---|
| `import … from '@react-pdf/renderer'` | `import … from '@inkoxide/render'` | Change it |
| `Document`, `Page`, `View`, `Text`, `Image`, `StyleSheet.create`, style objects in points | The same | Works as is |
| `Font.register({ family, src })` | The same, see [Fonts](GUIDE.md#fonts) | Works as is; Inter is built in |
| `Font.registerHyphenationCallback(fn)` | The same, see [hyphenation](GUIDE.md#text-hyphenation-and-direction); a `Text` can also take its own `hyphenationCallback` | Works as is |
| `renderToStream(element)` | The same, see [API](GUIDE.md#api) | Works as is |
| `wrap={false}`, `break` | The same, or `break="avoid"`, `break="page-before"` | Works as is |
| `minPresenceAhead={80}` | The same, or `break={{ minPresenceAhead: 80 }}` | Works, measured from the element's top: the room asked for includes the element's own height, where react-pdf counts it below the element. Add the element's height to keep as much room below it |
| `<Text render={({ pageNumber, totalPages }) => …} />` | The same, or `<PageNumber />` and `<TotalPages />` | Works as is |
| `fontWeight: 'bold'`, `flex: 1`, `paddingHorizontal`, `borderBottomColor` | The same | Works as is |
| `top`/`left` without `position: 'absolute'` | The same | Works, but pagination sees the element where `top` moves it: a page-filling element (`flex: 1`) moved down runs onto a new page, where react-pdf lets it overflow. Use a margin, or leave room for the offset |
| `fixed` with `position: 'absolute'` (a watermark, a page border, a footer) | The same; `<Footer>` and `<Header>` line up with the page by themselves | Works directly in a `Page`. `top`, `bottom` and the other insets are measured from the content box, inside the page padding: `bottom: 30` on a page with `paddingBottom: 65` becomes `bottom: -35`. `fixed` without `position: 'absolute'`, or inside another element, throws and points to `<Header>` and `<Footer>` |
| `<Link src="…">` | The same, or `href` on `Text` or `View`; `href="#id"` jumps to an element's `id` | Works as is; adds no colour or underline |
| Tables built from rows of `View`s | `<Table columns={['1fr', 60, 80]} headerRows={1}>` | Optional: `View` rows still work |
| `style={tw('p-4')}` with `react-pdf-tailwind` | `className="p-4"` | Works with `@inkoxide/tailwind` installed: `npm install @inkoxide/tailwind` |
| `verticalAlign: 'super'` or `'sub'` on a `Text` | The same, see [superscripts](GUIDE.md#text-superscripts-and-font-features) | Works. The baseline moves by the font's own superscript and subscript offsets, so a superscript can sit a little higher or lower; the size stays the `fontSize` you give |
| Paragraphs broken over their whole length (Knuth–Plass) | `textWrap: 'pretty'` (`text-pretty`), on a `Text`, or a `Page` for all its text | Opt in: lines break first fit by default, so a paragraph can wrap differently until you set it |
| `fontFeatureSettings: ['tnum']` or `{ tnum: 1, liga: false }` | The same, or a CSS string (`"'tnum' 1"`); `fontVariantNumeric` and Tailwind's `tabular-nums` too | Works as is |
| `debug` on `Page`, `View`, `Text` or `Image` | The same, on any element with a box, and on `Document` for every page; see [Debugging layout](GUIDE.md#debugging-layout) | Works: draws margin, border, padding and content boxes with the size in points, and the lines and runs of a `Text` |
| `float: 'left'`, `clear: 'both'` | The same, or `float-left` and `clear-both`, on children of a column `View` (or a `Page` or `Cell`) | Works, as in CSS: text after a float wraps beside it, and a box after it is narrowed to the room beside it, or moves below it when that is too narrow. Boxes and their parent never overlap a float, and the parent grows to hold its floats. A float in a row, or with `position: 'absolute'`, throws; in a box with floats, `flexGrow`, `flexBasis`, `justifyContent` and `alignContent` throw |
| `<ImageBackground src style>` | The same, with `fit` (or `objectFit`) and `objectPosition` placing the image; see [Images](GUIDE.md#images) | Works. The image fills the padding box, inside the borders, not the content box, and is clipped to it with `borderRadius`. `source`, `imageStyle` and `srcSet` throw: pass `src`, and place the image with `fit` and `objectPosition` |
| `<Svg>`, `<Canvas>` | The same; `<Text>` and `<Tspan>` inside `<Svg>` are SVG text, see [Drawing](GUIDE.md#drawing-svg-and-canvas) | Works. SVG text is real PDF text in the registered fonts; `<textPath>` and vertical text are not supported. The `<Canvas>` painter's `text` draws one line, its top at `y`: it does not wrap, and a line break in it throws. Mitred strokes default to PDF's miter limit of 10 |
| `Font.registerEmojiSource({ url, format })` | The same, or `{ builder }`; see [Fonts](GUIDE.md#fonts) | Works. The URLs must be allowed by `remoteImages`; an emoji whose image cannot be loaded is drawn by the fonts with a warning. A colour emoji font in `fallbackFonts` works too |
| `<Image>` inside `<Text>` | The same; see [Images](GUIDE.md#images) | Works. With no size, the image is 1em high and keeps its aspect ratio; give `width` or `height` in points or em, not percentages. `verticalAlign` places it (`middle`, `text-top`, `top`, a number of points…). SVG images work too |
| `<TextInput>`, `<Checkbox>`, `<Select select={[…]}>`, `<List select={[…]}>` | The same; `TextField` is another name for `TextInput`, and `options` for `select`. See [Forms](GUIDE.md#forms) | Works, with `multiSelect` on a `List`; the size, border, background and text style come from style props or classes |
| `format={{ type, param, nDec, sepComma, negStyle, currency, currencyPrepend }}` on `<TextInput>` | The same, or the readable keys `pattern`, `decimals`, `separators`, `negative` and `currencyBefore`; see [Forms](GUIDE.md#forms) | Works for every `type`, `zip4` included; on a `time`, `param` is the style, `0` to `3`. With any of these keys, the others default as before: no decimals and no thousands separator. A format with only `type` and `currency` takes the readable defaults, 2 decimals and `1,234.56`: add `nDec: 0` to keep `1250`. `value` is the raw value (`'1250'`, `'2026-10-14'`, `'18:30'`), which the field shows formatted. A PDF/A document leaves the formatting scripts out |
| `<FieldSet name="address">` | The same | Works as is, for every kind of field, and sets inside sets nest: `a.b.c` |
| `<Note>` | The same, see [Notes](GUIDE.md#notes) | Works with text children, `backgroundColor` for the colour, and `author` and `open`. It cannot go inside a `<Text>` yet |
| `<Image src="./logo.png">`, `<Image src="https://…">` | The same, with the folders or hosts allowed in the render options: `localImages: { root: './assets' }`, `remoteImages: { allow: ['https://cdn.example.com/'] }`; see [Images](GUIDE.md#images) | Allow them first: without that, a path is an image id and a URL is refused |

**Not available:**
- **Browser rendering:** `<PDFViewer>`, `<PDFDownloadLink>`, `BlobProvider` and `usePDF`. Render on the server and send the bytes.
- **Other output forms:** `pdf().toBuffer()`. Use `renderToBuffer`.
- **Document options:** `conformance` at the a and u levels (`PDF/A-2a`, `PDF/A-3u`, …), which need tagged PDF; `conformance` takes `PDF/A-1b`, `PDF/A-2b` and `PDF/A-3b`; `pdfVersion`; `onRender`.
- **Text:**
  - `hyphenationPenalty` on `Text`.
- **Layout:** `shapeOutside`: floats are rectangles.
- **Companion packages:** `@react-pdf/math`, `@react-pdf/mermaid`, `@react-pdf/svgkit` and `@react-pdf/ui` have no counterpart.

A unitless `lineHeight` spaces lines further apart in react-pdf than here, so a long document
can break onto a different number of pages. Any style key that is not supported throws, naming
the key, instead of being ignored.

## Performance

The same documents, rendered by both libraries with the same font, page size and page count:

| Document | Pages | @inkoxide/render, 1 thread | @react-pdf/renderer 4.9 |
|---|---:|---:|---:|
| Invoice (react-pdf with `react-pdf-tailwind`) | 1 | 1.0 ms | 19.2 ms |
| Invoice (react-pdf with `StyleSheet`) | 1 | 1.3 ms | 17.6 ms |
| Report: 40 sections, 1,000 table rows | 43 | 44 ms | 2,214 ms |

<sub>Apple M4 Pro, Node 24.14, median of 20 warm renders (10 for the report). On all cores the
report takes 33 ms. In a new process, the first render including loading the library takes
164 ms for the invoice against 231 ms, and 191 ms for the report against 2,407 ms.</sub>
