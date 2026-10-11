# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Now: components, styles and sources

Most of this lives in the React layer.

- **`<Link>`**, as a component next to `href` on `Text` and `View`.
- **`fixed`**: repeat any element on every page, besides `<Header>` and `<Footer>`.
- **Page sizes**: the full ISO A, B and C series, RA and SRA, Folio and ID-1, `size={[w, h]}`
  and a bare number.
- **Image sources**: file paths, `{ uri, method, headers, body }`, and a function or promise
  that returns a source.
- **Style keys and values**:
  - `objectFit` and `objectPosition` as style keys (today only the `fit` prop, always centred).
  - `textOverflow: 'ellipsis'`; `textDecoration: 'line-through'` and `'underline line-through'`;
    `textDecorationColor` and `textDecorationStyle`.
  - `border`, `borderTop`… shorthand strings such as `'1px solid #e5e7eb'`.
  - The units `em`, `rem`, `vw`, `vh` and `pc` in style objects.
- **Document props**: `producer`, `pageMode`, `pageLayout`.
- **Fonts**: WOFF and WOFF2 files, and a custom hyphenation function.
- **Output**: `renderToStream`.
- **`debug`**: outline boxes, padding and margins.
- **Page numbers**: size `<PageNumber />` and `<TotalPages />` to the final digits instead of a
  fixed width.

## Next: engine features

- **Text in `<Svg>`**: `<Text>` and `<Tspan>` inside `<Svg>`, and `<text>` in SVG files.
- **Colour emoji**, from an emoji font or from images.
- **`<Canvas>`**: clipping, text, images and gradients in the painter.
- **Inline images** inside `Text`.
- **Text annotations** (`<Note>`).
- **Encryption**: user and owner passwords, and permissions.
- **Superscript and subscript** (`verticalAlign: 'super' | 'sub'`) and OpenType feature
  control (`fontFeatureSettings`).
- **Line breaking**: optimal (Knuth–Plass style) paragraphs as an option next to first-fit.
- **Forms**: list boxes, field groups and number or date formats.

## Later

- **Rendering in the browser**, through a WebAssembly build of the engine.
- **Accessibility**: tagged PDF and PDF/UA, with alternative text for images.
- **More PDF/A levels** (PDF/A-1b, PDF/A-3b) and file attachments.
- **Layout**: floats with text wrapping around them, and `@media` styles for the page size.
- **Variable fonts**, through a static instance chosen per weight.
