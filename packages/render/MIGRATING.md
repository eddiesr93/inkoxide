# Migrating from react-pdf

@inkoxide/render has the components, style objects and functions of react-pdf: `Document`,
`Page`, `View`, `Text`, `Image`, `StyleSheet.create`, `Font.register` and styles in points.
Most documents move over in three steps: install the package, change the import, render. Then
check the few places where a document can come out differently, listed below.

You will not miss anything silently: a style key, a prop or a component that @inkoxide/render
does not support fails the render with an error that names it and the component it is on,
where react-pdf would ignore it. The first render tells you what is left to change.

## 1. Install

```sh
npm uninstall @react-pdf/renderer
npm install @inkoxide/render
```

- Styled with `react-pdf-tailwind`? Also `npm install @inkoxide/tailwind`, and write the classes
  as `className`.
- Rendering in the browser? Also `npm install @inkoxide/wasm`; see [In the browser](#in-the-browser).

## 2. Change the import

Replace `@react-pdf/renderer` with `@inkoxide/render` in every file:

```diff
- import { Document, Font, Page, StyleSheet, Text, View } from '@react-pdf/renderer';
+ import { Document, Font, Page, StyleSheet, Text, View } from '@inkoxide/render';
```

The components and `StyleSheet.create` keep their names and props. Your fonts keep working:
`Font.register({ family, src })` takes the same arguments, and Inter is built in.

## 3. Render

`renderToStream` is the same, and `pdf(element).toBuffer()` becomes `renderToBuffer`:

```diff
- const buffer = await pdf(<Invoice />).toBuffer();
+ const buffer = await renderToBuffer(<Invoice />);
```

`renderToFile(element, path)` writes the PDF to a file. See [API](GUIDE.md#api).

## 4. Fix what the first render reports

These are the errors a react-pdf document is most likely to meet, and what to write instead:

| What throws | What to write instead |
|---|---|
| A style key that is not supported, or a prop the element does not take (`href` or `bookmark` on a `Cell` or a `Header`, `className` or `style` on an SVG shape) | Remove it; the error names the key or the prop |
| `<Page wrap={false}>` | Nothing: a page's content always flows onto more pages. Use `wrap={false}` on the elements inside it |
| `fixed` without `position: 'absolute'`, or not directly in a `Page` | `<Header>` or `<Footer>`, which repeat on every page in its margins |
| `source`, `imageStyle` or `srcSet` on `<ImageBackground>` | `src`, and `fit` (or `objectFit`) and `objectPosition` to place the image |
| A float in a row, or with `position: 'absolute'`; `flexGrow`, `flexBasis`, `justifyContent` or `alignContent` in a box with floats | Floats only in a column `View`, a `Page` or a `Cell` |
| A line break in the `text` of the `<Canvas>` painter | One `text` call per line: it draws one line, its top at `y` |
| `hyphenationPenalty` on a `Text`, `pdfVersion` on a `Document` | Remove them; see [Not available](#not-available) |

## 5. Check what can come out differently

A document that renders can still differ from react-pdf in these places. Each says what to do
to keep the same result.

### Images from files and URLs

A path or a URL in `<Image src>` is read only from the folders and hosts you allow. Without
them, a path is an image id and a URL is refused:

```tsx
await renderToBuffer(<Invoice />, {
  localImages: { root: './assets' }, // <Image src="./logo.png" />
  remoteImages: { allow: ['https://cdn.example.com/'] }, // <Image src="https://cdn.example.com/a.png" />
});
```

The same goes for the URLs of `Font.registerEmojiSource`, which `remoteImages` must allow. See
[Images](GUIDE.md#images).

### Elements repeated on every page (`fixed`)

A `fixed` element with `position: 'absolute'` directly in a `Page` works, but its insets are
measured from the content box, inside the page's padding, not from the page's edge. On a page
with `paddingBottom: 65`:

```diff
- <Text fixed style={{ position: 'absolute', bottom: 30 }}>…</Text>
+ <Text fixed style={{ position: 'absolute', bottom: -35 }}>…</Text>
```

For a header or a footer, `<Header>` and `<Footer>` line up with the page by themselves.

### Line spacing and page count

A unitless `lineHeight` spaces lines further apart in react-pdf than here, so a long document
can break onto a different number of pages. Check documents whose page count matters.

### Line breaking

Lines break first fit by default, so a paragraph can wrap differently. For react-pdf's
breaking over the whole paragraph (Knuth–Plass), opt in with `textWrap: 'pretty'`
(`text-pretty`) on a `Text`, or on a `Page` for all its text.

### `minPresenceAhead`

It is measured from the element's top: the room it asks for includes the element's own
height, where react-pdf counts it below the element. Add the element's height to keep as much
room below it. For a heading 20 points high:

```diff
- <Text minPresenceAhead={80}>Results</Text>
+ <Text minPresenceAhead={100}>Results</Text>
```

`break={{ minPresenceAhead: 100 }}` is the same.

### `top` and `left` without `position: 'absolute'`

Pagination sees the element where `top` moves it: a page-filling element (`flex: 1`) moved down
runs onto a new page, where react-pdf lets it overflow. Use a margin, or leave room for the
offset.

### Links

`<Link src="…">` works, and adds no colour or underline: style it yourself.

```tsx
<Link src="https://example.com" style={{ color: '#2563eb', textDecoration: 'underline' }}>
  example.com
</Link>
```

### Superscripts and subscripts

`verticalAlign: 'super'` and `'sub'` move the baseline by the font's own superscript and
subscript offsets, so a superscript can sit a little higher or lower. The size stays the
`fontSize` you give. See [superscripts](GUIDE.md#text-superscripts-and-font-features).

### Images inside text

An `<Image>` inside a `<Text>` with no size is 1em high and keeps its aspect ratio. Give
`width` or `height` in points or em, not percentages; `verticalAlign` places it (`middle`,
`text-top`, `top`, a number of points…).

### Number formats of text inputs

`format={{ type, param, nDec, sepComma, negStyle, currency, currencyPrepend }}` on a
`<TextInput>` works for every `type`, `zip4` included; on a `time`, `param` is the style, `0`
to `3`. With any of these keys, the others default as before: no decimals and no thousands
separator. A format with only `type` and `currency` takes the readable defaults, 2 decimals and
`1,234.56`; add `nDec: 0` to keep `1250`:

```diff
- <TextInput name="fee" value="1250" format={{ type: 'number', currency: '$' }} />
+ <TextInput name="fee" value="1250" format={{ type: 'number', currency: '$', nDec: 0 }} />
```

`value` is the raw value (`'1250'`, `'2026-10-14'`, `'18:30'`), which the field shows
formatted. A PDF/A document leaves the formatting scripts out. See [Forms](GUIDE.md#forms).

### Drawing

SVG text is real PDF text in the registered fonts; `<textPath>` and vertical text are not
supported. Mitred strokes default to PDF's miter limit of 10.

## Renamed

| react-pdf | @inkoxide/render |
|---|---|
| `<PDFViewer>` | `<PdfViewer>`, from `@inkoxide/render/browser` |
| `<PDFDownloadLink>` | `<PdfDownloadLink>`, from `@inkoxide/render/browser` |
| `usePDF` | `usePdf`, from `@inkoxide/render/browser`: it returns `{ blob, url, loading, error, update }`, not a pair |
| `BlobProvider` | `usePdf` |
| `pdf(element).toBlob()` | `renderToBlob(element)`, from `@inkoxide/render/browser` |
| `pdf(element).toBuffer()` | `renderToBuffer(element)` |
| `style={tw('p-4')}` with `react-pdf-tailwind` | `className="p-4"`, with `@inkoxide/tailwind` installed |

`TextField` is another name for `TextInput`, and `options` for the `select` of a `Select` or a
`List`: both spellings work.

### In the browser

The browser components render in a Web Worker, with `@inkoxide/wasm` installed:

```diff
- import { PDFViewer, PDFDownloadLink, usePDF } from '@react-pdf/renderer';
+ import { PdfViewer, PdfDownloadLink, usePdf } from '@inkoxide/render/browser';

- const [instance, update] = usePDF({ document: <Invoice /> });
+ const { url, loading, error, update } = usePdf(<Invoice />);
```

`<PdfViewer>` and `<PdfDownloadLink>` render when they mount: give them another `key` to render
again. See [In the browser](GUIDE.md#in-the-browser).

## Works the same

Nothing to change for these:

- **Components:** `Document`, `Page`, `View`, `Text`, `Image`, `Link`, `ImageBackground`, `Svg`
  and its shapes, `Canvas`, `Note`, and the form fields `TextInput`, `Checkbox`, `Select`
  (`select={[…]}`), `List` (with `multiSelect`) and `FieldSet` (sets nest: `a.b.c`).
- **Styles:** style objects in points and `StyleSheet.create`: `fontWeight: 'bold'`, `flex: 1`,
  `paddingHorizontal`, `borderBottomColor` and the rest; `float` and `clear` on the children of a
  column `View`; `fontFeatureSettings` (`['tnum']` or `{ tnum: 1, liga: false }`).
- **Pagination:** `wrap={false}` and `break` on the elements in a page, `render` props with
  `pageNumber` and `totalPages`.
- **Fonts:** `Font.register`, `Font.registerHyphenationCallback` (a `Text` can also take its own
  `hyphenationCallback`) and `Font.registerEmojiSource`, which also takes `{ builder }`.
- **Output:** `renderToStream`.
- **`debug`** on `Page`, `View`, `Text` and `Image`: it draws the margin, border, padding and
  content boxes with their size in points.

## What you can use next

Optional, once the document renders:

- `<PageNumber />` and `<TotalPages />` inside a `Text`, instead of a `render` prop.
- `<Header>` and `<Footer>`, repeated on every page in its margins.
- `<Table columns={['1fr', 60, 80]} headerRows={1}>` for tables that split across pages and
  repeat their header rows, instead of rows of `View`s, which still work.
- `href` on any `Text` or `View`, and `href="#id"` to jump to an element's `id`.
- `break="avoid"` and `break="page-before"`, the same as `wrap={false}` and `break`.
- `debug` on any element with a box, and on `Document` for every page; see
  [Debugging layout](GUIDE.md#debugging-layout).

## Not available

- **Document options:** `pdfVersion`, `onRender`, and `conformance` at the a and u levels
  (`PDF/A-2a`, `PDF/A-3u`, …), which need tagged PDF. `conformance` takes `PDF/A-1b`,
  `PDF/A-2b` and `PDF/A-3b`.
- **Text:** `hyphenationPenalty` on a `Text`.
- **Fonts:** `Font.clear`, `Font.reset`, `Font.load` and `Font.getRegisteredFonts`. Fonts are
  loaded when a render needs them, and a registered face stays registered for the process.
- **Layout:** `shapeOutside`: floats are rectangles.
- **Companion packages:** `@react-pdf/math`, `@react-pdf/mermaid`, `@react-pdf/svgkit` and
  `@react-pdf/ui` have no counterpart.

## Performance

The same documents, rendered by both libraries with the same font, page size and page count:

| Document | Pages | @inkoxide/render, 1 thread | @react-pdf/renderer 4.9 |
|---|---:|---:|---:|
| Invoice (react-pdf with `react-pdf-tailwind`) | 1 | 1.2 ms | 18.4 ms |
| Invoice (react-pdf with `StyleSheet`) | 1 | 1.1 ms | 17.2 ms |
| Report: 40 sections, 1,000 table rows | 43 | 54 ms | 2,185 ms |
| In Chromium: invoice | 1 | 2.3 ms | 16.0 ms |
| In Chromium: report | 43 | 56 ms | 1,901 ms |

<sub>Apple M4 Pro, Node 24.14 and Chromium 153, median of 20 warm renders (10 for the report).
On all cores the report takes 41 ms. In a new process, the first render including loading the
library takes 166 ms for the invoice against 214 ms, and 207 ms for the report against
2,312 ms. In the browser, Inkoxide renders in a Web Worker, and react-pdf on the main thread,
which a report blocks for about two seconds; the engine is a 1.18 MB download with brotli.
[docs/BENCHMARKS.md](https://github.com/eddiesr93/inkoxide/blob/main/docs/BENCHMARKS.md) has
the rest.</sub>
