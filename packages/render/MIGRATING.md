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
| `wrap={false}`, `break`, `minPresenceAhead={80}` | The same, or `break="avoid"`, `break="page-before"`, `break={{ minPresenceAhead: 80 }}` | Works as is |
| `<Text render={({ pageNumber, totalPages }) => …} />` | The same, or `<PageNumber />` and `<TotalPages />` | Works as is |
| `fontWeight: 'bold'`, `flex: 1`, `paddingHorizontal`, `borderBottomColor`, `top`/`left` | The same | Works as is |
| A footer as a `View` with `fixed` and `position: 'absolute'` | `<Footer>` and `<Header>`, which line up with the page by themselves | `fixed` throws, pointing here |
| `<Link src="…">` | `href` on `Text` or `View`; `href="#id"` jumps to an element's `id` | Rename |
| Tables built from rows of `View`s | `<Table columns={['1fr', 60, 80]} headerRows={1}>` | Optional: `View` rows still work |
| `style={tw('p-4')}` with `react-pdf-tailwind` | `className="p-4"` | Built in |
| `<Svg>`, `<Canvas>` | The same | No `<Text>` inside `<Svg>` yet |

**Not available:**
- **Browser rendering:** `<PDFViewer>`, `<PDFDownloadLink>`, `BlobProvider` and `usePDF`. Render on the server and send the bytes.
- **Other output forms:** `pdf().toBuffer()`. Use `renderToBuffer`.
- **Fonts and annotations:**
  - `Font.registerEmojiSource`: there are no colour emoji yet.
  - `<Note>`.
- **`debug`:** it is ignored.

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
