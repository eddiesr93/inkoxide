# @inkoxide/render guide

The reference for @inkoxide/render. The [README](README.md) covers installing, the quick start
and the examples; [MIGRATING.md](MIGRATING.md) is the migration guide.

- [Components](#components)
- [Styling](#styling)
- [Fonts](#fonts)
- [Images](#images)
- [Pages, headers and footers](#pages-headers-and-footers)
- [Tables](#tables)
- [Links, bookmarks and metadata](#links-bookmarks-and-metadata)
- [Text: hyphenation and direction](#text-hyphenation-and-direction)
- [Drawing: SVG and Canvas](#drawing-svg-and-canvas)
- [Forms](#forms)
- [API](#api)
- [Errors](#errors)
- [FAQ](#faq)
- [Limitations](#limitations)

## Components

All components take `className`, `style` and `id`.

| Component | What it is |
|---|---|
| `Document` | The PDF: `title`, `author`, `subject`, `keywords`, `lang` (`"en-US"`). |
| `Page` | One page layout; content flows over as many sheets as it needs. `size` (a [preset](#page-sizes) such as `"A4"`, `"letter"` or `"A4-landscape"`, `[width, height]`, `{ width, height }`, or `[width]` or one length for a square), `orientation`, `margin`. Padding on a page is its margin. |
| `View` | A box: flex column by default, and also grid. |
| `Text` | A paragraph. A `Text` inside a `Text` is an inline run (bold, colour, link). |
| `Link` | A link (`href`, or `src`): a linked run inside a `Text`, a box like a `View` elsewhere. |
| `Image` | JPEG, PNG, GIF, WebP or SVG. `fit`: `fill`, `contain`, `cover`, `none`. |
| `Header`, `Footer` | Repeated on every page, in the top or bottom margin. |
| `PageNumber`, `TotalPages` | The current page and the page count, inside a `Text`. |
| `Table`, `Row`, `Cell` | A table that splits across pages. |
| `PageBreak`, `Spacer` | A forced page break; empty space. |
| `Svg`, `Path`, `Rect`, `Circle`, `Ellipse`, `Line`, `Polyline`, `Polygon`, `G`, `Defs`, `ClipPath`, `LinearGradient`, `RadialGradient`, `Stop` | Vector drawing in JSX. |
| `Canvas` | Drawing in code: `paint={(painter, width, height) => …}`. |
| `TextField`, `Checkbox`, `Radio`, `Select` | Fillable form fields (AcroForm). |

**The one rule:** a `Text` contains strings, numbers, `PageNumber`, `TotalPages`, `Link` and
other `Text`, and nothing else. A `View` inside a `Text` is an error that names the component.

## Styling

### Style objects

```tsx
<View style={{ padding: 12, borderBottomWidth: 0.5, borderBottomColor: '#ddd' }} />
<View style={[styles.row, index % 2 === 1 && styles.zebra]} />
<Text style={{ fontSize: '9pt', marginTop: '2mm' }} />
```

- **Units.** Numbers are points (1/72 inch). Strings may use `pt`, `mm`, `cm`, `in`, `px` or
  `pc`, or a percentage.
- **Arrays.** Falsy entries are skipped and later entries win.
- **Shorthands.** `padding`, `margin`, `paddingHorizontal`/`paddingVertical`, per-side keys and
  `flex: 1` (meaning `1 1 0%`) all work.
- **Inheritance.** Text properties (font, size, weight, colour, line height, alignment) pass down
  from `Page` and `View` to the text inside. Box properties apply only to the element that has
  them.

### Tailwind

Write the classes you would write on the web. They are resolved by the Tailwind v4 compiler.

- **Units are tuned for print:** `1rem = 12pt` and `1px = 0.75pt`. So `p-4` is 12pt, `text-sm`
  is 10.5pt and `text-xs` is 9pt. Use arbitrary values for exact sizes: `text-[9.5pt]`,
  `mt-[14pt]`, `w-[60mm]`.
- **Supported:** flexbox, grid, sizing, spacing, the full colour palette with opacity
  (`bg-black/5`), borders, per-corner radii, typography, `absolute` positioning with `z-*`,
  and 2D transforms.
- **Not supported:** variants (`md:`, `hover:`), shadows, gradients and `space-*`/`divide-*`
  (use `gap-*` and borders). Using one throws during development and names the class.

## Fonts

Four families ship with the package, so nothing needs registering:

| Family | Faces | Use it with |
|---|---|---|
| Inter | regular, italic, bold | the default (`font-sans`) |
| Inter Display | bold | `font-display`, for large headings |
| Roboto Mono | regular | `font-mono` |
| Noto Sans | regular | automatic, for characters Inter lacks |

`font-semibold` and other weights without a face of their own use the nearest one (bold), as in
CSS.

Add your own fonts with `Font.register`:

```ts
import { Font } from '@inkoxide/render';

Font.register({ family: 'Lora', src: './fonts/Lora-Regular.ttf' });

Font.register({
  family: 'Source Serif',
  fonts: [
    { src: './fonts/SourceSerif-Regular.ttf' },
    { src: './fonts/SourceSerif-Bold.ttf', fontWeight: 'bold' },
    { src: 'https://example.com/fonts/SourceSerif-Italic.ttf', fontStyle: 'italic' },
  ],
});
```

Then use it with `style={{ fontFamily: 'Lora' }}`, or with `className="font-[Lora]"`.

- **Sources.** `src` is a file path, a `file:` or `https:` URL, or the bytes (`Uint8Array` or
  `ArrayBuffer`). Formats are TrueType (`.ttf`) and OpenType (`.otf`). Variable fonts are not
  supported: register the static instances.
- **Loading.** `register` is synchronous. The file is read or fetched just before the next
  render, and a font that cannot be loaded fails that render with `INVALID_FONT`.
- **Weights.** Weights are numbers (100–900) or names (`'light'`, `'medium'`, `'bold'`).
  Missing weights and styles fall back to the nearest one, as in CSS.

## Images

```tsx
import { readFileSync } from 'node:fs';

const logo = readFileSync('logo.png');

<Image src={logo} className="h-10 w-10" fit="contain" />         // bytes
<Image src="data:image/png;base64,iVBORw0…" />                  // a data: URL
<Image src="logo" />                                            // an id from options.images
<Image src="https://cdn.example.com/photo.jpg" />               // remote, when allowed
```

Remote images are off by default, so a template cannot make your server fetch arbitrary URLs.
Allow them per render, or with the `INKOXIDE_REMOTE_IMAGES` environment variable (a
comma-separated list):

```ts
await renderToBuffer(<Doc />, { remoteImages: { allow: ['https://cdn.example.com/', '*.example.org'] } });
```

## Pages, headers and footers

```tsx
<Page size="A4" className="p-16 text-xs">
  <Header className="flex-row justify-between text-gray-500">
    <Text>ACME Corp.</Text>
    <Text pages="notFirst">Annual report 2026</Text>
  </Header>
  <Footer className="text-gray-400">
    <Text className="text-center">Page <PageNumber /> of <TotalPages /></Text>
  </Footer>

  <Text className="text-xl font-bold" break={{ minPresenceAhead: 80 }}>Results</Text>
  <View break="avoid">{/* a signature block that stays on one page */}</View>
  <PageBreak />
</Page>
```

- **Header and footer.** They sit in the page margin, line up with the content and are centred
  vertically. Their own vertical padding or alignment (`items-*` in a `flex-row`, `justify-*` in a
  column) overrides that.
- **Showing on some pages.** `pages="first" | "notFirst" | "last" | "notLast" | "odd" | "even"`,
  or a list of page numbers, shows an element in a header or footer only on those pages.
- **Paragraphs.** `orphans` and `widows` keep at least that many lines together at the foot and
  head of a page.
- **Content that depends on the page.** Use `render={({ pageNumber, totalPages }) => …}` on a
  `Text` or `View` in a header or footer.

### Page sizes

| `size` | Sizes |
|---|---|
| `4A0`, `2A0`, `A0`–`A10` | ISO 216 A series (`A4` is 210 × 297 mm) |
| `B0`–`B10` | ISO 216 B series |
| `C0`–`C10` | ISO 269 C series (envelopes) |
| `RA0`–`RA4`, `SRA0`–`SRA4` | ISO 217 untrimmed paper |
| `Letter`, `Legal`, `Tabloid`, `Executive`, `Folio` | 8.5 × 11, 8.5 × 14, 11 × 17, 7.25 × 10.5 and 8.5 × 13 in |
| `ID1` | ID-1 card (a bank card), 53.98 × 85.6 mm |

Presets are portrait, and their names match in any case (`"letter"`, `"LETTER"`). Add
`-landscape` (`"A5-landscape"`) or `orientation="landscape"` to turn one sideways. Any other size
is `[width, height]` or `{ width, height }`, and `[width]` or a single length makes a square
page. Lengths are points or strings with a unit:

```tsx
<Page size="a4-landscape" />
<Page size={[400, 600]} />
<Page size={['100mm', '150mm']} />
<Page size="80mm" />
```

`PAGE_SIZES` has every preset in points.

## Tables

```tsx
<Table columns={['1fr', 50, 80]} headerRows={1} className="w-full">
  <Row className="bg-gray-50 font-bold">
    <Cell className="p-2">Item</Cell>
    <Cell className="p-2 text-right">Qty</Cell>
    <Cell className="p-2 text-right">Amount</Cell>
  </Row>
  {items.map((it) => (
    <Row key={it.id}>
      <Cell className="border-b border-gray-200 p-2">{it.name}</Cell>
      <Cell className="border-b border-gray-200 p-2 text-right">{it.qty}</Cell>
      <Cell className="border-b border-gray-200 p-2 text-right">{it.amount}</Cell>
    </Row>
  ))}
</Table>
```

- **Columns.** Widths are fractions (`'1fr'`) or points.
- **Pagination.** Header rows repeat on every page. Rows split across pages, and a row taller
  than a page splits inside its cells.
- **Cells.** `colSpan`, `rowSpan` and `verticalAlign` are available on `Cell`.

## Links, bookmarks and metadata

```tsx
<Document title="Annual report" author="ACME" lang="en-US">
  <Page>
    <Text href="#results" className="text-blue-700 underline">Jump to results</Text>
    <Text href="https://example.com">Website</Text>
    <Text>
      Read the <Link href="https://example.com/terms" className="underline">terms</Link>.
    </Text>
    <Text id="results" bookmark="Results" className="text-xl font-bold">Results</Text>
    <Text bookmark={{ title: 'Revenue', level: 2 }}>Revenue</Text>
  </Page>
</Document>
```

`href` on a `Text` or `View` makes the whole element a link, and `<Link>` does the same as a
component: inside a `Text` it is a linked run, elsewhere a box like a `View`. A link takes the
style it is given and adds no colour or underline of its own. `href="#id"` jumps to the element
with that `id`.

Bookmarks become the PDF outline in the viewer's sidebar. With `{ pdfA: true }`, the output is
PDF/A-2b, for archiving.

## Text: hyphenation and direction

```tsx
<Document lang="en-US">
  <Page>
    <Text className="text-justify" hyphenate>Long paragraphs break words at syllables.</Text>
  </Page>
</Document>
```

- **Hyphenation** is off by default. Turn it on per `Text` with `hyphenate` (or the class
  `hyphens-auto`); it follows the `lang` of the text or the document. US English patterns ship
  with the package; other languages use the engine's built-in patterns.
- **Right-to-left text** uses `dir="rtl"` on a `Text`. Register a font that covers the script
  first (for example Noto Sans Arabic or Noto Sans Hebrew with `Font.register`): the bundled
  fonts cover Latin, Greek and Cyrillic.
- **Orphans and widows:** `orphans={2}` and `widows={2}` keep at least two lines of a paragraph
  together at the foot and the head of a page.

## Drawing: SVG and Canvas

```tsx
<Svg width={120} height={60} viewBox="0 0 120 60">
  <Rect x={0} y={20} width={30} height={40} rx={3} fill="#c7d2fe" />
  <Rect x={45} y={0} width={30} height={60} rx={3} fill="#4f46e5" />
  <Path d="M0 58 L120 58" stroke="#9ca3af" strokeWidth={1} />
</Svg>

<Canvas width={120} height={60} paint={(p, w, h) => p.moveTo(0, h).lineTo(w, 0).lineWidth(2).stroke('#4f46e5')} />
```

The SVG primitives take SVG attributes in camelCase (`strokeWidth`, `stopColor`); text inside
`<Svg>` is not supported yet. `Canvas` draws in code with a pdfkit-style painter. An SVG file
can also go straight into `<Image src={svgBytes} />`.

## Forms

```tsx
<TextField name="email" className="h-6 w-full rounded border border-gray-300 px-1" />
<Select name="plan" options={['Starter', 'Team']} value="Team" className="h-6 w-40 border" />
<Checkbox name="terms" className="h-3.5 w-3.5 border" />
<Radio name="billing" value="yearly" checked className="h-3.5 w-3.5 rounded-full border" />
```

These are real AcroForm fields: readers fill them in any PDF viewer and save the file. `name`
must be unique, except for radio buttons of one group. `TextField` also takes `multiline`,
`maxLength` and `password`; size, border, background and text style come from the classes.

## API

```ts
renderToBuffer(element, options?): Promise<Uint8Array>   // the PDF bytes
renderToFile(element, path, options?): Promise<void>      // renders, then writes the file
```

Both take the same options:

| Option | Default | What it does |
|---|---|---|
| `images` | none | Image bytes by id, for `<Image src="logo">` |
| `remoteImages` | off | `{ allow: ['https://cdn.example.com/', '*.example.org'] }`: hosts `<Image src="https://…">` may load from. The `INKOXIDE_REMOTE_IMAGES` environment variable (comma-separated) sets it for every render. |
| `pdfA` | `false` | Write PDF/A-2b, for archiving |
| `compress` | `true` | Compress the page content |
| `subsetFonts` | `true` | Embed only the glyphs the document uses |
| `creationDate` | none | RFC 3339 timestamp written to the PDF. Without it, the same input gives byte-identical output. |
| `threads` | all cores | How many cores one render spreads its pages over |
| `validate` | outside production | Check the document tree first, for clearer errors |

**`Font.register({ family, src })`** adds a font family, from a path, a URL or bytes. See
[Fonts](#fonts).

**`StyleSheet.create(styles)`** returns the styles unchanged, with their types.

**`init()`** loads Tailwind's compiler, the engine and the fonts. The first render does it
anyway; call it at server startup so the first request does not wait (about 150 ms).

**Concurrency.** A render runs on Node's thread pool, so many can run at once. `threads` sets
how many cores one render uses. Node's `UV_THREADPOOL_SIZE` (default 4) caps how many renders run
at the same time.

## Errors

Mistakes fail loudly, with the component that caused them. A `View` inside a `Text`:

```tsx
<Text>Total: <View /></Text>
```

stops the render with:

```
RenderError: <View> cannot be inside <Text>. A Text contains only strings, numbers,
<Text>, <PageNumber> and <TotalPages>.
    in View
    in Text
    in Page
    in Document
    in Receipt
```

Every error is a `RenderError` with a `code`, and with `path`, `component` and
`componentStack` where they apply:

| Code | What happened |
|---|---|
| `TEMPLATE_ERROR` | A component used in the wrong place, or an unknown class or style key |
| `INVALID_TREE` | A prop with an invalid value |
| `INVALID_FONT` | A registered font could not be read, fetched or parsed |
| `ENGINE_REJECTED` | The engine refused the document, for example an unreadable image |
| `IMAGE_FETCH_FAILED` | An allowed remote image could not be fetched |

## FAQ

**How is this different from HTML to PDF with Puppeteer or Playwright?**
Those print a web page with a headless Chrome, so any CSS works, but you ship and run a browser,
and each PDF is a browser round trip. Inkoxide has no browser: a native library that lays out
documents in milliseconds, with print features built in, such as repeating table headers, page
numbers and PDF/A.

**Does it run in the browser?**
No. The engine is a native Node addon. Render on your server, or in a serverless function on
Node, and send the bytes to the browser.

**The install worked, but the engine does not load.**
The engine ships as one package per platform: macOS (arm64, x64), Linux x64 (glibc and musl),
Linux arm64 (glibc) and Windows x64. npm installs the right one as an optional dependency, so it
is missing if you install with `--omit=optional`, or copy `node_modules` from one platform to
another (from macOS into a Linux Docker image, say). Run `npm install` on the platform that runs
the code.

**Do I need Tailwind installed?**
No. The Tailwind compiler ships with the package: write classes, with no config and no CSS build.

**Can I use my own components and hooks?**
Yes. A document is ordinary React: components, props, context, `.map` and conditionals all work,
as do hooks that compute during render (`useMemo`, `useContext`). The PDF is built from that
first render, so state updates and effects do not change it.

**A page number at the right of a footer stops short of the margin.**
Give its text `text-right`: `<Text className="text-right">Page <PageNumber /></Text>`. The
engine reserves room for the widest page number, and the alignment decides where shorter ones
sit.

**Is the output the same on every machine?**
Yes, for the same input, fonts and options, as long as you leave out `creationDate`.

## Limitations

- **Server only.** The engine is a native addon: there is no browser rendering, and it does not
  run on edge runtimes such as Cloudflare Workers.
- **Drawing.** Colour emoji and text inside `<Svg>` are not supported yet.
- **Tailwind.** Variants (`md:`, `hover:`), shadows and gradients are not supported.
- **Output.** There is no streaming: you get the whole PDF as bytes.
