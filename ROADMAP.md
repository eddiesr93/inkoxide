# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Now

- **Text outlines in `debug`**: outline inline runs inside a `Text`, line by line.
- **Superscript and subscript** (`verticalAlign: 'super' | 'sub'`) and OpenType feature
  control (`fontFeatureSettings`).

## Next: engine features

- **Text in `<Svg>`**: `<Text>` and `<Tspan>` inside `<Svg>`, and `<text>` in SVG files.
- **`<Canvas>`**: text in the painter (`text`, `font` and `fontSize`).
- **Inline images** inside `Text`.
- **Colour emoji**, from an emoji font or from images.
- **Forms**: list boxes, field groups and number or date formats.
- **Text annotations** (`<Note>`).
- **Floats**: elements floated left or right, with text wrapping around them.
- **Line breaking**: optimal (Knuth–Plass style) paragraphs as an option next to first-fit.

## Later

- **Rendering in the browser**, through a WebAssembly build of the engine.
- **Accessibility**: tagged PDF and PDF/UA, with alternative text for images.
- **File attachments**.
- **Layout**: `@media` styles for the page size.
- **Variable fonts**, through a static instance chosen per weight.
