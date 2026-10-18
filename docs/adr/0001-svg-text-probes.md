# 1. SVG text is laid out by Inkoxide and drawn at probes

Status: accepted, 2026-10-13.

## Context

`usvg` converts SVG into the tree the PDF writer draws. Inkoxide builds it without its `text`
feature, so a `<text>` element converts to nothing, and SVG text used to fail the render.
Turning the feature on was not an option:

- it depends on harfrust 0.12, which waits, with the HarfRust migration (#83), for a release
  that includes harfrust#484;
- shaping inside `usvg` would run outside the render's glyph limit (`MAX_EXTRA_GLYPHS`), its
  fonts and its font fallback;
- `usvg` converts text to outlines, which cannot be selected or searched; keeping it as text
  would still need harfrust.

Without the feature, `usvg` keeps nothing of a text element: its parsed tree (`svgtree`) is
private, and the converted tree has no text nodes. Whatever Inkoxide does with text has to
happen on the XML, before `usvg`, and has to survive `usvg`'s conversion (`use` copies,
markers, patterns, masks, clip paths, `switch`, nested SVGs) to be drawn where the text was.

## Decision

Inkoxide lays SVG text out with its own shaper and writes it as PDF text. Each `<text>` is
replaced, before `usvg` converts the file, by a placeholder: a "probe", a rectangle path over
the text's ink, or a group of probes, one per paint style of its spans.

- **Reading** (`images/svg_text/extract.rs`, once per SVG, cached with it): the cascade of the
  text properties, ported from `usvg` (presentation attributes, style sheets through
  `simplecss`, `!important`, `style`, `inherit`, the `font` shorthand), white space after
  `xml:space`, character positions and rotations, spans and their declared paint.
- **Layout** (`text/svg.rs`, per render): chunks shaped through `TextShaper::prepare`, so font
  fallback, bidi, the shape cache, the glyph limit and ToUnicode apply as for any text; fonts
  chosen by `FontSet::select`, CSS font matching over the document's declared fonts.
- **Probes** (`images/svg_text/rewrite.rs`): each probe's `style` holds the text's effects
  (transform, opacity, clip, mask, filter, display) and the paint its spans declare, so `usvg`
  resolves the paint the text would get, through `use` copies too. Where a style sheet
  declares a paint property or an effect, the probe declares it too (`inherit`, or the initial
  value), so that a rule about the probe's element cannot reach it. `usvg` applies an
  `!important` rule of a sheet over `style` (the first `!important` declaration it meets
  wins), so a probe is a `path`, or a `polygon` when an `!important` rule names paths; a group
  of probes is an `a`, or a `g`. The probe's geometry is the text's ink box, so
  `objectBoundingBox` paint, clips, masks and group bounds resolve against it.
- **Recognition**: the PDF writer finds a probe by its geometry, not its id, because `usvg`
  drops ids in `use` copies and markers. Each probe's corners carry its number in their last
  bits (a few units in the last place), so they are unique within an SVG; a deliberate collision
  with a rectangle can only make an SVG draw its own text in place of one of its own
  rectangles.
- **Drawing** (`pdf/svg.rs`): a probe draws its text in its place, with its fill and stroke
  (gradients and patterns included) in paint order, as PDF text in the document's embedded
  fonts. Colour glyphs and emoji images (`emojiImages`) are images, in spans whose
  `ActualText` is their text, as page text draws them (`ColorGlyphs::push`). An emoji image
  that is an SVG with text of its own is left out there, so that no form draws itself.
- **Per render**: the SVG is converted again with the probes, under the same checks; the result
  is cached by the rewritten source, so renders whose fonts lay the text out the same way
  reuse it. It keeps the size of the SVG without its text: an SVG without a size takes it from
  what it draws, which would otherwise depend on the fonts.
- **Limits**: what a text's placeholder is (a probe, a group of them, bare probes in a clip
  path) depends on the text alone, not on its layout. So the decode builds the placeholders
  that cost most (every paint style, the widest geometry), counts their bytes before writing
  them, and runs the SVG checks on that document: a render's conversion costs the checks no
  more, and fails a limit only where validation does. The text nodes of a text count as
  elements.

## Consequences

- SVG text is selectable, searchable and in the document's fonts, and does not wait for #83.
- The design depends on `usvg` internals: how it parses `a`, keeps invisible and empty groups,
  resolves `inherit` and copies paths. `usvg` is pinned, and the probe survival suite
  (`images/svg_text/survival.rs`) checks every context on an upgrade.
- Two cascades exist: ours for text properties, `usvg`'s for the rest. They can drift on edge
  cases; ours is a port of `usvg`'s, with the same `simplecss` and `svgtypes` versions. So do
  two readers of geometry: the paths a `textPath` follows are read by ours (`path.rs`, with the
  same `svgtypes`), the rest by `usvg`.
- An SVG with text is converted again on each render that lays its text out differently.
- Not in this version: text that an XML entity expands to (refused, as all text was: the
  placeholder replaces the element where it is in the source, and an entity's text is
  somewhere else) and `tref`. An author `!important` rule that matches any element (`*`)
  applies to the text's paint, as it does in a browser; one that names both `path` and
  `polygon` reaches the probes.
- Text on a path and vertical text (along a path too), added since, fit the design unchanged.
  The path a `textPath` follows is read from the XML with the text (`usvg` never sees it, and a
  path in `<defs>` is not in its tree), once per element it refers to, and the probe covers the
  ink wherever the glyphs go. Each glyph on a curve, and each in a column, has its own text
  matrix, and the text of each paint style is written once as `ActualText`, in logical order.
- `font-stretch` and font properties passed through `use`, also added since, stay in our
  cascade: `font-stretch` selects a face's width, and a `use` that gives the text it names
  other font properties gets a copy of that element, laid out with the `use`'s cascade and
  written into the SVG under an id of its own (`UseCopy`, at most 64 an SVG).

## Alternatives

- `usvg`'s text feature, converting text to outlines: not selectable, needs harfrust, shapes
  outside the glyph limit.
- `usvg`'s text feature for layout only, with Inkoxide writing PDF text from its layout: still
  needs harfrust, and two shapers would disagree on glyphs.
- Finding placeholders by id: `usvg` drops ids where it copies, so copies would be lost.
