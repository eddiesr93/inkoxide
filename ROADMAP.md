# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Next: engine features

- **Encryption**: user and owner passwords, and permissions.
- **Text in `<Svg>`**: `<Text>` and `<Tspan>` inside `<Svg>`, and `<text>` in SVG files.
- **`<Canvas>`**: text in the painter (`text`, `font` and `fontSize`).
- **Inline images** inside `Text`.
- **Colour emoji from images** (`Font.registerEmojiSource`); emoji fonts already work.
- **Forms**: list boxes, field groups and number or date formats.
- **Text annotations** (`<Note>`).
- **Floats**: elements floated left or right, with text wrapping around them.
- **More PDF/A levels**: PDF/A-1b and PDF/A-3b.

## Later

- **Rendering in the browser**, through a WebAssembly build of the engine.
- **Accessibility**: tagged PDF and PDF/UA, with alternative text for images.
- **File attachments**.
- **Layout**: `@media` styles for the page size.
- **Variable fonts**, through a static instance chosen per weight.
