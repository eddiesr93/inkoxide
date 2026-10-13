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
| `debug` on `Page`, `View`, `Text` or `Image` | The same, on any element with a box, and on `Document` for every page; see [Debugging layout](GUIDE.md#debugging-layout) | Works: draws margin, border, padding and content boxes with the size in points |
| `<ImageBackground src style>` | The same, with `fit` (or `objectFit`) and `objectPosition` placing the image; see [Images](GUIDE.md#images) | Works. The image fills the padding box, inside the borders, not the content box, and is clipped to it with `borderRadius`. `source`, `imageStyle` and `srcSet` throw: pass `src`, and place the image with `fit` and `objectPosition` |
| `<Svg>`, `<Canvas>` | The same | `<Svg>` has no `<Text>` or `<Tspan>` yet. The `<Canvas>` painter draws paths, shapes, fills and strokes with transforms, opacity, clipping (`clip`) and gradients (`linearGradient`, `radialGradient`); it has no `text`, `font` or `fontSize`, and calling one throws |
| `<Image src="./logo.png">`, `<Image src="https://…">` | The same, with the folders or hosts allowed in the render options: `localImages: { root: './assets' }`, `remoteImages: { allow: ['https://cdn.example.com/'] }`; see [Images](GUIDE.md#images) | Allow them first: without that, a path is an image id and a URL is refused |

**Not available:**
- **Browser rendering:** `<PDFViewer>`, `<PDFDownloadLink>`, `BlobProvider` and `usePDF`. Render on the server and send the bytes.
- **Other output forms:** `pdf().toBuffer()`. Use `renderToBuffer`.
- **Document options:** `ownerPassword`, `userPassword` and `permissions` (encryption); `conformance` at the a and u levels (`PDF/A-2a`, `PDF/A-3u`, …), which need tagged PDF; `conformance` takes `PDF/A-1b`, `PDF/A-2b` and `PDF/A-3b`; `pdfVersion`; `onRender`.
- **Text:**
  - `verticalAlign: 'super'` and `'sub'` (`verticalAlign` is for table cells here).
  - `fontFeatureSettings`.
  - `Image` inside `Text`.
  - `Font.registerEmojiSource`: there are no colour emoji yet.
  - Knuth–Plass line breaking: lines break first-fit, so a paragraph can wrap differently.
- **Layout:** `float`, `clear` and `shapeOutside`.
- **Forms:** `<List>`, `<FieldSet>`, and `format` on a text field.
- **Annotations:** `<Note>`.
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
