# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Now

- **`fixed`**: repeat any element on every page, besides `<Header>` and `<Footer>`.
- **`objectPosition`**: place a fitted image inside its box (today it is always centred).
- **Document props**: `producer`, `pageMode`, `pageLayout`.
- **`debug`**: outline boxes, padding and margins.
- **Page numbers in body text**: lay them out with their real digits, as headers and footers
  already do.

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
