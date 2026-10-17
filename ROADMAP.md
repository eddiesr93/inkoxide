# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Later

- **In the browser**: threads, through a thread pool in Web Workers, for pages that are
  cross-origin isolated; `renderToStream`; reading local files.
- **A build-time Tailwind plugin**, so that a browser need not load the Tailwind compiler.
- **Accessibility**: tagged PDF and PDF/UA, with alternative text for images.
- **File attachments**.
- **Encryption**: an `entropy` option, for output that cannot be linked to its input; Unicode
  passwords, prepared with SASLprep (passwords are printable ASCII now); and an independent
  review of the key derivation by cryptographers. It has had an internal review only.
- **PDF/A-1b**: images with partial alpha flattened onto white, as an option, and binary alpha
  written as a stencil mask, which PDF/A-1b allows; both are refused now. A `validate` that knows
  the PDF/A level, so that it reports what only `render` reports now.
- **Layout**: `shapeOutside` on floats; text in a box beside a float that wraps around it line
  by line, as text directly beside it does (the box is narrowed over its whole height now);
  `flexGrow` and `justifyContent` in a box with floats; `textWrap: 'balance'`; a
  `hyphenationPenalty` on `Text`.
- **Variable fonts**: optical sizes chosen from the text size (`opsz`); `fontStretch`; weight
  ranges for SVG text, which takes the nearest instance a document declares.
- **SVG text**: text on a path (`<textPath>`) and vertical text; `font-variant`, `font-kerning`
  and `font-feature-settings`; font properties passed through `use`; `style` and `className` on
  the primitives inside `<Svg>`.
- **Colour emoji**: SVG-in-OpenType fonts; COLRv1 sweep gradients and blend modes (a sweep
  gradient draws with its first stop now, and every blend mode as source-over); colour glyphs as
  Type 3 fonts instead of images.
- **Forms**: `sort`, `edit`, `noSpell` and `align` on fields.
- **Notes**: an `icon` prop, for the other note icons.
