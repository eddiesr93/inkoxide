# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Later

- **In the browser**: `renderToStream`; reading local files; threads from a CDN, with the pool's
  worker served from another origin; a module with threads on stable Rust, once its standard
  library can be rebuilt there.
- **The Tailwind build plugin** for webpack, Rspack and Next.js (Turbopack), and reading a
  theme's `@source` directives (Vite, Rollup, Rolldown and esbuild have it now).
- **Accessibility**: PDF/UA-2 (PDF 2.0, with structure destinations); structure elements in
  object streams, so tagged files grow less; list numbering attributes; tagging the parts of SVG
  images.
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
  ranges for SVG text, which takes the nearest instance a document declares; an instance's
  `FontBBox` from its own outlines (it is the default instance's, from `head`, now, which a heavy
  instance can exceed).
- **SVG text**: vertical text on a path; `Identity-V` fonts for vertical text, so that each
  glyph needs no text matrix of its own; font properties passed through `use`; `style` and
  `className` on the primitives inside `<Svg>`.
- **Colour emoji**: SVG-in-OpenType fonts; COLRv1 sweep gradients and blend modes (a sweep
  gradient draws with its first stop now, and every blend mode as source-over); colour glyphs as
  Type 3 fonts instead of images.
- **Forms**: `sort`, `edit`, `noSpell` and `align` on fields.
- **Notes**: an `icon` prop, for the other note icons.
