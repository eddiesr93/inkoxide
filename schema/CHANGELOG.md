# Schema changelog

## v1 (draft, M0)

Initial contract: `box`, `text`, `image`, `table`, `pageBreak`, `spacer`
nodes; flex/grid layout styles; visual styles; text paragraph styles;
pagination controls (`break`); header/footer per page template;
`pageNumber`/`totalPages` placeholders. See `docs/SCHEMA.md`.

v1 stays a draft (non-breaking additions only, logged here) until the
first npm release (M5); after that, breaking changes bump `version`.

### Additions

- M5: `image.data` also accepts a `data:<mime>;base64,` URL (description
  only; any base64 string was already valid).
- M7: `link` on `box`, `text`, `image`, `table` and `spacer` nodes and on
  text runs (`#id` or an absolute URI); `bookmark: { title, level }` on the
  same nodes builds the document outline. New error code `UNKNOWN_ANCHOR`.
- M7: `borderCollapse: "collapse" | "separate"` and `borderSpacing` on
  tables (separate mode: each cell draws its own borders and background,
  cells are spaced by `borderSpacing`).
- M7 grid parity: track objects `{min, max}` (minmax) and `{fitContent}`;
  `{repeat, tracks}` in `gridTemplateColumns`/`gridTemplateRows`
  (count, `auto-fill` or `auto-fit`); `gridAutoColumns`, `gridAutoRows`,
  `gridTemplateAreas`, `gridArea`, `justifyItems`, `justifySelf`.
- Post-M7: `discriminator: { propertyName: "type" }` on `Node`. It is an
  annotation only (draft 2020-12 ignores it); ajv with
  `discriminator: true` uses it to report errors for the matching node
  type only, with the same paths as the engine.
- v1.1 paint: `transform` (list of `translate`, `scale`, `rotate`,
  `skew`, `matrix`) and `transformOrigin`; `zIndex` (paint order among
  siblings); `borderRadius` also takes a per-corner object;
  `borderStyle` also takes a per-side object.
- v1.1 text: `whiteSpace` also `pre-wrap` and `pre-line`; `textIndent`;
  `direction` (`ltr`, `rtl`, `auto`) with Unicode bidi reordering;
  `textAlign` also `start` and `end` (now the default, `start`, is `left`
  in left-to-right paragraphs, as before); text runs take
  `decorationColor`, `decorationThickness` and `decorationStyle`
  (`solid`, `double`, `dotted`, `dashed`, `wavy`).
- v1.1 pagination: `orphans` and `widows` on text nodes (default 1);
  `pages` on `box`, `text`, `image`, `table` and `spacer` nodes
  (`first`, `last`, `notFirst`, `notLast`, `odd`, `even`, or a list of
  page numbers, negative from the end) paints a node on some pages only.
  Table rows taller than a page now split between lines inside their
  cells instead of being sliced (behaviour only).
