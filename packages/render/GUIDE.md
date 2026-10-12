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
- [Debugging layout](#debugging-layout)
- [API](#api)
- [Errors](#errors)
- [Serving templates](#serving-templates)
- [FAQ](#faq)
- [Limitations](#limitations)

## Components

Components take `className`, `style` and `id`, and those with a box also take `debug`
([Debugging layout](#debugging-layout)), with these exceptions: `Document` takes only `debug`;
`Page` and `Canvas` have no `id`; `PageBreak` takes only `id`; `PageNumber` and `TotalPages`
take no props; the shapes inside an `Svg` take SVG attributes (`fill`, `stroke`,
`transform`, `id`) instead of `className` and `style`; and a `Text` or `Link` inside a `Text`,
a run of the paragraph, takes only text styles, `href`, `lang`, `hyphenate` and
`hyphenationCallback` (props such as `id`, `bookmark`, `pages` or `render` are an error there).

| Component | What it is |
|---|---|
| `Document` | The PDF: `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `lang` (`"en-US"`), `creationDate`, `pageMode`, `pageLayout` and `fallbackFonts` ([metadata](#links-bookmarks-and-metadata)). |
| `Page` | One page layout; content flows over as many sheets as it needs. `size` (a [preset](#page-sizes) such as `"A4"`, `"letter"` or `"A4-landscape"`, `[width, height]`, `{ width, height }`, or `[width]` or one length for a square), `orientation`, `margin`. Padding on a page is its margin; a percentage is of the page width, on every side, as in CSS. |
| `View` | A box: flex column by default, and also grid. |
| `Text` | A paragraph. A `Text` inside a `Text` is an inline run (bold, colour, link). |
| `Link` | A link (`href`, or `src`): a linked run inside a `Text`, a box like a `View` elsewhere. |
| `Image` | JPEG, PNG, GIF, WebP or SVG. `fit` (or `objectFit` in its style): `fill`, `contain`, `cover`, `none`; `objectPosition` in its style places the fitted image. |
| `Header`, `Footer` | Repeated on every page, in the top or bottom margin. |
| `PageNumber`, `TotalPages` | The current page and the page count, inside a `Text`. |
| `Table`, `Row`, `Cell` | A table that splits across pages. |
| `PageBreak`, `Spacer` | A forced page break; empty space. |
| `Svg`, `Path`, `Rect`, `Circle`, `Ellipse`, `Line`, `Polyline`, `Polygon`, `G`, `Defs`, `ClipPath`, `LinearGradient`, `RadialGradient`, `Stop` | Vector drawing in JSX. |
| `Canvas` | Drawing in code: `paint={(painter, width, height) => …}`. |
| `TextField`, `Checkbox`, `Radio`, `Select`, `Field` | Fillable form fields (AcroForm). |

**The one rule:** a `Text` contains strings, numbers, `PageNumber`, `TotalPages`, `Link` and
other `Text`, and nothing else. A `View` inside a `Text` is an error that names the component.

## Styling

### Style objects

```tsx
<View style={{ padding: 12, borderBottomWidth: 0.5, borderBottomColor: '#ddd' }} />
<View style={[styles.row, index % 2 === 1 && styles.zebra]} />
<Text style={{ fontSize: '9pt', marginTop: '2mm' }} />
```

- **Units.** Numbers are points (1/72 inch). Strings may use `pt`, `mm`, `cm`, `in`, `px`
  (0.75pt), `pc` (12pt), `rem` (12pt, as in Tailwind), `em` (the element's font size; in
  `fontSize`, the inherited one), `vw` and `vh` (1% of the page's width and height), or a
  percentage. `calc()` adds lengths (`'calc(10mm + 1em)'`) or percentages, but not one to the
  other. A class's arbitrary value (`w-[…]`) takes the same syntax and reads the same. A page's
  `size` takes the units that do not depend on the font or the page.
  Percentages follow CSS: widths, padding and margins are of the parent's width, and gaps of
  the element's own size. A percentage height needs a parent with a set height; a child of the
  `Page` takes it of the page.
- **Arrays.** Falsy entries are skipped and later entries win, over `className` too, as a
  later CSS rule does: a later `borderRadius: 8` replaces an earlier corner's radius.
- **Shorthands.** `padding`, `margin`, `paddingHorizontal`/`paddingVertical`, per-side keys and
  `flex: 1` (meaning `1 1 0%`) all work. `border` and `borderTop`, `borderRight`,
  `borderBottom`, `borderLeft` take CSS strings (`'1px solid #e5e7eb'`, `'2pt dashed red'`,
  `'none'`); a missing style is solid and a missing colour is the text colour. A side wins over
  `border`, and `borderWidth`, `borderColor`, `borderStyle` and the per-side keys win over both.
- **CSS names.** `textDecoration` takes `'underline'`, `'line-through'` or both; a run has one
  line, so `'underline line-through'` draws the line-through. `textDecorationColor`,
  `textDecorationStyle` and `textDecorationThickness` are the same as `decorationColor`,
  `decorationStyle` and `decorationThickness`. `textOverflow: 'ellipsis'` is `ellipsis: true`
  (with `maxLines`). `objectFit` on an `Image` is its `fit` prop, which wins when both are set.
  `objectPosition` on an `Image` places it in its box (see [Images](#images)). Both throw on
  other elements.
- **Inheritance.** Text properties (font, size, weight, colour, line height, alignment) pass down
  from `Page` and `View` to the text inside. Box properties apply only to the element that has
  them.

### Tailwind

Write the classes you would write on the web. They are resolved by the Tailwind v4 compiler, in
`@inkoxide/tailwind`, which you install next to `@inkoxide/render` when you use `className`:

```sh
npm install @inkoxide/tailwind
```

Without it, `style` works the same, and a `className` fails with an error that says to install
it.

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
| Noto Sans | regular | automatic, for characters Inter lacks ([`fallbackFonts`](#links-bookmarks-and-metadata)) |

`font-semibold` and other weights without a face of their own use the nearest one (bold), as in
CSS.

No serif face ships with the package, so `font-serif` sets Noto Sans. For serif text, register
a serif family and use it with `fontFamily` or `font-[…]`, or set `--font-serif` in the theme you
pass to `initTailwind({ css })` from `@inkoxide/tailwind`.

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

- **Sources.** `src` is a file path, a `file:`, `https:` or `http:` URL, or the bytes
  (`Uint8Array` or `ArrayBuffer`). Prefer `https:`: a font fetched over `http:` can be changed on
  the way. Formats are TrueType (`.ttf`), OpenType (`.otf`), WOFF (`.woff`) and WOFF2
  (`.woff2`). Variable fonts are not supported: register the static instances. Font collections
  (`.ttc`) are not supported either: register each font as its own file.
- **Loading.** `register` is synchronous. The file is read or fetched just before the next
  render, and a font that cannot be loaded fails that render with `INVALID_FONT`. The fonts
  registered with it still load, and later renders can use them. A URL is fetched with a
  30-second limit for the whole download and a 50 MiB size limit, and redirects are followed;
  renders wait for it, so for a font over that size or from a slow host, read the bytes
  yourself and register them.
- **Weights.** Weights are numbers (1–1000, as in CSS) or names (`'light'`, `'medium'`,
  `'bold'`). Missing weights and styles fall back to the nearest one, as in CSS.
- **Faces.** A face is a family, a weight and a style. Registering one again from the same
  source (the same path, URL or bytes) does nothing, and from another source throws. A face the
  bundled fonts have (`Inter` 400) cannot be replaced: register your build under another family
  name. You can add the faces they lack, such as `Inter` bold italic. Two family names that
  differ only in spaces and hyphens (`Brand Two`, `Brand-Two`) cannot both be registered.

## Images

```tsx
import { readFileSync } from 'node:fs';

const logo = readFileSync('logo.png');

<Image src={logo} className="h-10 w-10" fit="contain" />         // bytes: Uint8Array, Buffer, ArrayBuffer
<Image src="data:image/png;base64,iVBORw0…" />                  // a base64 data: URL
<Image src="logo" />                                            // an id from options.images
<Image src="./images/logo.png" />                               // a file, when allowed
<Image src="https://cdn.example.com/photo.jpg" />               // remote, when allowed
<Image src={{ uri: 'https://cdn.example.com/chart', method: 'POST', headers, body }} />
<Image src={() => loadLogo()} />                                // a function or a promise
```

- **Fit and position.** `fit` (or `objectFit`, or `object-contain`, `object-cover`,
  `object-fill`, `object-none`) sizes the image in its box: `fill` (the default) stretches it
  (an SVG with a `viewBox` is not stretched: as in CSS, the box becomes its viewport, and its
  `preserveAspectRatio`, by default centred and whole, fits the drawing in), `contain` shows all
  of it, `cover` fills the box, `none` keeps its own size (96 dpi). With
  `contain`, `cover` and `none`, the image is centred unless `objectPosition` places it, as CSS
  `object-position` does: one or two values, each a keyword (`left`, `center`, `right`, `top`,
  `bottom`, in either order when both are keywords), a length or a percentage (`'25% 75%'`
  lines up that point of the image with that point of the box). A single value leaves the other
  axis centred. The Tailwind classes `object-left-top`, `object-right`, `object-bottom` and so on,
  and `object-[25%_75%]`, do the same. What falls outside the box is clipped. Edge offsets
  (`'right 10pt bottom 5pt'`) are not supported.

  ```tsx
  <Image src={photo} className="h-40 w-full" fit="cover" style={{ objectPosition: 'center top' }} />
  <Image src={logo} className="h-10 w-32 object-contain object-left" />
  ```

- **Files.** Image files are off by default. Allow the folders they come from with the
  `localImages` option, or the `INKOXIDE_LOCAL_IMAGES` environment variable (folders separated
  by `:`, or `;` on Windows):

  ```ts
  await renderToBuffer(<Doc />, { localImages: { root: '/srv/app/images' } });
  ```

  A string is then a file path when it is written as one: relative with `./` or `../`,
  absolute (`/srv/app/images/logo.png`, `C:\images\logo.png`), or a `file:` URL. Any other
  string (`logo`, `logo.png`, `img/logo.png`) is an image id, so write `./logo.png` for a file.
  Without `localImages`, a path is an image id too, and no file is read.
- **Folders.** A relative path is resolved against the first `root` (an array allows several),
  not the working directory. Any path must lead, symbolic links followed, to a regular file
  inside one of them. Network paths (`\\host\share`, `//host/share`, `file://host/…`) are
  refused, and so is a file over `maxBytes` (default 20 MiB). The error names the path as the
  template wrote it, and is the same for a missing file and a file outside the folders:
  `Cannot read the image ../secret.png: no such file in the localImages folders.`
  (`IMAGE_READ_FAILED`).
- **Ids first.** A string that names an image in the `images` option (or a
  [template's](#serving-templates) `images`) is that image, even when it looks like a path or a
  URL. No file is read for it.
- **Requests.** `{ uri, method, headers, body }` fetches `uri` with that method, headers and
  body. It follows the same rules as a URL string: the allowlist, the timeout, the size limit
  and no redirects.
- **Redirects.** A remote image is not followed to another URL, since that URL could be outside
  the allowlist: a redirect fails the render with `IMAGE_FETCH_FAILED`
  (`HTTP 302, and redirects are not followed`). Use the URL the image ends up at.
- **Remote limits.** `remoteImages.timeoutMs` (default 10 000 ms; `Infinity` waits without a
  limit) covers the whole download, body included, and `maxBytes` (default 20 MiB) its size. A
  download that times out, breaks off or grows past the limit fails with `IMAGE_FETCH_FAILED`.
- **Functions and promises.** A function is called once per render, after the tree is built and
  before the engine runs, with the other images loading in parallel; a promise is awaited
  then. Either resolves to bytes, a string or a request, read as if written in `src`. If it
  throws, rejects or does not settle within `imageTimeoutMs` (default 10 000 ms), the render
  fails with `TEMPLATE_ERROR`. A `render` prop that creates a new function or promise each time
  has it loaded again when it renders again for the page count. Prefer a function to a promise
  made before the render starts: a promise that rejects while nothing awaits it yet is an
  unhandled rejection.

Image files and remote images are off by default, so a template cannot make your server read
its files or fetch arbitrary URLs. Allow only the folders and hosts your images are in. Remote
images are allowed per render, or with the `INKOXIDE_REMOTE_IMAGES` environment variable (a
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
  or a list of page numbers, shows an element in a header, a footer or a
  [fixed element](#on-every-page-fixed) only on those pages. Page numbers count from 1 across
  the whole document; a negative number counts from the end, so `pages={[1, -1]}` is the first
  and the last page. `0` and an empty list are errors.
- **In the page body.** `pages` works on any element of the body too. The element keeps its
  place in the layout on every page and is painted only on those pages: on others it leaves an
  empty space, and of a box split across pages only the parts on those pages are drawn.
- **Paragraphs.** `orphans` and `widows` keep at least that many lines together at the foot and
  head of a page.
- **Page breaks.** `<PageBreak />`, `break="page-before"` and `break="page-after"` start a new
  page at any depth of the body, when content follows them: a break at the end adds no blank
  page, and neither does one with nothing above it on its page. A break before the first child
  of a `View` takes the `View` along. They are errors inside a `flex-row`, grid, wrapping or
  reversed `View` and inside an absolutely positioned, rotated or transformed element, where
  they would do nothing. On an element, `break="page-before"` and `break="page-after"` are
  errors only when it is absolutely positioned (it is out of the flow); on a rotated or
  transformed element they work.
- **Keeping together.** `break="avoid"` (`wrap={false}`) moves an element to the next page whole
  when it does not fit; a `PageBreak` inside it still breaks, and the part before it moves whole.
  A `flex-row`, grid or wrapping `View` moves whole too. Taller than a page, both split: a row
  between the lines of its children, each column continuing on the next page.
- **Content that depends on the page.** Use `render={({ pageNumber, totalPages }) => …}` on a
  `Text` or `View` in a header, a footer or a fixed element. A component rendered by a `render`
  prop can read the same numbers with the `usePage()` hook, which throws anywhere else:

  ```tsx
  function Folio() {
    const { pageNumber, totalPages } = usePage();
    return <Text>{pageNumber === totalPages ? 'Last page' : `Page ${pageNumber}`}</Text>;
  }

  <Footer>
    <View render={() => <Folio />} />
  </Footer>
  ```

### On every page: `fixed`

Use `Header` and `Footer` for page chrome: titles and page numbers in the margins, with room
kept for them. For a watermark, a page border or a stamp, mark an element `fixed`. It is painted
on every page of its `Page`, over the content or, with a negative `zIndex`, under it:

```tsx
<Page size="A4" style={{ padding: 48 }}>
  <Text
    fixed
    style={{ position: 'absolute', top: '45%', left: 0, right: 0, textAlign: 'center' }}
    className="rotate-[-30deg] text-[64pt] font-bold text-red-500/15"
  >
    DRAFT
  </Text>
  <View
    fixed
    style={{ position: 'absolute', top: -16, right: -16, bottom: -16, left: -16 }}
    className="border border-gray-300"
  />
  <Text
    fixed
    pages="notFirst"
    style={{ position: 'absolute', bottom: -36, left: 0, right: 0, textAlign: 'center' }}
    render={({ pageNumber, totalPages }) => `${pageNumber} / ${totalPages}`}
  />
  {/* the content */}
</Page>
```

- **Placement.** A fixed element goes directly in the `Page`, with `position: 'absolute'` (or
  `className="absolute"`). Its `top`, `right`, `bottom`, `left` and percentages are measured from
  the page's content box, the area inside the margin, on every page. Negative values reach into
  the margin: with `padding: 48`, `bottom: -36` is 12pt above the edge of the paper. On a
  one-page document, adding `fixed` moves nothing.
- **Stacking.** A fixed element is painted over the content, and under the `Header` and
  `Footer`. With a negative `zIndex` (`-z-10`) it is painted under the content instead, over the
  page background: a watermark that text and filled boxes cover. Fixed elements stack among
  themselves by `zIndex`, then in the order they are written.
- **Space.** It takes none: the content flows under it. Content that needs room on every page
  belongs in a `Header` or `Footer`, with a margin sized for it.
- **Page numbers.** `render`, `<PageNumber />` and `<TotalPages />` show each page its own
  numbers, and `pages` chooses the pages it appears on.
- **Links, ids and bookmarks.** An `href` makes a link on every page. An `id` (the target of
  `href="#id"`) and a `bookmark` point to the first page.
- **Not allowed.** `fixed` on an element inside another element, in a `Table`, `Header` or
  `Footer`, on a `Header` or `Footer` itself, on a `Text` inside a `Text`, on a form field, or
  without `position: 'absolute'`. A fixed element is painted whole on every page, so `break`,
  `wrap={false}` and `minPresenceAhead` on it or inside it, and a `PageBreak` inside it, would do
  nothing (as in a `Header` or `Footer`). Each is an error that names the component.

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
- **Cells.** `colSpan`, `rowSpan` and `verticalAlign` are available on `Cell`. The columns set a
  cell's width, and a cell's `height` and `minHeight` are minimums: the row grows to fit the
  content. Styles that place or size the cell itself (`width`, `minWidth`, `maxWidth`, `margin`,
  `position`, `flexGrow`, …) or apply an effect to it (`opacity`, `rotate`, `transform`, …) are
  errors on a `Cell`; put them on a `View` inside it.

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

`Document` writes `title`, `author`, `subject`, `keywords`, `creator` (the application that
made the file) and `producer` (the software that wrote the PDF, by default `Inkoxide <version>`)
into the PDF's properties. `creationDate` (an RFC 3339 timestamp, `"2026-10-12T09:30:00Z"`) dates
it, as the `creationDate` option does; without either, the same input gives byte-identical output.

`pageMode` and `pageLayout` say how the viewer opens the PDF:

```tsx
<Document pageMode="useThumbs" pageLayout="twoPageRight">…</Document>
```

`pageMode` is `'useNone'` (no side panel), `'useOutlines'` (the bookmarks), `'useThumbs'` (page
thumbnails) or `'fullScreen'`. The default shows the bookmarks when there are any. `pageLayout`
is `'singlePage'`, `'oneColumn'` (continuous scrolling), `'twoColumnLeft'` or `'twoColumnRight'`
(two scrolling columns, odd pages on the left or right), or `'twoPageLeft'` or `'twoPageRight'`
(two pages at a time); without it, the viewer uses its own setting.

`fallbackFonts` lists the fonts tried, in order, for characters a run's font lacks. The default is
`['NotoSans-Regular']`. The entries are font ids: the bundled `Inter-Regular`, `Inter-Italic`,
`Inter-Bold`, `InterDisplay-Bold`, `NotoSans-Regular` and `RobotoMono-Regular`, or a face added
with `Font.register`, whose id is its family with spaces as hyphens, its weight, and `-italic` for
an italic face:

```tsx
Font.register({ family: 'Noto Sans Arabic', src: './fonts/NotoSansArabic-Regular.ttf' });

<Document fallbackFonts={['NotoSans-Regular', 'Noto-Sans-Arabic-400']}>…</Document>
```

An id that is not registered is skipped, and `fallbackFonts={[]}` turns the fallback off.

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
  with the package and serve every English `lang`; other languages use the engine's built-in
  patterns. A paragraph is hyphenated in one language, so a `Text` inside a `Text` with another
  `lang` is not hyphenated by patterns: make it a paragraph of its own, or give it a
  `hyphenationCallback`. A soft hyphen
  (`\u00AD`) in the text is always a possible break.
- **Your own hyphenation.** `Font.registerHyphenationCallback` sets a function that splits each
  word into the parts a line may break between, for every `Text`; `[word]` keeps a word whole.
  It replaces the built-in patterns, so `hyphenate` is not needed with it.

  ```ts
  import { Font } from '@inkoxide/render';

  const whole = new Set(['Inkoxide', 'PostgreSQL']);
  // syllables(word) is your own rules, or a hyphenation library: 'hyphenation' → ['hy', 'phen', 'ation'].
  Font.registerHyphenationCallback((word) => (whole.has(word) ? [word] : syllables(word)));
  ```

  A word is a run of letters as displayed, after `uppercase` or another text transform; spaces,
  digits and punctuation stay as they are, and a word that already has soft hyphens is left
  alone. A `Text`, or a `Text` inside one, takes its own function with
  `hyphenationCallback={fn}`, and the built-in patterns leave its words alone; it goes back to
  `hyphenate` and `lang` with `hyphenationCallback={null}`. `hyphenate={false}` (or
  `hyphens-none`) turns the callback off for that text. Text with `whitespace-nowrap` or
  `whitespace-pre` is not broken.
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

In an SVG, whether in `<Svg>` or an image file:

- **Text** (`<text>`) fails the render with `INVALID_IMAGE`, in a nested SVG image too: convert
  text to outlines (paths) in your design tool.
- **Filters** (`filter`: drop shadows, blurs) are not applied: the element is drawn without them.
- **Images** inside the SVG must be embedded as `data:` URLs (JPEG, PNG, GIF, WebP or SVG).
  Links to files or URLs are left out, and an embedded image over 50 megapixels fails the
  render.
- **Size**: the width and height are 0.00001 to 100,000 px each, and the `viewBox` width and
  height at least 0.00001. Outside these the SVG fails the render with `INVALID_IMAGE`; a nested
  SVG image is left out.

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
`Field` is any of the four, picked by its `kind` (`"text"`, `"checkbox"`, `"radio"`,
`"select"`), for forms built from data: `<Field kind={q.kind} name={q.name} … />`.

## Debugging layout

```tsx
<View debug className="m-4 border-2 p-3">…</View>   // one element
<Page debug>…</Page>                                // the page and everything on it
<Document debug>…</Document>                        // every page
```

`debug` draws an element's boxes over the page, like the box model in browser developer tools:
the margin in orange, the border in yellow, the padding in green, the content box as a blue
outline, and the element's width and height in points as a small label. It works on `View`,
`Text`, `Image`, `Svg`, `Canvas`, `Table`, `Row`, `Cell`, `Spacer`, the fields, `Header` and
`Footer`; on a `Page` it also outlines the page margins. A box split across pages is outlined
on each page it crosses, and a header or footer on every page.

It outlines elements with a box of their own. A `Text` or `Link` inside a `Text` is a run of that
paragraph's lines, not a box, so `debug` on it draws nothing: put it on the outer `Text`, which
outlines the whole paragraph.

The outlines are drawn last, on top of the content, and the labels are drawn with lines, so
text selection, search, links, bookmarks and form fields are unchanged. It changes the output,
so remove it before you ship.

## API

```ts
renderToBuffer(element, options?): Promise<Uint8Array>   // the PDF bytes
renderToFile(element, path, options?): Promise<void>      // renders, then writes the file
renderToStream(element, options?): Promise<Readable>     // the PDF as a Node stream
```

`renderToStream` returns a stream to pipe into an HTTP response:
`(await renderToStream(<Invoice />)).pipe(res)`. The stream starts once the PDF is ready, and a
failed render rejects the promise.

All three take the same options:

| Option | Default | What it does |
|---|---|---|
| `images` | none | Image bytes by id, for `<Image src="logo">`. An id wins over a path or URL with the same text. |
| `localImages` | off | `{ root: '/srv/app/images' }` or `{ root: [dirA, dirB] }`: folders `<Image src="./logo.png">` may read files from. A relative path is resolved against the first. `maxBytes` (20 MiB) limits each file. The `INKOXIDE_LOCAL_IMAGES` environment variable (folders separated by `:`, `;` on Windows) sets it for every render. |
| `remoteImages` | off | `{ allow: ['https://cdn.example.com/', '*.example.org'] }`: hosts `<Image src="https://…">` and `<Image src={{ uri }}>` may load from. A URL entry allows its origin (scheme, host and port), and, with a path, that path and the paths under it: `https://cdn.example.com/img` allows `/img/logo.png`, not `/imgs/logo.png`. `timeoutMs` (10 000; `Infinity` for no limit) and `maxBytes` (20 MiB, 20 971 520 bytes) limit each image, and redirects are not followed. The `INKOXIDE_REMOTE_IMAGES` environment variable (comma-separated) sets it for every render. |
| `imageTimeoutMs` | `10000` | How long a function or promise `<Image src>` may take to settle; `Infinity` waits without a limit |
| `pdfA` | `false` | Write PDF/A-2b, for archiving |
| `compress` | `true` | Compress the page content |
| `subsetFonts` | `true` | Embed only the glyphs the document uses |
| `creationDate` | none | RFC 3339 timestamp written to the PDF. Without it, the same input gives byte-identical output. |
| `threads` | all cores | How many cores one render spreads its pages over: an integer from 1 to 256 |
| `validate` | outside production | Check the document tree first, for clearer errors |

A bad option fails the render with `ENGINE_REJECTED` at its path (`options.threads`,
`options.remoteImages.timeoutMs`): a `creationDate` that is not RFC 3339, a `threads` that is
not an integer from 1 to 256, or a timeout or `maxBytes` that is not a number from 0 (NaN, a
negative number, a string). A fractional timeout is rounded up to a whole millisecond.

**`Font.register({ family, src })`** adds a font family, from a path, a URL or bytes. See
[Fonts](#fonts).

**`Font.registerHyphenationCallback(fn)`** splits words for hyphenation with your own function;
`null` removes it. See [Text: hyphenation and direction](#text-hyphenation-and-direction).

**`StyleSheet.create(styles)`** returns the styles unchanged, with their types.

**`init()`** loads Tailwind's compiler (when `@inkoxide/tailwind` is installed), the engine and
the fonts. The first render does it anyway; call it at server startup so the first request does not wait (about 150 ms). It resolves
to the same status as `engineStatus()`.

**Health checks.** `version()` returns `{ engine, schema, gitSha }`: the engine's version, the
document schema version it reads and the commit it was built from. `engineStatus()` returns
`undefined` until the engine has started (the first `init` or render), then `{ version, fonts,
hyphenation, warnings }`: the font ids registered, which hyphenation languages loaded
(`{ en: true }`), and start-up warnings such as a missing bundled font.

```ts
app.get('/health', (_req, res) => {
  const status = engineStatus();
  res.status(status ? 200 : 503).json(status ?? { starting: true });
});
```

**`countPages(pdf)`** is the page count of a PDF made by `renderToBuffer`. It reads the page
tree the engine writes, so it is not a general PDF parser.

**Types.** The props of a component are exported as a type named after it (`DocumentProps`,
`TextProps`, `TextFieldProps`, `FieldProps`, `SvgProps`, `CanvasProps` and so on); `PolyProps`
serves `Polyline` and `Polygon`, `GradientProps` both gradients and `SvgPresentation` the
attributes every SVG shape takes. Also exported:

- styles and values: `Style`, `StyleProp`, `AbsoluteLength`, `Length`, `LengthPercent`,
  `LengthAuto`, `PageSize`, `PagePreset`, `BreakProp`, `BookmarkProp`, `FontWeightName`;
- `render` props and images: `RenderProp`, `PageInfo`, `ImageSrc`, `ImageSrcValue`,
  `ImageRequest`, and `Painter`, the painter a `Canvas` draws with;
- options and results: `RenderToBufferOptions`, `LocalImageOptions`, `RemoteImageOptions`,
  `FontRegistration`, `FontSource`, `FontSrc`, `FontWeight`, `HyphenationCallback`,
  `EngineStatus`, `EngineVersion`, `RenderErrorCode` and `DataIssue`.

**Concurrency.** A render runs on Node's thread pool, so many can run at once. `threads` sets
how many cores one render uses; with `threads: 1` each render stays on its own pool thread.
Node's `UV_THREADPOOL_SIZE` (default 4) caps how many renders run at the same time.

## Errors

Mistakes fail loudly, with the component that caused them. A `View` inside a `Text`:

```tsx
<Text>Total: <View /></Text>
```

stops the render with:

```
RenderError: <View> cannot be inside <Text>. A Text contains only strings, numbers,
<Text>, <Link>, <PageNumber> and <TotalPages>.
    in View
    in Text
    in Page
    in Document
    in Receipt
```

Every error of `renderToBuffer`, `renderToStream`, `renderToFile` and `init` is a `RenderError`
with a `code` and an HTTP `status`, and with `path`, `component` and `componentStack` where they
apply:

| Code | What happened |
|---|---|
| `TEMPLATE_ERROR` | A component used in the wrong place, a prop with an invalid value (a bad `Page` `size`, a `Link` without `href`), an unknown class or style key, a component or `render` prop that throws, or an image `src` that cannot be used: a URL outside `remoteImages`, a path while `localImages` is off (from a function), or a function or promise that fails or does not settle in `imageTimeoutMs` |
| `INVALID_TREE` | The document tree does not match the engine's schema. Checked only with `validate`, which is on outside production |
| `INVALID_FONT` | A registered font could not be read, fetched or parsed |
| `ENGINE_REJECTED` | The engine refused the document, for example an unreadable image, or a render option is out of range (`path` is `options.threads`, `options.creationDate`, …) |
| `IMAGE_FETCH_FAILED` | An allowed remote image could not be fetched: an HTTP error, a redirect, a timeout, a dropped connection or over `maxBytes` |
| `IMAGE_READ_FAILED` | An image file could not be read: missing or outside the `localImages` folders, a network path, not a valid `file:` URL, not a regular file, over the size limit, permission denied, or a `localImages` folder that cannot be opened |
| `INTERNAL` | The engine could not start (no fonts in the assets folder, `inkoxide` and `@inkoxide/schema` versions that do not match), `renderToFile` could not write the file, or a bug |

`INTERNAL` has status 500, `IMAGE_FETCH_FAILED` 502 and the others 400. Besides `code`,
`status`, `message`, `path`, `component` and `componentStack`, a `RenderError` may have
`className` (the Tailwind class that failed), `engineCode` (the engine's own code, for
`ENGINE_REJECTED` and some `INTERNAL` errors) and `issues` (for `INVALID_DATA`). It turns into
JSON with only its fields that are set (`res.status(e.status).json(e)`): `code`, `message`,
`path`, `component`, `componentStack`, `className`, `engineCode` and `issues`, but not `status`
or the stack. Two more codes come from [templates](#serving-templates): `INVALID_DATA` (400) and
`UNKNOWN_TEMPLATE` (404).

`Font.register` and `Font.registerHyphenationCallback` check their arguments when you call them
and throw a plain `Error` straight away: a missing family name or `src`, a `src` that is not a
path, a URL or bytes, an unknown weight, a face already registered from another source, a face
the bundled fonts have, or a family whose font id another family has.

## Serving templates

An advanced API, for a server that renders templates by id from JSON data, such as
`POST /pdf/invoice`. It lives in its own entry, `@inkoxide/render/templates`, and checks the
data with a [zod](https://zod.dev) schema before React renders anything.

```tsx
import { readFileSync } from 'node:fs';

import { Document, Image, Page, RenderError, Text } from '@inkoxide/render';
import { generate, type TemplateLike } from '@inkoxide/render/templates';
import { z } from 'zod';

const schema = z.object({ number: z.string(), total: z.number() });

const invoice: TemplateLike<typeof schema> = {
  meta: { id: 'invoice' },
  schema,
  Template: ({ data }) => (
    <Document>
      <Page>
        <Image src="logo" className="h-10 w-10" />
        <Text>Invoice {data.number}: {data.total}</Text>
      </Page>
    </Document>
  ),
  images: { logo: readFileSync('logo.png') },
};

const templates = new Map<string, TemplateLike>([[invoice.meta.id, invoice]]);

app.post('/pdf/:id', async (req, res) => {
  try {
    const template = templates.get(req.params.id);
    if (!template) throw new RenderError('UNKNOWN_TEMPLATE', `No template "${req.params.id}".`);
    const { pdf } = await generate(template, req.body);
    res.type('application/pdf').send(Buffer.from(pdf));
  } catch (e) {
    if (!(e instanceof RenderError)) throw e;
    res.status(e.status).json(e);
  }
});
```

A template is `{ meta: { id }, schema, Template, images? }`: the component receives the parsed
data as `data`, and `images` are its static images by id (a logo). An image passed in the render
options under the same id replaces the template's.

- **`generate(template, data, options?)`** checks `data`, renders and resolves to
  `{ pdf, pages, passes, bytes, timings, fonts, warnings, document, images, schemaVersion,
  engineVersion }`. `timings` has the milliseconds of each stage, each counted once: `reactMs`,
  `treeMs` (with the rebuilds for the page count), `validateMs` (0 when `validate` is off),
  `engineMs`, `fetchMs` and `totalMs`. `warnings` are the document's font fallbacks and `render`
  props that did not settle; start-up warnings are in `engineStatus()`. `document` is the tree that was rendered,
  and `images` the images it was rendered with, by id: the tree names an image given as bytes,
  a file, a URL or a function by an id it made up (`image-0`, `remote-0`), and `images` has the
  bytes behind those ids. The options are those of `renderToBuffer`.
- **`parseData(template, data)`** only checks the data, and returns it parsed, with the
  schema's defaults applied.
- **`renderTree(template, data, options?)`** builds the document tree without rendering it, for
  snapshots and previews. Its `document` is the tree, and its `images` the images it uses. It is
  synchronous, so it cannot load what a render loads first: `await init()` before it, and again
  after `Font.register`. Otherwise it throws an `INTERNAL` RenderError that says so.
  `renderSettled(tree, images, options?)` renders it, and renders again with the page count
  while `render` props change it; `images` are more images by id (`{}` when there are none), and
  it resolves to `{ pdf, pages, passes, tree, images, fetchMs, treeMs }` (`treeMs`: the
  rebuilds for the page count). Pass both the same options.
- **`renderDocument(document, images?, options?)`** renders a document tree that is already
  built, such as a stored `result.document`, and resolves to `{ pdf, pages, engineMs }`. The
  document names its images by id, so pass their bytes in `images` or the `images` option
  (`renderDocument(result.document, result.images)`); the other image options do not apply.

Data that does not match the schema fails with `INVALID_DATA` (400) before anything renders. Its
`issues` list every problem as `{ path, message, code }`, with readable messages:
`{ path: 'items[0].qty', message: 'Missing: expected a number', code: 'invalid_type' }`.
`UNKNOWN_TEMPLATE` (404) is never thrown by the package; it is there for your own lookup, as
above, so that every error has the same shape.

The entry also exports the types of these: `TemplateLike` (a template), `GenerateOptions` (the
options of `generate`), `RenderResult` (what `generate` resolves to), `Timings` (its `timings`),
`TreeResult` (what `renderTree` returns) and `RenderOpts` (the options of `renderSettled` and
`renderDocument`).

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

**Every render fails with "No fonts found … Set INKOXIDE_ASSETS_DIR".**
The bundled fonts and hyphenation patterns are files in the package's `assets/` folder
(`node_modules/@inkoxide/render/assets`), found next to its code. When a bundler (esbuild,
webpack) moves the code into one file, for a serverless function say, they are no longer there.
Copy that folder into the deployment and point the `INKOXIDE_ASSETS_DIR` environment variable at
the copy: a folder with `fonts/` (the bundled `.ttf` and `.otf` files) and `hyphenation/`
(`hyph-en-us.tex`). Without the hyphenation patterns, text still renders with the engine's
built-in ones, and `engineStatus().warnings` says so.

**Do I need Tailwind installed?**
Only to style with `className`: then `npm install @inkoxide/tailwind`. It brings the Tailwind
compiler, so there is no config and no CSS build. Without it, style with `style`.

**Can I use my own components and hooks?**
Yes. A document is ordinary React: components, props, context, `.map` and conditionals all work,
as do hooks that compute during render (`useMemo`, `useContext`). The PDF is built from that
first render, so state updates and effects do not change it, and nothing can wait for data:
a component that suspends (`use()` of a promise, a `lazy` component, inside `<Suspense>` or
not) fails the render with `TEMPLATE_ERROR`. Load the data first and pass it in as props.

**How much room does a page number take?**
Its own digits. In a header, a footer or a fixed element, the text is laid out again on every
page, so `<Text className="text-right">Page <PageNumber /> of <TotalPages /></Text>` ends at
the footer's padding on every page. In the page body, the text is laid out with the numbers of
the page it lands on, and the page breaks are worked out again if those numbers move it. The
exception is text the body repeats on several pages, a table's header rows: it is sized for a
three-digit number, each page draws its own digits in that room, and a number of four digits
or more can overflow it.

**Is the output the same on every machine?**
Yes, for the same input, fonts and options, as long as you leave out `creationDate`.

## Limitations

- **Server only.** The engine is a native addon: there is no browser rendering, and it does not
  run on edge runtimes such as Cloudflare Workers.
- **Drawing.** Colour emoji and text inside `<Svg>` are not supported yet. SVG filters are not
  applied, and an SVG only shows images embedded as `data:` URLs (see
  [Drawing](#drawing-svg-and-canvas)).
- **Content taller than a page.** What cannot split (an `Image`, an absolutely positioned,
  rotated or transformed element, table rows tied by a `rowSpan`) is cut at the page edge when
  it is taller than a page.
- **Tailwind.** Variants (`md:`, `hover:`), shadows and gradients are not supported.
- **Output.** The engine produces the whole PDF at once: `renderToStream` streams the finished
  file, it does not send pages while later ones are still being laid out.
