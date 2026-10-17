# @inkoxide/render guide

The reference for @inkoxide/render. The [README](README.md) covers installing, the quick start
and the examples.

- [Components](#components)
- [Styling](#styling)
- [Fonts](#fonts)
- [Images](#images)
- [Pages, headers and footers](#pages-headers-and-footers)
- [Tables](#tables)
- [Links, bookmarks and metadata](#links-bookmarks-and-metadata)
- [Text: hyphenation and direction](#text-hyphenation-and-direction)
- [Text: superscripts and font features](#text-superscripts-and-font-features)
- [Drawing: SVG and Canvas](#drawing-svg-and-canvas)
- [Forms](#forms)
- [Notes](#notes)
- [Debugging layout](#debugging-layout)
- [API](#api)
- [Errors](#errors)
- [Serving templates](#serving-templates)
- [In the browser](#in-the-browser)
- [FAQ](#faq)
- [Limitations](#limitations)

## Components

Components take `className`, `style` and `id`, and those with a box also take `debug`
([Debugging layout](#debugging-layout)), with these exceptions: `Document` takes only `debug`;
`Page` has no `id`; `PageBreak` takes only `id`; `PageNumber` and `TotalPages`
take no props; the shapes inside an `Svg` take SVG attributes (`fill`, `stroke`,
`transform`, `id`) instead of `className` and `style`; and a `Text` or `Link` inside a `Text`,
a run of the paragraph, takes only text styles (with `verticalAlign: 'super'` or `'sub'`),
`href`, `lang`, `hyphenate` and `hyphenationCallback` (props such as `id`, `bookmark`, `pages` or `render` are an error there).

| Component | What it is |
|---|---|
| `Document` | The PDF: `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `lang` (`"en-US"`), `creationDate`, `pageMode`, `pageLayout` and `fallbackFonts` ([metadata](#links-bookmarks-and-metadata)). |
| `Page` | One page layout; content flows over as many sheets as it needs. `size` (a [preset](#page-sizes) such as `"A4"`, `"letter"` or `"A4-landscape"`, `[width, height]`, `{ width, height }`, or `[width]` or one length for a square), `orientation`, `margin`. Padding on a page is its margin; a percentage is of the page width, on every side, as in CSS. |
| `View` | A box: flex column by default, and also grid. |
| `Text` | A paragraph. A `Text` inside a `Text` is an inline run (bold, colour, link). |
| `Link` | A link (`href`, or `src`): a linked run inside a `Text`, a box like a `View` elsewhere. |
| `Image` | JPEG, PNG, GIF, WebP or SVG. `fit` (or `objectFit` in its style): `fill`, `contain`, `cover`, `none`; `objectPosition` in its style places the fitted image. |
| `ImageBackground` | A `View` with an image behind its children: `src` and `fit` as on `Image` ([Images](#images)). |
| `Header`, `Footer` | Repeated on every page, in the top or bottom margin. |
| `PageNumber`, `TotalPages` | The current page and the page count, inside a `Text`. |
| `Table`, `Row`, `Cell` | A table that splits across pages. |
| `PageBreak`, `Spacer` | A forced page break; empty space. |
| `Svg`, `Path`, `Rect`, `Circle`, `Ellipse`, `Line`, `Polyline`, `Polygon`, `G`, `Defs`, `ClipPath`, `Marker`, `LinearGradient`, `RadialGradient`, `Stop`, `Tspan`, `TextPath` | Vector drawing in JSX; a `Text` in an `Svg` is SVG text. |
| `Canvas` | Drawing in code: `paint={(painter, width, height) => …}`. |
| `TextField` (or `TextInput`), `Checkbox`, `Radio`, `Select`, `List`, `Field` | Fillable form fields (AcroForm). |
| `FieldSet` | A `View` that groups the form fields inside it under its `name`. |
| `Note` | A text note: an icon that opens its text in the viewer. |

**The one rule:** a `Text` contains strings, numbers, `PageNumber`, `TotalPages`, `Link`,
`Image` (an image in the line), `Note` and other `Text`, and nothing else. A `View` inside a `Text` is an error that names the component.

## Styling

### Style objects

```tsx
<View style={{ padding: 12, borderBottomWidth: 0.5, borderBottomColor: '#ddd' }} />
<View style={[styles.row, index % 2 === 1 && styles.zebra]} />
<Text style={{ fontSize: '9pt', marginTop: '2mm' }} />
```

- **Units.** Numbers are points (1/72 inch). Strings may use `pt`, `mm`, `cm`, `in`, `px`
  (0.75pt), `pc` (12pt), `rem` (12pt, as in Tailwind), `em` (the element's font size; in
  `fontSize`, the inherited one; in `lineHeight` and `letterSpacing`, the size of the text they
  apply to, so they scale with each run), `vw` and `vh` (1% of the page's width and height), or a
  percentage. `lineHeight` is a multiplier of the text size (`1.4`, `'140%'`, `'1.4em'`) or a
  length (`'14pt'`). `calc()` adds lengths (`'calc(10mm + 1em)'`) or percentages, but not one to the
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
  `inset` sets `top`, `right`, `bottom` and `left` at once (one value, or
  `{ top, right, bottom, left }`), and those keys win over it.
- **Indents.** `textIndent` (`indent-*`) indents the first line of a `Text`. A negative value is
  a hanging indent: the first line starts that far before the others, so give the `Text` as
  much `paddingLeft` (`-indent-4 pl-4`).
- **CSS names.** `textDecoration` takes `'underline'`, `'line-through'` or both; a run has one
  line, so `'underline line-through'` draws the line-through. `textDecorationColor`,
  `textDecorationStyle` and `textDecorationThickness` are the same as `decorationColor`,
  `decorationStyle` and `decorationThickness`. `textOverflow: 'ellipsis'` is `ellipsis: true`
  (with `maxLines`). `objectFit` on an `Image` or `ImageBackground` is its `fit` prop, which
  wins when both are set. `objectPosition` on them places the image in its box (see
  [Images](#images)). Both throw on other elements.
- **Floats.** `float: 'left'` or `'right'` (`float-left`, `float-right`) on a child of a column
  `View`, a `Page` or a `Cell` puts it at that side, as narrow as its content unless it has a
  width, at or below where it comes in the flow. Text after it wraps beside it line by line, in
  its own box: a `Text` with a width, an `alignSelf` or auto side margins keeps the box it would
  have without the float, and its lines take the room the float leaves in it. A box after it is
  narrowed to the room beside the floats over its whole height, or moves below them when its
  content does not fit there. `clear: 'left'`, `'right'` or `'both'` moves an element below the
  floats before it, and a parent is as tall as its floats. A float moves to the next page with
  the lines beside it. Floats are rectangles (`shapeOutside` is not supported), do not float
  inside rows or grids, and a box with floats lays its children out top to bottom: `flexGrow`,
  `flexBasis`, `justifyContent` and `alignContent` are errors there.
- **Opacity.** `opacity` applies to each element, not to an element and its children as one
  group, as CSS does: where children overlap, each shows through the others. `opacity: 0`
  draws nothing, and leaves out the links, form fields and notes of everything inside the
  element.
- **Inheritance.** Text properties (font, size, weight, colour, line height, alignment) pass down
  from `Page` and `View` to the text inside. Box properties apply only to the element that has
  them.
- **Page media.** An `'@media …'` key holds styles for the pages it matches, so one component
  can fit the page it lands on:

  ```tsx
  const styles = StyleSheet.create({
    grid: {
      flexDirection: 'column',
      '@media (orientation: landscape)': { flexDirection: 'row', gap: 24 },
      '@media (width < 150mm)': { fontSize: 9 },
    },
  });
  ```

  A query reads the page: `width` and `height` (the whole sheet, margins included, as `vw` and
  `vh` do), `aspect-ratio` and `orientation` (a square page is portrait), with `min-`/`max-`
  or the range forms (`(400pt <= width < 600pt)`), joined by `and`, `or`, `not` and commas.
  `print` and `all` match, `screen` does not. Lengths take `pt`, `mm`, `cm`, `in`, `px`, `pc`
  and `rem`, and are compared to 1/100 pt, so `(max-width: 210mm)` holds on A4. Blocks apply
  after the object's own properties, in the order written, and may nest (all their queries must
  match); a later entry of a style array still wins. A block for another page size is read too,
  so a mistake in it fails on every page. Features of screens (`hover`, `prefers-color-scheme`,
  `resolution`) are errors. Each `Page` has one size, so the queries are answered once per
  page, before layout; a JSON document has no `@media` (its author knows each page's size).

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
- **Page variants** read the page as `@media` does: `md:p-4`, `max-sm:text-xs`,
  `min-[200mm]:flex-row`, `landscape:flex-row`, `print:hidden`, and stacked, `sm:landscape:`.
  Breakpoints are Tailwind's: `sm` 480pt, `md` 576pt (A4 and Letter portrait are wider), `lg`
  768pt (their landscape is wider), `xl` 960pt, `2xl` 1152pt; a theme can add its own
  (`--breakpoint-a4: 595.28pt` gives `a4:` and `max-a4:`).
- **Not supported:** variants for screens and states (`hover:`, `focus:`, `dark:`), container
  queries (`@sm:`), shadows, gradients and `space-*`/`divide-*` (use `gap-*` and borders). Using one throws and names the class; in production (`NODE_ENV=production`) it is logged once and ignored.
- **In a browser**, the Tailwind compiler is about 90 KB to download with brotli. The build
  plugin of `@inkoxide/tailwind` replaces it with a table of the classes your sources use, a
  few KB ([`@inkoxide/tailwind` README](../tailwind/README.md#in-a-browser-bundle)):
  `plugins: [inkoxideTailwind()]` in a Vite config. Then write each class whole: one put
  together at run time (`` `bg-${color}-500` ``) is not in the table, unless it is in the
  plugin's `safelist`.

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
  (`.woff2`). Font collections (`.ttc`) are not supported: register each font as its own file.
- **Loading.** `register` is synchronous. The file is read or fetched just before the next
  render, and a font that cannot be loaded fails that render with `INVALID_FONT`. The fonts
  registered with it still load, and later renders can use them. The face that failed is then
  dropped, and is not tried again: later renders succeed, and text in it falls back as for a
  face never registered (to the nearest face of its family, else to `Inter`), with only a
  warning in the `warnings` of `generate`. Register it again, from the same source or another,
  to try once more. To refuse to render without it, load your fonts at startup with
  `await init()` and stop when it throws. A URL is fetched with a
  30-second limit for the whole download and a 50 MiB size limit, and redirects are followed;
  renders wait for it, so for a font over that size or from a slow host, read the bytes
  yourself and register them.
- **Weights.** Weights are numbers (1–1000, as in CSS) or names (`'light'`, `'medium'`,
  `'bold'`). Missing weights and styles fall back to the nearest one, as in CSS.
- **Variable fonts.** Register a variable font with the range of weights it covers, as CSS
  `@font-face` does:

  ```tsx
  Font.register({ family: 'Inter Var', src: './fonts/InterVariable.ttf', fontWeight: [100, 900] });
  Font.register({
    family: 'Inter Var',
    src: './fonts/InterVariable-Italic.ttf',
    fontWeight: [100, 900],
    fontStyle: 'italic',
  });
  ```

  Each weight in the range draws at that `wght` (`fontWeight: 450` is the 450 instance), and a
  weight outside it at the nearer end. `fontVariationSettings` sets other axes, as in CSS:
  `"'wdth' 75, 'opsz' 32"` or `{ wdth: 75 }`, inherited like other text styles; its `wght` wins
  over `fontWeight`, and on a face registered with one weight it picks an instance too. A face
  can take `fontVariationSettings` in `Font.register` (`{ ital: 1 }`), so one file can be the
  italic as well. Each combination a document uses is a static instance, embedded as TrueType
  (CFF2 outlines too), so viewers show exactly what was laid out. TrueType (`gvar`) and CFF2
  variable fonts work; one with more than 63 axes or an `avar` table of version 2 is refused
  when it loads. Text in an `<Svg>` takes the nearest instance the document declares.
- **Colour emoji.** Register a colour emoji font, such as Noto Color Emoji, and list it in
  [`fallbackFonts`](#links-bookmarks-and-metadata):

  ```tsx
  Font.register({ family: 'Noto Color Emoji', src: './fonts/Noto-COLRv1.ttf' });

  <Document fallbackFonts={['NotoSans-Regular', 'Noto-Color-Emoji-400']}>…</Document>
  ```

  An emoji (`😀`, `❤️`, a flag, a keycap, a skin tone, a sequence such as `👩‍💻`) goes to the first
  colour font in `fallbackFonts` that has it, then to the run's font, then to the other
  fallbacks. Followed by U+FE0E (text presentation), or as a character that is text by default
  (`❤`, `©`), it is text like any other. Colour glyphs come from the font's `COLR` table
  (versions 0 and 1) or its PNG bitmaps (`CBDT`, as in Noto Color Emoji's bitmap build, or
  `sbix`); a font with bitmaps and no outlines works too, but cannot be a form field's font. They
  are drawn as images that copy and search as their text. `COLR` layers in the foreground colour
  take the run's `color`, as in browsers (in the text of an SVG image, black); the colour's alpha
  fades the whole glyph. Gradients that sweep around a point
  draw with their first colour, and blend modes draw as normal. A colour glyph that is malformed
  or takes too much work to draw (over 10 000 drawing steps) draws without colour, and so do
  colour glyphs past the first 4096 of a render (one that takes the run's colour counts once per
  colour) or its first 32 MB of colour glyph images. In a
  font with bitmaps and no outlines, which has nothing else to draw them with, such glyphs are
  not drawn, but they still copy and search as their text. Fonts
  with only `SVG` glyphs are not drawn in colour. PDF/A-1b forbids transparency: there, a colour glyph that needs it (a bitmap with an
  alpha channel, as in every emoji bitmap font, or a layer with an opacity) fails the render with
  `PDFA_CONFLICT` at the path of its run (at the `src` of an SVG whose text has it), and inline
  images and emoji images that need it fail it at their `src` or `emojiImages` key. Opaque `COLR` glyphs draw in colour there.
- **Emoji from images.** `Font.registerEmojiSource` draws emoji from an image set instead:
  `{ url, format }` adds each emoji's code points and the format to the URL
  (`https://cdn.example.com/emoji/1f469-200d-1f4bb.png`; `withVariationSelectors: true` keeps
  `fe0f` in them), and `{ builder }` turns the code points into any image `src`. The images load
  like `<Image src>`: a URL must be allowed by `remoteImages` (else the render fails, naming the
  emoji), a path needs `localImages`, and images fetched from URLs are kept between renders (16
  MB of them, each render still holding them to its own `remoteImages.maxBytes`). An emoji image
  is 1em high with its aspect ratio, its bottom 0.1em below the baseline (raised or lowered
  with the text by `verticalAlign: 'super'` or `'sub'`), and copies as the emoji. An emoji whose image cannot be loaded, or is not an image the engine can draw (other
  bytes, an image over the size limits), is drawn by the fonts, with a warning; a ZWJ
  sequence (`👩‍💻`) without an image of its own uses the images of its parts. `null` removes the
  source.

  ```ts
  Font.registerEmojiSource({ url: 'https://cdn.example.com/emoji/72x72/', format: 'png' });
  ```

- **Faces.** A face is a family, a weight and a style. Registering one again from the same
  source (the same path, URL or bytes) does nothing, and from another source throws. A face the
  bundled fonts have (`Inter` 400) cannot be replaced: register your build under another family
  name. You can add the faces they lack, such as `Inter` bold italic. Two family names that
  differ only in spaces and hyphens (`Brand Two`, `Brand-Two`) give the same font id to faces of
  the same weight and style, so such a face cannot be registered under both: `Brand Two` 400 and
  `Brand-Two` 400 throw, `Brand Two` 400 and `Brand-Two` 700 do not.

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

- **Size.** An `Image` with no `width` or `height` is as big as the image is (96 dpi), but no
  wider than the room it has: when the image is wider, it is scaled down to fit, keeping its
  aspect ratio. CSS would let it overflow. With one of them, the other follows the ratio.
- **Alternative text.** `alt` is the text the image copies and searches as, and what screen
  readers say for it: `<Image src={chart} alt="Revenue by month, rising from 12k to 30k" />`. It
  takes at most 1024 characters; `<Svg>` and `<Canvas>` take it too.
- **Images in text.** An `Image` inside a `Text` sits in the line, like a character: lines break
  before and after it. With neither `width` nor `height` it is 1em high (the text's size) and as
  wide as its aspect ratio gives; with one of them (in points or em), the other follows the
  ratio. Percentages and box styles (margins, padding, borders) are errors there. `fit`,
  `objectFit`, `objectPosition` and `alt` work as on a block image, and a `Link` around it makes
  it a link. `verticalAlign` places it: `baseline` (the
  default, its bottom on the baseline), `middle` (its middle half an x-height above the
  baseline), `text-top` and `text-bottom` (its top at the font's ascender, its bottom at its
  descender), `top` and `bottom` (at the line's top or bottom), `super` and `sub`, or a number of
  points its bottom sits above the baseline. The line grows to fit it. SVG images work too.

  ```tsx
  <Text>
    Rated <Image src={star} style={{ height: '0.9em', verticalAlign: 'middle' }} alt="4 stars" /> by
    <Link href="https://example.com"><Image src={logo} className="align-text-bottom" /></Link>
  </Text>
  ```

- **Background images.** `ImageBackground` is a `View` with an image behind its children. It
  takes the props of a `View` and `src` in any form `Image` takes; `fit`, `objectFit` and
  `objectPosition` (and the `object-*` classes) place the image as on an `Image`, `fill` by
  default. The image fills the padding box, inside the borders, as a CSS `background-image`
  does: it is drawn over the background colour and under the borders and the children, and
  clipped to the box, rounded by `borderRadius`. A box split across pages is sliced with its
  image, like its background colour. `source`, `imageStyle` and `srcSet` throw: pass the image
  as `src`, and place it with `fit` and `objectPosition`.

  ```tsx
  <ImageBackground src={photo} fit="cover" className="h-48 rounded-xl p-6 object-top">
    <Text className="text-2xl font-bold text-white">Annual report</Text>
  </ImageBackground>
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
its files or fetch arbitrary URLs. Allow only the folders and hosts your images are in. An entry
allows only the default port of its scheme unless it names one (`cdn.example.com:8443`). Remote
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

- **Columns.** Widths are fractions (`'1fr'`), points, or `'auto'`: as wide as
  its content, narrowed toward its longest word when the room runs short.
- **Borders.** Adjacent cell borders merge into one by default (`borderCollapse: 'collapse'`):
  the wider wins. With `borderCollapse: 'separate'` (`border-separate`), each cell draws its own
  borders, background and radius, `borderSpacing` (`border-spacing-*`) apart and from the
  table's edge. Both go on the `Table`.
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

Bookmarks become the PDF outline in the viewer's sidebar.

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
with `Font.register`, whose id is its family with spaces as hyphens, its weight (or a variable
face's range, `Inter-Var-100-900`), and `-italic` for an italic face:

```tsx
Font.register({ family: 'Noto Sans Arabic', src: './fonts/NotoSansArabic-Regular.ttf' });

<Document fallbackFonts={['NotoSans-Regular', 'Noto-Sans-Arabic-400']}>…</Document>
```

An id that is not registered is skipped, and `fallbackFonts={[]}` turns the fallback off.

### PDF/A

`conformance` makes the file PDF/A, for archiving, and so does the `pdfA` render option, which
wins over it:

```tsx
<Document title="Invoice 2026-0815" conformance="PDF/A-2b">…</Document>
```

| `conformance` | `pdfA` | Writes |
|---|---|---|
| `'PDF/A-2b'` or `'PDF/A-2'` | `true` or `'2b'` | PDF/A-2b |
| `'PDF/A-3b'` or `'PDF/A-3'` | `'3b'` | PDF/A-3b, as 2b, with [attachments](#attachments) (an e-invoice's XML) |
| `'PDF/A-1b'` or `'PDF/A-1'` | `'1b'` | PDF/A-1b, the oldest level, for archives that require it |
| `'PDF/A-2a'` | `'2a'` | PDF/A-2b, [tagged](#accessibility) (accessible) |
| `'PDF/A-3a'` | `'3a'` | PDF/A-3b, tagged |

PDF/A-1b has no transparency, so a render fails with `ENGINE_REJECTED` (`engineCode`
`PDFA_CONFLICT`) and the component at fault for a colour with alpha below 1 (a transparent
background is fine: it draws nothing), an `opacity` between 0 and 1, an image with an alpha
channel such as most PNG logos, an SVG drawn with opacity, a mask or a blend mode, or `debug`.
PDF/A-2b and 3b allow all of these. The u levels (`'PDF/A-2u'`) and PDF/A-1a are not
supported: they fail with `TEMPLATE_ERROR`.

`conformance` is not part of the document tree: to render a stored `result.document` again at
the same level, pass `pdfA` to [`renderDocument`](#serving-templates), from
`@inkoxide/render/templates`.

### Passwords and permissions

`userPassword` encrypts the PDF (AES-256, which Acrobat X and later, pdf.js, Chrome and Preview
open): the file opens only with that password. `ownerPassword` lifts `permissions`, what a
reader without it may do:

```tsx
<Document
  userPassword="open-sesame"
  ownerPassword={process.env.PDF_OWNER_PASSWORD}
  permissions={{ printing: 'highResolution', copying: false }}
>
  …
</Document>
```

| Permission | Default | Allows |
|---|---|---|
| `printing` | not allowed | `'lowResolution'` or `'highResolution'` printing |
| `modifying` | `false` | changes other than annotations and form fields |
| `copying` | `false` | copying text and images |
| `annotating` | `false` | adding annotations and filling in fields |
| `fillingForms` | `false` | filling in fields |
| `contentAccessibility` | `true` | extracting text for screen readers |
| `documentAssembly` | `false` | inserting, rotating and deleting pages |

- Passwords are printable ASCII, at most 127 bytes. With only `ownerPassword`, anyone opens the
  file and the permissions apply; with only `userPassword`, it is also the owner password, so
  `permissions` need an `ownerPassword` other than `userPassword`.
- The `encryption` render option (`{ userPassword, ownerPassword, permissions }`) replaces the
  three props as a whole, for passwords that come from the request rather than the template.
- Encryption and PDF/A do not go together: PDF/A forbids encryption.
- A bad value fails with `ENGINE_REJECTED` at its path (`options.encryption.userPassword`, …).
  No error shows a password, and the passwords are not part of `result.document` or of the
  tree's JSON.
- Readers enforce the permissions; the file cannot. Encryption hides the text and images, not
  the page count or sizes. Field values are hidden, but not which checkboxes are checked, nor
  the positions of the chosen radio button and list box options.
- The same document and passwords give the same bytes, as unencrypted output does.

## Text: hyphenation and direction

```tsx
<Document lang="en-US">
  <Page>
    <Text className="text-justify" hyphenate>Long paragraphs break words at syllables.</Text>
  </Page>
</Document>
```

- **Hyphenation** is off by default. Turn it on per `Text` with `hyphenate` (or the class
  `hyphens-auto`); it follows the `lang` of the text or the document. With neither, nothing is
  hyphenated, and the `warnings` of `generate` say so. US English patterns ship
  with the package and serve every English `lang`; other languages use the engine's built-in
  patterns. A paragraph is hyphenated in one language, so a `Text` inside a `Text` with another
  `lang` is not hyphenated by patterns: make it a paragraph of its own, or give it a
  `hyphenationCallback`. A soft hyphen
  (`\u00AD`) in the text is always a possible break.
- **Words wider than the line.** A word that does not fit its line is broken between any two
  characters, hyphenated or not, as CSS does with `overflow-wrap: anywhere` (never inside a
  grapheme cluster, such as a letter with its accent or an emoji). `[word]` from a hyphenation
  callback keeps a word from being hyphenated, but not from being broken this way. Text with
  `whitespace-nowrap` or `whitespace-pre` is never broken: a long word overflows the line.
- **Your own hyphenation.** `Font.registerHyphenationCallback` sets a function that splits each
  word into the parts a line may break between, for every `Text`; `[word]` keeps a word from being hyphenated.
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
- **Even lines.** `textWrap: 'pretty'` (the class `text-pretty`) breaks a paragraph's lines
  over the whole paragraph instead of filling each in turn, as TeX does: lines come out more
  even, with fewer hyphens, and the spaces of justified lines may also shrink by up to a third.
  It is inherited, so `<Page style={{ textWrap: 'pretty' }}>` sets it for a page. `'wrap'` is
  the default and fills lines in turn; `'balance'` is not supported yet. Text with
  `whitespace-nowrap` or `whitespace-pre`, and very long paragraphs past the limits in the
  [schema](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md), break first fit.
- **Orphans and widows:** `orphans={2}` and `widows={2}` keep at least two lines of a paragraph
  together at the foot and the head of a page.

## Text: superscripts and font features

```tsx
<Text>
  E = mc<Text style={{ verticalAlign: 'super', fontSize: 7 }}>2</Text>, H
  <Text className="align-sub text-[7pt]">2</Text>O
</Text>
<Text style={{ fontFeatureSettings: "'tnum' 1, 'ss01'" }}>1,111.10</Text>
<Text className="tabular-nums slashed-zero">10,000</Text>
```

- **Superscripts and subscripts.** `verticalAlign: 'super'` or `'sub'` on a `Text` (the classes
  `align-super` and `align-sub`) raises or lowers its baseline by the offsets the font gives for
  superscripts and subscripts, for text of its size; `'baseline'` puts it back. The size stays
  the one you set: give the superscript a smaller `fontSize`. The line grows to fit it, as in
  CSS. A `Text` inside a superscript stays on its baseline. On a `Cell`, `verticalAlign` places
  the content (`top`, `middle`, `bottom`), and `super` or `sub` there is an error.
- **OpenType features.** `fontFeatureSettings` takes a CSS string
  (`"'tnum' 1, 'liga' off"`), a list of tags to turn on (`['tnum', 'smcp']`), or an object of
  tags to values or switches (`{ tnum: 1, liga: false, salt: 2 }`): 1 turns a feature on, 0
  off, and a larger number picks an alternate. `fontVariantNumeric` (`'tabular-nums'`,
  `'oldstyle-nums slashed-zero'`, …) and its Tailwind classes turn on the matching features,
  and `fontFeatureSettings` (or `font-features-[…]`) wins over them. Both are inherited like the
  other text properties; a `Text` that sets its own `fontFeatureSettings` replaces the inherited
  one. A feature does something only when the font has it: Inter has `tnum`, `zero`, `ss01` to
  `ss08`, `frac`, `case` and more, but no small capitals; Noto Sans has `smcp`. A run takes at
  most 32 features, and a document at most 64 different sets of them.
- `PageNumber` and `TotalPages` draw their digits without the features.

## Drawing: SVG and Canvas

```tsx
<Svg width={120} height={60} viewBox="0 0 120 60">
  <Rect x={0} y={20} width={30} height={40} rx={3} fill="#c7d2fe" />
  <Rect x={45} y={0} width={30} height={60} rx={3} fill="#4f46e5" />
  <Path d="M0 58 L120 58" stroke="#9ca3af" strokeWidth={1} />
</Svg>

<Canvas width={120} height={60} paint={(p, w, h) => p.moveTo(0, h).lineTo(w, 0).lineWidth(2).stroke('#4f46e5')} />
```

The SVG primitives take SVG attributes in camelCase (`strokeWidth`, `stopColor`);
`visibility="hidden"` leaves a shape undrawn, as in SVG. `Canvas`
draws in code with a pdfkit-style painter. An SVG file can also go straight into
`<Image src={svgBytes} />`.

**Text in a drawing** is a `Text` inside the `<Svg>` (or a `G`, `Defs`, `ClipPath` or `Marker`
in it), with `Tspan`s for spans of it. It is real PDF text: it can be selected, searched and
copied.

```tsx
<Svg width={160} height={60}>
  <Rect width={160} height={60} rx={6} fill="#eef2ff" />
  <Text x={80} y={26} textAnchor="middle" fontWeight="bold" fill="#4f46e5">
    Revenue <Tspan fill="#16a34a">+12%</Tspan>
  </Text>
  <Text x={80} y={46} textAnchor="middle" fontSize={9} letterSpacing={1}>
    Q3 2026
  </Text>
</Svg>
```

- **Position**: `x` and `y`, with `textAnchor` (`start`, `middle`, `end`) and
  `dominantBaseline` (`alphabetic`, `middle`, `central`, `hanging`, `text-before-edge`, …).
  A list (`x={[10, 20, 30]}`) places each character, and `dx`, `dy` and `rotate` shift and
  turn them. `textLength` fits the text to a width, by its spacing or, with
  `lengthAdjust="spacingAndGlyphs"`, its glyphs too.
- **Font**: `fontFamily` (a stack: `"Brand, Inter"`), `fontWeight`, `fontStyle` and
  `fontSize`, on the text, a `Tspan` or a `G` around it; otherwise the text style around the
  drawing, as for any `Text`. The font is one registered with `Font.register` (or bundled),
  matched like other text, so a missing family falls back with a warning.
- **Style**: `fill` and `stroke` with colours, gradients and patterns, `letterSpacing`,
  `wordSpacing`, `textDecoration` (`underline`, `overline`, `line-through`), `direction`
  (`rtl` sets the base direction of the text and mirrors its anchor) and, on a `Tspan`,
  `baselineShift` (`super`, `sub` or a length) and `alignmentBaseline`, which places the span
  on another baseline than the text's `dominantBaseline`. `fontFeatureSettings`
  (`"'tnum', 'liga' off"`), `fontVariant` (`small-caps`, `tabular-nums`, …) and `fontKerning`
  (`none`) choose OpenType features, as in CSS. A `Text` in an `<Svg>` takes these SVG props:
  `style` and `className` are for text outside drawings.
- **Vertical text**: `writingMode="vertical-rl"` (or `vertical-lr`, the same for one line) sets
  the text down a column centred on its `x`: CJK characters stand upright, in their vertical
  forms when the font has them (`、` `「` `ー`), with its vertical metrics; Latin letters and
  digits lie sideways, as in browsers. `textOrientation="upright"` stands every character,
  `"sideways"` turns every one, and `writingMode="sideways-rl"` turns every one too
  (`sideways-lr` goes up). The anchor and `textLength` act down the column
  (`spacingAndGlyphs` stretches sideways glyphs, and spaces upright ones), and the underline
  goes on the left, the overline on the right.
- **Text on a path**: a `TextPath` directly inside the `Text` sets its characters along the
  path or shape `href` names (`href="#curve"`, in `Defs`), or along its own `path` data, each
  turned with the curve, from `startOffset` (a length or a percentage of the path's length).
  `side="right"` follows the path the other way; what goes past its end is not drawn.
  `method` and `spacing` are accepted and do nothing, as in browsers. Vertical text on a path
  is not supported.
- Strings, `Tspan` and `TextPath` go in it; anything else is an error.

```tsx
<Svg width={200} height={90}>
  <Defs>
    <Path id="curve" d="M10 80 Q 100 0 190 80" />
  </Defs>
  <Text fontSize={13} fill="#4f46e5">
    <TextPath href="#curve" startOffset="50%" textAnchor="middle">Along the curve</TextPath>
  </Text>
  <Text x={10} y={10} writingMode="vertical-rl">縦書き</Text>
</Svg>
```

**Markers** draw a shape at the vertices of a `Path`, `Line`, `Polyline` or `Polygon`: define a
`Marker` in `Defs` and reference it with `markerStart`, `markerMid` or `markerEnd`.

```tsx
<Svg width={120} height={40}>
  <Defs>
    <Marker id="arrow" viewBox="0 0 10 10" refX={10} refY={5} markerWidth={6} markerHeight={6} orient="auto-start-reverse">
      <Path d="M0 0L10 5L0 10Z" fill="context-stroke" />
    </Marker>
  </Defs>
  <Line x1={10} y1={20} x2={110} y2={20} stroke="#4f46e5" strokeWidth={2} markerStart="url(#arrow)" markerEnd="url(#arrow)" />
</Svg>
```

`refX` and `refY` are the point of the marker placed on the vertex, `markerWidth` and
`markerHeight` its viewport (3 by default), and `orient` turns it with the path (`'auto'`), the
same but reversed at the start (`'auto-start-reverse'`), or by a number of degrees. A marker is
scaled by the stroke width unless `markerUnits="userSpaceOnUse"`. `context-stroke` and
`context-fill` paint it with the shape's stroke or fill. A `RadialGradient` takes `fr`, the
radius of its focal circle, next to `fx` and `fy`.

**The `Canvas` painter** builds a path (`moveTo`, `lineTo`, `bezierCurveTo`,
`quadraticCurveTo`, `rect`, `roundedRect`, `circle`, `ellipse`, `polygon`, `path`) and paints it
with `fill`, `stroke` or `fillAndStroke`, in the current state: `fillColor`, `strokeColor`,
opacities, `lineWidth`, `lineCap`, `lineJoin`, `miterLimit` (10 by default, as in PDF), `dash`,
and the transforms `translate`, `rotate` and `scale`. `save()` and `restore()` keep and bring
back the state and the transforms.

```tsx
<Canvas
  width={120}
  height={60}
  paint={(p, w, h) => {
    const sky = p.linearGradient(0, 0, 0, h).stop(0, '#4f46e5').stop(1, '#c7d2fe');
    p.save().roundedRect(0, 0, w, h, 8).clip();
    p.rect(0, 0, w, h).fill(sky);
    p.restore().circle(w / 2, h / 2, 12).fill(p.radialGradient(56, 26, 0, 60, 30, 12).stop(0, 'white').stop(1, '#f59e0b'));
  }}
/>
```

- **`clip(rule?)`** clips what is painted after it to the current path, until `restore()`;
  `'evenodd'` picks the even-odd rule. Clips nest.
- **`linearGradient(x1, y1, x2, y2)`** and **`radialGradient(x1, y1, r1, x2, y2, r2)`** (from
  the first circle to the second) return a `CanvasGradient`; `stop(offset, color, opacity?)`
  adds its colours. Pass it to `fill`, `stroke`, `fillAndStroke`, `fillColor` or `strokeColor`
  in place of a colour. Its coordinates are in the user space where it paints, transforms
  included, as in pdfkit.
- **`text(str, x, y, options?)`** draws one line of text with the fill colour or gradient, its
  top at `y` (as in pdfkit). **`font(name, size?)`** sets the font: a registered font id
  (`'Inter-Bold'`), a family name, or a pdfkit standard name (`'Helvetica-Bold'`), which picks
  the canvas's family in that weight and style; **`fontSize(n)`** sets the size (12 by
  default). Until `font()` the text is in the font around the canvas. The options
  (`CanvasTextOptions`) are `width` with `align` (`left`, `center`, `right`; `justify`, as in
  pdfkit, is `left`, since a line does not wrap), `baseline`
  (`top`, `middle`, `bottom`, `alphabetic`, `hanging`), `characterSpacing`, `wordSpacing`,
  `underline`, `strike`, `fill` (default `true`) and `stroke` (with the stroke colour and line
  style). Text does not wrap: a line break in `str` is an error.

  ```tsx
  <Canvas width={160} height={40} paint={(p, w) => {
    p.font('Inter-Bold', 14).fillColor('#4f46e5').text('Total', 0, 4);
    p.font('Inter').text('$1,280.00', 0, 4, { width: w, align: 'right' });
  }} />
  ```

In an SVG, whether in `<Svg>` or an image file:

- **Text** (`<text>`, `<tspan>`, `<a>`) is drawn as PDF text with the document's fonts, in
  nested SVG images too. Each span gets the declared font that matches its `font-family`,
  `font-weight` and `font-style` as CSS matches them (the first declared font when no family
  matches), with the fallback fonts for characters it lacks; a font that only an SVG file
  needs goes in `fallbackFonts` ([metadata](#links-bookmarks-and-metadata)), so that the
  document declares it. Style sheets, `xml:space`, positions and rotations, anchors, baselines,
  `baseline-shift`, spacing, `text-decoration`, `textLength`, `font-variant`, `font-kerning`
  and `font-feature-settings` work, also in clip paths, masks, patterns, markers and `use`
  copies. Vertical text (`writing-mode` of `tb`, `tb-rl`, `vertical-rl`, `vertical-lr`,
  `sideways-rl` or `sideways-lr`, with `text-orientation` and `glyph-orientation-vertical`) and
  text on a path (`<textPath>` on a `path` or basic shape, or with its own `path` data) are
  laid out as described above, and read in their order when the PDF's text is copied or
  extracted. Vertical text on a path fails the render with `INVALID_IMAGE`: convert it to
  outlines in your design tool, as for the text paths of an SVG that has more than 100,000
  path segments in all. An SVG's text uses at most 64 different sets of font features, and so do the SVGs of a
  document in all.
  `font-stretch` is ignored, and font properties set on a `<use>` do not reach the text it
  copies (its fill and stroke do). A `<text>` element that an XML entity expands to fails with `INVALID_IMAGE` too:
  write the element in the SVG itself. An entity inside a `<text>` that is in the SVG
  (`<text>&co; report</text>`) is fine. An `!important` rule that names both `path` and
  `polygon` also sets the paint of the SVG's text. Colour emoji fonts draw in colour, and emoji the document maps to images (`emojiImages`) are those
  images, as in other text; an emoji image that is itself an SVG with text is left out there,
  but its text still copies. An SVG with text in a document that declares no fonts fails
  validation with `INVALID_IMAGE`, and an SVG holds at most 1,000,000 characters of text,
  counting every copy. Its text counts toward the other limits of the SVG too, as it is drawn,
  so an SVG that fails one fails validation, not only the render. An SVG without `width`,
  `height` and `viewBox` takes its size from what it draws, its text left out, so the same
  with any fonts. Where nothing sets a font size it is 16, as in browsers.
- **Filters** (`filter`: drop shadows, blurs) are not applied: the element is drawn without them.
- **Images** inside the SVG must be embedded as `data:` URLs (JPEG, PNG, GIF, WebP or SVG).
  Links to files or URLs are left out, and an embedded image over 50 megapixels fails the
  render.
- **Size**: the width and height are 0.00001 to 100,000 px each, and the `viewBox` width and
  height at least 0.00001. Outside these the SVG fails the render with `INVALID_IMAGE`; a nested
  SVG image is left out.
- **Huge numbers** elsewhere in the SVG (`transform="scale(1e30)"`, a gradient or a shape at
  1e30) are written as at most ±10¹¹ points, the largest number the PDF writer emits anywhere,
  so PDF readers can parse every number. What is drawn that far out is off the page either way.

## Forms

```tsx
<TextField name="email" className="h-6 w-full rounded border border-gray-300 px-1" />
<Select name="plan" options={['Starter', 'Team']} value="Team" className="h-6 w-40 border" />
<List name="days" options={['Mon', 'Tue', 'Wed']} multiSelect value={['Mon', 'Wed']} className="w-40 border" />
<Checkbox name="terms" className="h-3.5 w-3.5 border" />
<Radio name="billing" value="yearly" checked className="h-3.5 w-3.5 rounded-full border" />
```

These are real AcroForm fields: readers fill them in any PDF viewer and save the file. `name`
must be unique, except for radio buttons of one group. Every field takes `readOnly` (viewers do
not let the user change it) and `required` (it must be filled in before the form is
submitted). `TextField` also takes `multiline`, `maxLength` and `password`; size, border,
background and text style come from the classes. `TextInput` is another name for `TextField`.

A field's text, its value and the options of a `Select` or `List`, is drawn with the field's own
font only, a glyph per character, as viewers redraw it while the user types: no fallback fonts,
no shaping and no right-to-left ordering. A character the font lacks fails the render
(`INVALID_VALUE` at the value or the option), so give the field a font that covers its script.
Arabic, Hebrew and the scripts of India show their letters unjoined, in typed order.

`Select` is a drop-down list and `List` a list box, which shows several options at once (four
rows without a height) and starts at the first selected one. Both take their choices as
`options`, or `select`, and the selected one as `value`; a `List` with `multiSelect` takes an
array of them. `Field` is any of these, picked by its `kind` (`"text"`, `"checkbox"`,
`"radio"`, `"select"`, `"list"`), for forms built from data:
`<Field kind={q.kind} name={q.name} … />`.

A `FieldSet` groups fields: it is a `View`, with the same props (`render` too), whose `name`
goes in front of the name of every field inside it, at any depth. Viewers show and export the
fields by those full names, and sets nest:

```tsx
<FieldSet name="billing" className="gap-2">
  <TextField name="street" />                 {/* billing.street */}
  <FieldSet name="contact">
    <TextField name="email" />                {/* billing.contact.email */}
  </FieldSet>
</FieldSet>
```

A dot in a field's own `name` does the same (`name="billing.street"`). A name is either a field
or a group: `billing` cannot also be a field when `billing.street` is one.

A single-line `TextField` takes a `format`, which it shows its value in and which viewers apply
as the user types:

```tsx
<TextField name="fee" value="1250" format={{ type: 'number', currency: '$' }} />          {/* $1,250.00 */}
<TextField name="rate" value="0.125" format={{ type: 'percent', decimals: 1 }} />         {/* 12.5% */}
<TextField name="start" value="2026-10-14" format={{ type: 'date', pattern: 'd mmm yyyy' }} /> {/* 14 Oct 2026 */}
<TextField name="phone" value="5551234567" format={{ type: 'phone' }} />                   {/* (555) 123-4567 */}
```

| `type` | `value` | Options |
|---|---|---|
| `number` | digits, as `'-1234.5'` | `decimals` (default 2, rounded half away from zero), `separators` (`'1,234.56'`, `'1234.56'`, `'1.234,56'`, `'1234,56'`), `negative` (`'minus'`, `'red'`, `'parens'`, `'parensRed'`), `currency` and `currencyBefore` (default `true`) |
| `percent` | a fraction, as `'0.125'` | `decimals`, `separators` |
| `date` | `'2026-10-14'`, with an optional time (`T18:30`) | `pattern` (required) |
| `time` | `'18:30'` or `'18:30:05'`, or a date with a time | `pattern` (required) |
| `zip`, `zipPlus4`, `phone`, `ssn` | 5, 9, 10 or 9 digits | none |

A `pattern` is at most 64 characters of `d`, `dd`, `ddd` (Wed), `dddd` (Wednesday), `m`, `mm`,
`mmm` (Oct), `mmmm` (October), `yy`, `yyyy`, `H`, `HH` (0–23), `h`, `hh` (1–12), `M`, `MM`
(minutes), `s`, `ss`, `t` (a/p), `tt` (am/pm) and the characters ` .,/:-`; names are English.
A `time` pattern takes only the time letters, `H`, `h`, `M`, `s` and `t`. Viewers apply the
format with the standard form scripts, which some (Preview) do not run; a PDF/A document leaves
the scripts out and keeps the formatted text.

The keys `nDec`, `sepComma` (`true` is `'1,234.56'`, `false` `'1234.56'`), `negStyle`
(`'MinusBlack'`, `'Red'`, `'ParensBlack'`, `'ParensRed'`), `currencyPrepend` and `param` are
other names for `decimals`, `separators`, `negative`, `currencyBefore` and `pattern`, and
`zip4` for `zipPlus4`. A number or percent format with any of them defaults to no decimals and
no thousands separator, so `{ type: 'number', currencyPrepend: true }` shows `'1250'` as
`1250`; without them, to 2 decimals and `'1,234.56'`. On a `time`, `param` is a style: `0` (`HH:MM`), `1` (`h:MM tt`), `2`
(`HH:MM:ss`) or `3` (`h:MM:ss tt`).

## Notes

```tsx
<Note author="Ana">Check the totals against the ledger.</Note>
<Note className="absolute right-0 top-0 bg-blue-500" open>Draft: not for circulation.</Note>
<Text>Total: 1,250<Note author="Ana">Includes the March refund.</Note> due on 1 May.</Text>
```

A `Note` is a text annotation: a sticky-note icon that viewers open to show its text, which is
its children (strings and numbers only). `author` is shown as its title, `open` shows the text
when the document opens, and its `backgroundColor` (or a `bg-*` class) is its colour, yellow by
default. It takes no space: the icon, 20 pt square at any zoom, sits at the top-left corner of
its box, so a note placed before a paragraph in a row marks it, and `position: 'absolute'` with
insets puts it anywhere. It is on the page where its box starts; in a `Header`, a `Footer` or a
`fixed` element it is on every page. Under a `transform`, the icon stays upright and the same
size: only its corner moves. The icon is drawn into the PDF, so it prints and looks the same in
every viewer, and the output stays PDF/A-2b.

Inside a `Text`, a note is part of the line: it takes no room, and its icon hangs from the top of
the line where it is, on the page of that line. Line breaking keeps it with the word before it,
or after a space, with the word after it. There it takes `backgroundColor` and no other style.

## Accessibility

```tsx
<Document title="Annual report" lang="en" pdfUA>
  <Page>
    <Text role="h1">Annual report</Text>
    <Text>Sales grew in every region.</Text>
    <Image src={chart} alt="Revenue by month, rising from 12k to 30k" />
    <Image src={flourish} alt="" />
    <View role="list">
      <View role="listItem" className="flex-row">
        <Text role="label">1.</Text>
        <View role="listBody"><Text>North: +12%</Text></View>
      </View>
    </View>
    <Table columns={['2fr', '1fr']} headerRows={1}>
      <Row><Cell>Region</Cell><Cell>Growth</Cell></Row>
      <Row><Cell header>North</Cell><Cell>12%</Cell></Row>
    </Table>
    <TextField name="email" label="Your email address" />
  </Page>
</Document>
```

`tagged` writes **tagged PDF**: a structure tree that tells screen readers, reflowing viewers
and text extraction what each part of the page is, in reading order. `pdfUA` writes
**PDF/UA-1** (ISO 14289-1), the standard for accessible PDF, which is tagged and also needs a
`title`, a `lang`, alternative text on every image that is not decoration, a `label` on every
field (in a `Header` or `fixed` too), headings in order and lists made of items: without one, the
render fails with `ENGINE_REJECTED` (`engineCode` `UA_CONFLICT`) and the component that lacks it. `conformance="PDF/A-2a"` or `"PDF/A-3a"` is PDF/A, tagged. The render
options `tagged` and `pdfUA` do the same and win over the props.

- **What is what.** A `Text` is a paragraph, an `Image`, `Svg` or `Canvas` a figure with its
  `alt`, a `Table` a table (its header rows head their columns; `<Cell header>` heads its row;
  spans are kept), a field a form field with its `label`, a `Note` or an `Attachment` an
  annotation, and a link a link, which screen readers announce by its text. An image with `alt`
  inside a `Text` is a figure in place, and a link inside one a link. A `View` is not an element: its children belong to its parent.
  `role` says otherwise: `'h1'` to `'h6'` for headings (PDF/UA wants them to go down one level at
  a time from `h1`), `'section'`, `'blockQuote'`, `'caption'`, `'figure'` (a `View` drawn as one
  figure, with `alt`), `'list'`, `'listItem'`, `'label'` and `'listBody'` for lists, or
  `'artifact'` for decoration that screen readers skip. A `'listItem'` belongs in a `'list'` and a
  `'listBody'` in a `'listItem'`; the list's numbering is read from its labels (`1.`, `a)`, `•`).
- **Decoration.** `alt=""` makes an image decoration. Backgrounds, borders, rules, `Header`s,
  `Footer`s and `fixed` elements (painted on every page), and repeated table header rows are
  decoration too.
- **Reading order** is the order of the template, whatever `zIndex`, floats, absolute positions
  or grid areas do on the page: write the template in the order it should be read.
- **What checkers cannot tell.** That alternative text says what an image shows, that the
  template's order is the reading order, and that colours have enough contrast are yours to
  check. Test with a screen reader, and with [PAC](https://pac.pdf-accessibility.org/) or
  veraPDF.

A tagged file is larger: about 2.5 times for a long report of tables, much less for documents
of paragraphs.

## Attachments

```tsx
<Document
  conformance="PDF/A-3b"
  attachments={[
    {
      name: 'factur-x.xml',
      data: invoiceXml,
      mimeType: 'text/xml',
      relationship: facturXRelationship('EN 16931'),
      description: 'Invoice data',
    },
  ]}
  xmpExtensions={[facturX({ profile: 'EN 16931' })]}
>
  <Page>
    <Attachment file="factur-x.xml" icon="paperclip">The invoice as XML</Attachment>
    …
  </Page>
</Document>
```

`attachments` are files the PDF carries: viewers list them in their attachments panel
(`pageMode="useAttachments"` opens it). Each has a `name`, unique in the document, and its
`data`, a `Uint8Array` or a string (its text, in UTF-8); `mimeType`, `description`,
`relationship` (`'source'`, `'data'`, `'alternative'`, `'supplement'`) and `modified` (RFC 3339)
are optional. A file is written once, compressed when that makes it smaller, and encrypted with
the rest of an encrypted PDF. The bytes go into the document tree as base64 `data`, so a stored
`result.document` renders alone, its files with it, at 4/3 of their size; a document written as
JSON names large files by `src` instead, with their bytes in the render option `attachments`.
At most 1000 files, 64 MiB each and 256 MiB together. Inkoxide
attaches the bytes it is given, whatever they are: a viewer may open an attached program or web
page when the reader asks it to, so attach only files you would send on their own.

`<Attachment file>` is an icon that opens one: a push pin, a paper clip (`icon="paperclip"`), a
bar chart (`graph`) or a tag. It sits like a [`Note`](#notes): it takes no room, its icon is at
the top-left corner of its box, inside a `Text` it hangs from the line, and in a `Header` or a
`Footer` it is on every page, each icon opening the one file. Its children are the text viewers
show for it (default: the file's description, else its name), and `backgroundColor` is its
colour.

Under PDF/A, only PDF/A-3b allows attachments; PDF/A-1b and 2b refuse them (`PDFA_CONFLICT`).
**E-invoices** (Factur-X, ZUGFeRD 2) are PDF/A-3b with the invoice's XML attached and the `fx`
metadata that `facturX({ profile })` makes, as above; `facturXRelationship(profile)` gives the
relationship the profile asks for (`'data'` for MINIMUM and BASIC WL, `'alternative'` for the
others). `xmpExtensions` takes other schemas too (`{ schema, namespace, prefix, properties }`,
Text values), and needs PDF/A: the metadata they go in. Inkoxide does not check the XML: validate
it, and the PDF, with your e-invoicing tools.

A document written as JSON names its files in `attachments` with `data` (base64) or `src`, an id
of the render option `attachments`:
`renderDocument(doc, images, { attachments: { 'invoice-xml': bytes } })`.

## Debugging layout

```tsx
<View debug className="m-4 border-2 p-3">…</View>   // one element
<Page debug>…</Page>                                // the page and everything on it
<Document debug>…</Document>                        // every page
```

`debug` draws an element's boxes over the page, like the box model in browser developer tools:
the margin in orange, the border in yellow, the padding in green, the content box as a blue
outline, and the element's width and height in points as a small label. It works on `View`,
`ImageBackground`, `Text`, `Image`, `Svg`, `Canvas`, `Table`, `Row`, `Cell`, `Spacer`, the fields, `Header` and
`Footer`; on a `Page` it also outlines the page margins. A box split across pages is outlined
on each page it crosses, and a header or footer on every page.

On a `Text`, it also outlines the paragraph line by line: each line box as a dashed outline, its
baseline, and the glyphs of each run on the line, from the font's ascender to its descender,
filled in purple and teal by turns. A run that wraps has a box on each line it reaches, so a
`Text` or `Link` inside a `Text` shows where its words landed. Those inner elements are runs of
the paragraph's lines, not boxes, so `debug` on one of them draws nothing: put it on the outer
`Text`.

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
| `remoteImages` | off | `{ allow: ['https://cdn.example.com/', '*.example.org'] }`: hosts `<Image src="https://…">` and `<Image src={{ uri }}>` may load from. A URL entry allows its origin (scheme, host and port), and, with a path, that path and the paths under it: `https://cdn.example.com/img` allows `/img/logo.png`, not `/imgs/logo.png`. A host entry allows `https:` at that host, or with `*.` at its subdomains. Ports must match: an entry without a port allows only its scheme's default port (443 for `https:`, 80 for `http:`), and `cdn.example.com:8443` or `https://cdn.example.com:8443` only port 8443. `timeoutMs` (10 000; `Infinity` for no limit) and `maxBytes` (20 MiB, 20 971 520 bytes) limit each image, and redirects are not followed. The `INKOXIDE_REMOTE_IMAGES` environment variable (comma-separated) sets it for every render. |
| `imageTimeoutMs` | `10000` | How long a function or promise `<Image src>` may take to settle; `Infinity` waits without a limit |
| `pdfA` | `false` | Write PDF/A, for archiving: `true` or `'2b'`, `'3b'` or `'1b'` (see [PDF/A](#pdfa)). Wins over `<Document conformance>`. |
| `encryption` | none | `{ userPassword, ownerPassword, permissions }`: encrypt the PDF (see [Passwords and permissions](#passwords-and-permissions)). Replaces the `<Document>` props of the same names. |
| `compress` | `true` | Compress the page content |
| `subsetFonts` | `true` | Embed only the glyphs the document uses |
| `creationDate` | none | RFC 3339 timestamp written to the PDF. Without it, the same input gives byte-identical output. |
| `threads` | all cores | How many cores one render spreads its pages over: an integer from 1 to 256 |
| `validate` | outside production | Check the document tree first, for clearer errors |

A bad option fails the render with `ENGINE_REJECTED` at its path (`options.threads`,
`options.remoteImages.timeoutMs`): a `creationDate` that is not RFC 3339, a `threads` that is
not an integer from 1 to 256, or a timeout or `maxBytes` that is not a number from 0 (NaN, a
negative number, a string), or an option of the wrong type (`threads: '4'`, `localImages`
without `root`). A fractional timeout is rounded up to a whole millisecond.

**`Font.register({ family, src })`** adds a font family, from a path, a URL or bytes. See
[Fonts](#fonts).

**`Font.registerHyphenationCallback(fn)`** splits words for hyphenation with your own function;
`null` removes it. See [Text: hyphenation and direction](#text-hyphenation-and-direction).

**`Font.registerEmojiSource(source)`** draws emoji from images: `{ url, format? }` or
`{ builder }`; `null` removes it. See [Fonts](#fonts).

**`StyleSheet.create(styles)`** returns the styles unchanged, with their types.

**`init()`** loads Tailwind's compiler (when `@inkoxide/tailwind` is installed), the engine and
the fonts. The first render does it anyway; call it at server startup so the first request does
not wait. It resolves to the same status as `engineStatus()`.

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
`TextProps`, `TextFieldProps`, `FieldProps`, `SvgProps`, `MarkerProps`, `TspanProps`, `TextPathProps`, `CanvasProps` and so on); `PolyProps`
serves `Polyline` and `Polygon`, `GradientProps` both gradients, `SvgPresentation` the
attributes every SVG shape takes, and `SvgTextProps` the props of a `Text` in an `Svg`. Also
exported:

- styles and values: `Style`, `StyleProp`, `MediaStyle` (a `StyleProp` with `@media` blocks),
  `AbsoluteLength`, `Length`, `LengthPercent`,
  `LengthAuto`, `PageSize`, `PagePreset`, `BreakProp`, `BookmarkProp`, `FontWeightName`,
  `Permissions` (of `<Document permissions>`), and `TextFormat`, the `format` of a `TextField`;
- `render` props and images: `RenderProp`, `PageInfo`, `ImageSrc`, `ImageSrcValue`,
  `ImageRequest`, `Painter`, the painter a `Canvas` draws with, `CanvasGradient`, its
  gradients, and `CanvasTextOptions`, the options of its `text`;
- options and results: `RenderToBufferOptions`, `LocalImageOptions`, `RemoteImageOptions`,
  `FontRegistration`, `FontSource`, `FontSrc`, `EmojiSource`, `FontWeight`, `HyphenationCallback`,
  `EngineStatus`, `EngineVersion`, `RenderErrorCode` and `DataIssue`.

**Concurrency.** A render runs on Node's thread pool, so many can run at once. `threads` sets
how many cores one render uses; with `threads: 1` each render stays on its own pool thread.
Node's `UV_THREADPOOL_SIZE` (default 4) caps how many renders run at the same time.

## Errors

Mistakes fail loudly, with the component that caused them: a style key or value an element
cannot use, and a prop it would ignore (`<Page wrap={false}>`, `href` on a `Cell` or a
`Header`, `className` on an SVG shape, `render` on an `Image`). An error inside a `render` prop
names the components around it too. A `View` inside a `Text`:

```tsx
<Text>Total: <View /></Text>
```

stops the render with:

```
RenderError: <View> cannot be inside <Text>. A Text contains only strings, numbers,
<Text>, <Link>, <Image>, <Note>, <PageNumber> and <TotalPages>.
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
| `TEMPLATE_ERROR` | A component used in the wrong place, a prop with an invalid value (a bad `Page` `size`, a `Link` without `href`), an unknown style key, an unknown Tailwind class (outside production; there it is logged once and ignored), a component or `render` prop that throws, or an image `src` that cannot be used: a URL outside `remoteImages`, a path while `localImages` is off (from a function), or a function or promise that fails or does not settle in `imageTimeoutMs` |
| `INVALID_TREE` | The document tree does not match the engine's schema. Checked only with `validate`, which is on outside production |
| `INVALID_FONT` | A registered font could not be read, fetched or parsed |
| `ENGINE_REJECTED` | The engine refused the document, for example an unreadable image, or a render option is out of range (`path` is `options.threads`, `options.creationDate`, …) |
| `IMAGE_FETCH_FAILED` | An allowed remote image could not be fetched: an HTTP error, a redirect, a timeout, a dropped connection or over `maxBytes` |
| `IMAGE_READ_FAILED` | An image file could not be read: missing or outside the `localImages` folders, a network path, not a valid `file:` URL, not a regular file, over the size limit, permission denied, or a `localImages` folder that cannot be opened |
| `INTERNAL` | The engine could not start (no fonts in the assets folder, `inkoxide` and `@inkoxide/schema` versions that do not match), a bundled font or hyphenation file could not be read or, in a browser, fetched (`engineCode` `NOT_LOADED`), `renderToFile` could not write the file, or a bug |

`INTERNAL` has status 500, `IMAGE_FETCH_FAILED` 502 and the others 400. Besides `code`,
`status`, `message`, `path`, `component` and `componentStack`, a `RenderError` may have
`className` (the Tailwind class that failed), `engineCode` (the engine's own code, for
`ENGINE_REJECTED` and some `INTERNAL` errors) and `issues` (for `INVALID_DATA`). It turns into
JSON with only its fields that are set (`res.status(e.status).json(e)`): `code`, `message`,
`path`, `component`, `componentStack`, `className`, `engineCode` and `issues`, but not `status`
or the stack. Two more codes come from [templates](#serving-templates): `INVALID_DATA` (400) and
`UNKNOWN_TEMPLATE` (404).

`Font.register`, `Font.registerHyphenationCallback` and `Font.registerEmojiSource` check their
arguments when you call them and throw a plain `Error` straight away: a missing family name or
`src`, a `src` that is not a path, a URL or bytes, an unknown weight, a weight range that is not
two weights from the lower one, a malformed `fontVariationSettings`, a face already registered
from another source, a face the bundled fonts have, a family whose font id another family has, a
callback that is not a function, or an emoji source with neither a `url` string nor a `builder`
function.

## Serving templates

An advanced API, for a server that renders templates by id from JSON data, such as
`POST /pdf/invoice`. It lives in its own entry, `@inkoxide/render/templates`, and checks the
data with a [zod](https://zod.dev) 4 schema before React renders anything. Install zod to write
the schemas:

```sh
npm install zod
```

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
  `engineMs`, `fetchMs` and `totalMs`. `warnings` are the document's font fallbacks, `hyphenate` with no
  language, and `render` props that did not settle; start-up warnings are in `engineStatus()`. `document` is the tree that was rendered,
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

## In the browser

The same templates render in a browser, with the engine as WebAssembly in a Web Worker, so the
page keeps responding while it renders. Install the module next to the package, and import
from `@inkoxide/render/browser`:

```sh
npm install @inkoxide/render @inkoxide/wasm
```

```tsx
import { PdfDownloadLink, PdfViewer, usePdf } from '@inkoxide/render/browser';

function InvoicePage({ invoice }: { invoice: InvoiceData }) {
  const doc = <Invoice invoice={invoice} />;
  return (
    <>
      <PdfDownloadLink document={doc} fileName="invoice.pdf">
        {({ loading }) => (loading ? 'Rendering…' : 'Download')}
      </PdfDownloadLink>
      <PdfViewer document={doc} style={{ width: '100%', height: 800 }} />
    </>
  );
}
```

- **`usePdf(element?, options?)`** renders when the component mounts and returns `{ blob, url,
  loading, error, update }` (`UsePdfResult`); `update(element?)` renders again. Without an
  element nothing renders until `update(element)`. A render that starts stops the one still
  running, and the object URL is revoked when a new one replaces it and on unmount. `options`
  are those of `renderToBuffer` below, read when a render starts.
- **`<PdfDownloadLink>`** is a link that downloads the PDF: `document`, `fileName` (default
  `'document.pdf'`), `options` and the props of an `<a>`; its children may be a function of the
  `usePdf` result. **`<PdfViewer>`** is an `<iframe>` with the browser's own viewer: `document`,
  `options`, `fallback` (shown until the first render has finished, and when it failed),
  `title` (default `'PDF'`) and the props of an `<iframe>`. Each renders when it mounts: give it
  another `key` to render again. Their props are `PdfDownloadLinkProps` and `PdfViewerProps`.
- **`renderToBuffer`** and **`renderToBlob`** render outside React components, and **`generate`**
  renders a [template](#serving-templates). Their options (`BrowserRenderOptions`, and
  `GenerateOptions` for `generate`) are those of the Node entry, with a `signal` or `timeoutMs`
  (a time in ms) that stops a render: its worker ends, and the next render starts a new one. A
  render that times out rejects with a `DOMException` named `TimeoutError`.
- **`init(options?)`** starts the engine, and every render calls it; only the options of the
  first call count. The options (`BrowserOptions`): `wasm` is where the module comes from (a
  URL, a `Response`, its bytes or a compiled `WebAssembly.Module`), or `{ single, threads }`, each
  of the two modules; `assets` is a folder to serve the bundled fonts and hyphenation patterns
  from: `fonts/` and `hyphenation/` from `@inkoxide/render`'s `assets/`, with the `.bin` files of
  `@inkoxide/wasm`'s `hyphenation/` in `hyphenation/` too; `threads` is how many threads renders
  use ([below](#threads-in-the-browser)); `worker: false` renders on the calling thread.
- **`clearCaches()`** empties the engine's caches of decoded images, font subsets and SVG text.
  The memory stays with the engine, which reuses it for the next renders; the PDFs are the
  same.

**Setting up.** Bundlers copy the module, the worker and the bundled fonts, which the packages
refer to with `new URL(…, import.meta.url)`. With Vite, set `worker: { format: 'es' }` for the
engine's module worker. For the dev server, also add
`optimizeDeps: { exclude: ['@inkoxide/wasm'] }`: pre-bundling moves the module away from its
`hyphenation/` folder, and a document that hyphenates a language other than English then fails
with `INTERNAL` (`engineCode` `NOT_LOADED`). [examples/browser](https://github.com/eddiesr93/inkoxide/tree/main/examples/browser)
is a Vite page; its `vite.config.ts` sets `worker.format`, the policy below, and an alias that
points `@inkoxide/render` at `@inkoxide/render/browser`, so that components written for Node
import unchanged. To use the engine without React, with document trees as JSON, see
[`@inkoxide/wasm`](https://github.com/eddiesr93/inkoxide/blob/main/packages/wasm/README.md).

**Fonts and images.** A bundled font is downloaded the first time a document uses it, as a
TrueType file, for the same PDF as on the server. `Font.register` takes a URL (relative to the
page) or the bytes; a file path (a `file:` URL, a drive letter or a backslash) fails the render
with `INVALID_FONT`. A WOFF2 font gives other PDF bytes than its TrueType file;
WOFF gives the same. Images come as bytes in `images`, or from the hosts allowed in
`remoteImages`: the requests carry no cookies, need CORS, and a redirect is refused.
`localImages` is not available.

**Content Security Policy.** The engine needs `script-src 'wasm-unsafe-eval'` and a worker of
the page's origin (`worker-src 'self'`). `<PdfViewer>` shows the PDF from an object URL
(`blob:`), as does a frame or an image given `usePdf().url`: allow it with `frame-src blob:`, and
`img-src blob:` for an image. The policy of examples/browser allows both. Nothing needs `'unsafe-eval'`, and the page needs no
cross-origin isolation (COOP and COEP), unless it wants threads.

### Threads in the browser

On a page that is cross-origin isolated, the engine renders on a pool of threads, as on the
server: shaping, painting and compression run on several cores. Serve the page with

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

on every response, a `304 Not Modified` included: WebKit refuses to start the engine's worker
again from its cache when a 304 lacks them. `threads` (default `'auto'`) is then the number of
cores, at most 8; `1` keeps one thread, and a number asks for that many (up to 256). The engine
downloads its module with threads instead of the other, starts that many more workers, which
share its memory, and `engineStatus().threads` tells how many it got. Without isolation, with
`worker: false`, or where the browser cannot reserve the module's memory (at most 2 GiB), it
renders on one thread. The PDFs are the same bytes either way.

A long document gains most: the engine alone renders a 98-page report about 2.2 times as fast on
4 threads and 2.9 times on 8 (measured in Node). A React document gains less, since its tree is
built in JavaScript, on one thread: 1.3 times for the 43-page report of the benchmarks, on 4
threads as on 8; a one-page invoice gains nothing. Give renders a `timeoutMs` with threads: a
bug on a pool thread that cannot reach the page leaves its render waiting. Isolation
also blocks resources from other origins that do not allow it (CORS, or a
`Cross-Origin-Resource-Policy` header), such as images, fonts and embeds. A bug in the engine on
one of its threads ends the worker, as `signal` does, and the render fails with `INTERNAL`.
Where the pool's workers cannot start (a `worker-src` policy, or a bundler that does not copy the
engine's nested worker), the engine renders on one thread.

**What differs from Node.**

- Renders run in one worker, one at a time, on one thread unless the page is cross-origin
  isolated ([threads](#threads-in-the-browser)); without a pool `threads` may only be `1`.
- `renderToFile`, `renderToStream` and image files are not available.
- The engine module is about 1.4 MB to download with brotli, once, and Tailwind's compiler
  about 90 KB when templates use `className`, or a few KB with the [build plugin](#tailwind).
  A bundled font (65 to
  245 KB with brotli) is downloaded when a document first draws with it: Noto Sans, the
  fallback, only for a character Inter lacks. A language's hyphenation patterns (1 to 350 KB)
  are downloaded when a document first hyphenates text in it.
- The PDF has the bytes Node renders, except where SVG markers are rotated and where SVG text
  follows a path: their numbers can differ in the last digits, since the browser's math library
  rounds differently.
- A bug in the engine fails the render with `INTERNAL`; the next render runs in a new instance.

## FAQ

**How is this different from HTML to PDF with Puppeteer or Playwright?**
Those print a web page with a headless Chrome, so any CSS works, but you ship and run a browser,
and each PDF is a browser round trip. Inkoxide has no browser: a native library that lays out
documents in milliseconds, with print features built in, such as repeating table headers, page
numbers and PDF/A.

**Does it run in the browser?**
Yes: import from `@inkoxide/render/browser`, and install `@inkoxide/wasm`, the engine as
WebAssembly. See [In the browser](#in-the-browser).

**The install worked, but the engine does not load.**
The engine ships as one package per platform
([Platforms](https://github.com/eddiesr93/inkoxide#platforms)). npm installs the right one as an
optional dependency, so it
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
Only to style with `className`: install `@inkoxide/tailwind` ([Tailwind](#tailwind)). It brings
the Tailwind compiler, so there is no config and no CSS build. Without it, style with `style`.

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

- **Browsers.** Renders run in a worker, on one thread unless the page is cross-origin isolated
  (see [In the browser](#in-the-browser)).
  Browsers and Node are the runtimes supported; edge runtimes are not.
- **Drawing.** Vertical SVG text on a path is not supported. SVG filters are not applied, and an SVG only shows images embedded as `data:` URLs (see
  [Drawing](#drawing-svg-and-canvas)).
- **Content taller than a page.** What cannot split (an `Image`, a float, an absolutely
  positioned, rotated or transformed element, table rows tied by a `rowSpan`) is cut at the
  page edge when it is taller than a page.
- **Tailwind.** Variants for screens and states (`hover:`, `dark:`), container queries, shadows
  and gradients are not supported.
- **Output.** The engine produces the whole PDF at once: `renderToStream` streams the finished
  file, it does not send pages while later ones are still being laid out.
