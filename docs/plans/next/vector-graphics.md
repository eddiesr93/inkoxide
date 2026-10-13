# Plan: text in `<Svg>`, `<Marker>` and `<Canvas>`

Part of [Plans for "Next"](README.md). Draft for review; line references point to `f038fed`.

> **Decided on 2026-10-13:** Inkoxide lays out SVG text with its own shaper and writes real PDF
> text. usvg's `text` feature stays off: it depends on harfrust 0.12, which waits with #83 for a
> release that includes harfrust#484, and shaping inside usvg would run outside the render's
> glyph limit. Rejected alternatives: usvg converting text to outlines (not selectable), and
> usvg laying out text that Inkoxide writes as PDF text (both need harfrust).

## `<Marker>`

- **The engine already draws markers.** usvg converts them, and the SVG checks charge them once
  per vertex (crates/core/src/images/svg.rs:875-925, 1186-1195; tests at :1566 and :1740). Only
  the React side is missing.
- **API.** `<Marker id viewBox refX refY markerWidth markerHeight orient markerUnits>`, where
  `orient` is `'auto'`, `'auto-start-reverse'` or a number. `SvgPresentation`
  (components/index.ts:549-567) gains `markerStart`, `markerMid` and `markerEnd`;
  `GradientProps` (:628-642) gains `fr`. `Marker` and `MarkerProps` are exported from both
  packages.
- **React.** Add `marker` to `SVG_TAGS` (tree/svg.ts:9-23), and `refX`, `refY`, `markerWidth`,
  `markerHeight` and `markerUnits` to `CAMEL_ATTRS` (:26-33).
- **Tests.** Marker attributes in features.test.tsx, exports.test.ts, and a visual fixture with
  markers.
- **Effort.** S, 1 day.

## Text in `<Svg>`

The design: before usvg converts the file, each `<text>` is replaced with a placeholder path, a "probe", whose geometry is the text's ink box from this render's layout. Probes are found again by their geometry, not by id, because usvg drops ids in `use` copies and markers.

**Scope and API**
- **React API.**
  - **`Text` inside SVG.** A `Text` inside `Svg`, `G`, `Defs`, `ClipPath` or `Marker` is SVG text.
    - Props: `x, y, dx, dy, rotate` (number, number[] or string), `textAnchor`, `dominantBaseline`, `fontFamily`, `fontSize`, `fontWeight`, `fontStyle`, `letterSpacing`, `wordSpacing`, `textDecoration`, `textLength`, `lengthAdjust`, plus `SvgPresentation`.
    - Children: strings and `Tspan`. `TspanProps` are the same, plus `baselineShift`.
    - `Text` is typed `TextProps | SvgTextProps`.
  - **Additions to existing types.**
    - `SvgPresentation` (components/index.ts:549-567) gains `markerStart`, `markerMid`, `markerEnd`, `textAnchor`, `dominantBaseline`, `visibility` and the font props.
    - `GradientProps` (:628-642) gains `fr`.
  - **Exports** from both packages: `Tspan`, `Marker`, `SvgTextProps`, `TspanProps`, `MarkerProps`.
- **Engine:**
  - `<text>`, `<tspan>`, and `<a>` inside text, in SVG files and `<Image>` SVGs, become selectable PDF text with ToUnicode.
  - The schema keeps its shape. Only the descriptions of `fonts[].family/weight/style` change: they now match SVG `font-family`.

### 1. Where text is intercepted: before usvg
- **After usvg is not possible.** Without the `text` feature, usvg converts nothing of a `<text>`:
  - `EId::Text` is compiled out in `usvg-0.48.1/src/parser/converter.rs:686-691`, and in clip-path content at :752-757.
  - Its svgtree copy (`svgtree/parse.rs:207-208`) is private (`parser/mod.rs:14,26`).
  - So `images/svg.rs` must rewrite the source before usvg sees it.
- **The placeholder ("probe")** is a path whose geometry is the text's ink box from that render's layout:
```
<path id="(text's id)" d="M x0 y0 H x1 V y1 H x0 Z"
 style="transform:…;opacity:…;clip-path:…;mask:…;filter:…;display:…;   (text-level effects)
        fill:…|inherit;fill-opacity:…|inherit;stroke:…|inherit;…;paint-order:…|inherit;visibility:…|inherit;
        marker-start:none;marker-mid:none;marker-end:none"/>
```
  - When the spans use more than one paint style (K > 1), the placeholder is an `<a id style="(effects)">` holding K probes, one per style, in order.
  - Inside `<clipPath>` the placeholder is always bare probe paths.
- **Checked against usvg 0.48.1 source:**
  - `<a>` is parsed as `g` (`svgtree/parse.rs:203-206`). A `g` is kept even when empty and without an id (converter.rs:814, 897-910, 659).
  - Invisible paths stay in the tree; `Path::new` fails only on non-finite bounds (converter.rs:935-955; `tree/mod.rs:1311-1337`).
  - The path's transform comes from its group (converter.rs:808, 821).
  - Markers would be drawn on probes (converter.rs:990-1023), hence the explicit `none` values.
  - Declarations in `style` override author rules such as `path{fill:none}`, because rules are applied first and `style` last (`parse.rs:381-399`).
  - `inherit` resolves from the svgtree ancestors, including `use` copies (`parse.rs:462-470`).
  - Clip-path content takes only graphic elements, and `g` is not one (converter.rs:716-735; `svgtree/mod.rs:653-666`). That is why probes there are bare paths.
- **How probes are recognised: by geometry, not id.**
  - usvg drops ids in `use` copies (`parse.rs:244-248`) and inside markers (converter.rs:813-819, 1026-1031).
  - The writer keeps a map from the four corner values' bits to the text index. It only matches paths with exactly this 5-segment rectangle shape.
  - Two texts with identical boxes are made distinct by nudging x1 with `f32::next_up`.
  - A crafted collision can only make an SVG draw its own text in place of one of its own rectangles.
- **Drawing order, transforms and copies follow automatically**, because usvg copies a probe like any other path: `use`/`symbol`, `switch`, `pattern`, `mask`, `marker` (`context-fill` works), opacity groups.
- **Nested SVGs** (data: URLs) go through the same function from `resolve_data` (`svg.rs:481-518`), sharing one map per outermost SVG.
- **Gap:** text with K > 1 paint styles, referenced by a `use` inside a `clipPath`, is dropped (`use_node.rs:283-286`).

### 2. Cascade
usvg's resolved values can't be read back: svgtree is private, and the output tree keeps only paint and stroke data on paths. So the work is split:
- **Paint is resolved by usvg on the probes.** This covers fill, stroke and the `stroke-*` properties, opacities, `color`/`currentColor`, paint-order, visibility and `context-*`.
  - Each probe's `style` holds the value declared on the text or its tspans, or `inherit` otherwise. Paint therefore inherits through `use` correctly.
- **Text properties are resolved by our own cascade**, in `images/svg_text/extract.rs`. This covers:
  - the `font-*` properties and the `font` shorthand;
  - letter-spacing and word-spacing, text-anchor, dominant-baseline, alignment-baseline, baseline-shift, text-decoration;
  - direction, unicode-bidi, writing-mode, xml:space;
  - the text-level effects copied onto the placeholder (transform is a CSS property in usvg, `svgtree/mod.rs:686-750`).
- **The cascade is a port of usvg's:**
  - presentation attributes first, then sheet rules in order with `!important` (`parse.rs:294-336, 381-392`), then `style=""`;
  - `<style>` collection and selector matching through simplecss over roxmltree nodes, exactly like usvg's `XmlNode` (`parse.rs:659-718`);
  - `inherit`, and inheritance along the XML ancestors;
  - font-size resolution ported from `units.rs:94-145`.
- **Limitation:** font properties set on a `use` don't reach the text it copies.
- **Cost:** only text, tspan and their ancestors are matched, once each, and charged to the existing `rule_matches`/`style_work` counts (`svg.rs:1131-1139`).

### 3. Layout (new `text/svg.rs`, per render, through the render's shaper)
- **Whitespace:** port usvg's xml:space handling (`svgtree/text.rs:133-362`).
- **Per-character positions:** each code point gets x/y/dx/dy/rotate from the lists on the text and its tspans. Inner elements win, the last rotate value repeats, lengths take units, and % is of the viewport.
- **Shaping:** a new chunk starts at each absolute x or y.
  - Each chunk is one `TextShaper::prepare` call (`text/mod.rs:265-270`). It gets one `TextRun` per span (font id from `FontSet::select`, size, `letter_spacing`), `WhiteSpace::Pre`, no hyphenation, and the SVG `direction`.
  - This reuses existing machinery:
    - font fallback (`prepare.rs:155-205`), bidi and the shape cache;
    - ToUnicode recording (`prepare.rs:381`);
    - the `MAX_EXTRA_GLYPHS` spend (`prepare.rs:446`).
  - Visual order comes from `text/paint.rs` `reorder` (:296, made `pub(crate)`).
- **After shaping:**
  - word-spacing is added to space glyphs (`prepare.rs:452-454`); dx/dy apply per cluster;
  - text-anchor applies per chunk (start/middle/end, mirrored for right-to-left);
  - dominant-baseline, alignment-baseline and baseline-shift come from font metrics. `FontMetrics` (`fonts/mod.rs:26-46`) gains x-height and the OS/2 sub/superscript offsets;
  - textLength/lengthAdjust: `spacing` adds gaps, `spacingAndGlyphs` scales horizontally;
  - rotate applies per glyph;
  - underline and line-through use the font's metrics (`fonts/mod.rs:40-43`); overline sits at the ascent.
- **Ink box:** the union of glyph outline boxes (new `LoadedFont::glyph_bbox`, ttf-parser, kept in `fonts/`) plus decorations.
  - It becomes the probe geometry, so objectBoundingBox paint, clip and mask resolve against it, as usvg does with its own text.
- **Not in v1:**
  - `textPath` and vertical `writing-mode` fail with `Reject::Unsupported` "…; convert it to outlines", as all text does today (`svg.rs:579-583`).
  - Ignored: font-variant, font-stretch, font-kerning, font-feature-settings, glyph-orientation, `tref`.

### 4. Paint and PDF output
- **Drawing the text** (`pdf/svg.rs` `children()`, :623-643): a path found in the probe map draws its text instead of itself. For each span:
  - paint comes from the probe for that span's style: `set_paint` (:701) with the ink box, plus the stroke setup factored out of `path()` (:663-686);
  - text render mode 0 or 1, in paint-order; skipped when `!is_visible()`;
  - underline and overline are drawn before the glyphs, line-through after.
- **Glyph output:**
  - `BT /F<key> 1 Tf`, then a `Tm` per run carrying size, the y-flip, rotation and scale, written through `number()`;
  - glyphs on one baseline share a `TJ`, as `show_text` does (`pdf/mod.rs:770-820`);
  - `.notdef` is skipped under PDF/A;
  - in clip mode (:627-635) the text is filled, and `simple_clip` (:301-318) returns `None` for probes;
  - new `ResKind::Font` (:79-113); glyphs count toward `MAX_SEGMENTS` (:433-439).
- **Glyph accounting:**
  - The laid-out glyphs of every painted SVG are merged into `used` (`pdf/mod.rs:268-286`). This is a superset of what is drawn, so fonts used only in SVG text are still embedded and subset.
  - ToUnicode needs no extra work: layout runs before `into_unicode` (`lib.rs:321`).
  - Fonts are written before images (`pdf/mod.rs:296-318`), so the font references are ready when the SVG forms are written. Nothing needs reordering.
- **SVG size:** `write_svg` (`pdf/svg.rs:47-74`) takes the size from the cached decode, not from the per-render tree.
  - An SVG with no width/height/viewBox gets its size from its content (converter.rs:600-605), and the probes would change that.

### 5. Caching
- **Process-wide cache, by content** (`images/mod.rs:99-104`):
  - SVGs without text are unchanged.
  - SVGs with text cache the font-free tree (text dropped; used for size and validation) plus an `Arc<SvgTextDoc>`. Per text, that holds the source byte range, placeholder effects, resolved properties, characters and positions, spans and paint declarations.
- **Per render**, after `lib.rs:305-306`, sequentially in document order:
  1. lay out each text;
  2. splice the placeholders into the source;
  3. run `parse_xml` + `check` + usvg on the result;
  4. build the probe map; `ImageSet` gets a per-render `DecodedImage`.
- **Cost:** about one extra decode of that SVG per render, plus shaping.
- **Cross-render reuse:** a `SharedLru` (like `images/cache.rs:18`) keyed by xxh3 of the rewritten XML. When fonts give the same layout, the tree is reused.

### 6. Limits and fuzzing
- **Text volume:** a new `Cost.text_chars`, counted per copy in `Walk::visit` (`svg.rs:1097-1206`). `MAX_TEXT_CHARS` = 1,000,000, counting `use`/marker/pattern copies and nested SVGs. Layout runs once per element; output runs once per copy.
- **Shaping:** SVG text spends `MAX_EXTRA_GLYPHS`. A `ShapingFailure` becomes a render error naming the font (`lib.rs:247-258`).
- **Probes:** `check()` runs again on the rewritten document, so probes count under `MAX_EXPANDED`.
- **Bad sizes:** non-finite or non-positive font sizes draw nothing.
- **Validation:** an SVG with text in a document that declares no fonts fails with `INVALID_IMAGE` (`validate/rules.rs:~560`).
- **Fuzzing:**
  - `check_image` declares the fixture fonts (`fuzz/src/lib.rs:111` has `"fonts":[]` today).
  - The generator (`fuzz/src/svg.rs:65-131, 382-398`) emits:
    - text/tspan with position lists, font props, anchors, baselines and textLength;
    - CSS rules on text, tspan, `path`, `a` and `*`;
    - text inside clipPath, mask, pattern, marker and symbol, plus `use` copies;
    - text in nested SVGs, and `url()`/`context-fill` paints.
  - Add dictionary entries, and seed with `fixtures/images/text.svg`.

### 7. React
- reconciler/host-config.ts:
  - `getChildHostContext` (:208-209) maps a `Text` under SVG content to a new context `ink-svg-text`.
  - `allowed` (:78-109) lets `Text` into SVG containers, and only `Tspan` into SVG text.
  - `createTextInstance` (:226-229) accepts strings there.
- tree/svg.ts:
  - add `marker` and `tspan` to `SVG_TAGS` (:9-23), and write `Text` as `text`;
  - escape strings instead of throwing (:69-79);
  - add `refX`, `refY`, `markerWidth`, `markerHeight`, `markerUnits`, `textLength`, `lengthAdjust` to `CAMEL_ATTRS` (:26-33).
- Fonts: `svgMarkup` takes a resolver from `drawing()`.
  - It resolves the inherited family, weight and style through `FontRegistry.resolve`, using `parseFontFamily` (packages/style/src/values.ts:350).
  - It marks the font id as used (build.ts:268-280), and writes the chosen face's family, weight and style, so the engine finds an exact declared match.
  - The enclosing text style goes on the root `<svg>` as the default.


### 8. Bounding boxes
Probes are ordinary paths, and group boxes include every child (`tree/mod.rs:1866-1906`). So the transparency form box (`pdf/svg.rs:494`) and the clip and mask forms (:512, :600) include the text's ink and stroke without changes.

### Tests
- **Unit tests:**
  - replace the text-rejection test (`svg.rs:1310-1321`);
  - cascade cases: `!important`, `style`, type/class/descendant selectors, `inherit`, font shorthand;
  - whitespace, positions and chunks, anchors, baselines, textLength;
  - the `FontSet::select` table, shared with a registry.ts Vitest.
- **Probe-survival suite** across contexts: plain, `use`, `symbol`, `marker`, `pattern`, `mask`, `clipPath`, `switch`, nested SVG, opacity group, and author CSS on `path`/`a`. It should fail loudly on a usvg upgrade.
- **Writer tests:** BT/TJ output, gradient-filled text, stroked text, clip text, PDF/A `.notdef`.
- **Fixture** `svg-text.json` + `text.svg`: passes the byte-determinism, MessagePack, qpdf and veraPDF checks, plus visual references and a ToUnicode assertion.
- **`limits.rs`:** `use` bombs of text, and `glyph-doubling.ttf` inside SVG text.
- **`finite_numbers.rs`:** font sizes and positions of 1e30.
- **React:** markup, fonts, placement errors (replacing `features.test.tsx:998-1006`), Marker.

### Docs
- `GUIDE.md:524-527` becomes the list of what SVG text supports, including the textPath, vertical-text and font-through-`use` limits.
- `MIGRATING.md:24`, `ROADMAP.md:15-16`, CHANGELOG.
- Schema CHANGELOG, and a regenerated SCHEMA.md for the `fonts` descriptions.
- ARCHITECTURE.md: SVG text goes through `prepare.rs`; simplecss and svgtypes are used only in `images/svg_text/extract.rs`.
- TESTING.md, and an ADR on the probe design.

### Dependencies
- `simplecss =0.2.2` and `svgtypes =0.16.1` become direct dependencies (Apache-2.0 OR MIT). They are the same versions usvg already pulls into Cargo.lock, so the binary doesn't grow and CSS behaves identically.
- No usvg features and no harfrust.
- Every simplecss call in `svg_text/extract.rs` is charged to the same CSS work limits as `images/svg.rs`.

### Effort and risks
- **Effort: L, about 20 days.**
  - Fonts: 1.5 days. Extraction and cascade: 4. Layout: 4.
  - Rewrite, probes and cache: 3. Writer and glyph accounting: 2.5.
  - Tests, fuzzing and visual references: 3. React: 2. Docs: 1.
  - `<Marker>`: S, 1 day, React only (unchanged).
- **Risks:**
  - The probe design depends on usvg internals listed in §1. usvg is pinned (`Cargo.toml:32`) and the probe-survival suite guards upgrades.
  - Two cascades (ours for text properties, usvg's for the rest) can drift on edge cases.
  - Converting text SVGs again on each render costs time; the LRU mitigates it.
  - An author `!important` rule on `a` or `*` can still override the placeholder.

### Open decisions
1. **`textPath` and vertical text in v1:** reject them (recommended; consistent with today), or draw them approximately.
2. **Properties passed by `use`:** paint yes and fonts no in v1 (recommended).
3. **Default SVG font size:** 16 like browsers, also set as usvg `Options.font_size` so `em` units stay consistent (recommended; note in the CHANGELOG), or keep usvg's 12.
4. **`<Image>` SVG files that need fonts not used elsewhere:** declare them through `fallbackFonts` (recommended for now).
5. **`style`/`className` on SVG primitives:** keep props only (recommended).

## `<Canvas>`: text, clipping, gradients, miterLimit

`font`, `fontSize` and `text` depend on the first version of SVG text: they need text-anchor (`align`/`width`), dominant-baseline (pdfkit's default `top` becomes `text-before-edge`), letter- and word-spacing, text-decoration, exact `FontSet::select` matches on the faces React resolves, and gradient-filled text through probes. Clipping, gradients and `miterLimit` are independent of it.

**API** (packages/react/src/canvas.ts)
- **`miterLimit(n)`** sets `stroke-miterlimit`.
- **`clip(rule?)`** emits `<clipPath id="ink-cN"><path …/></clipPath>` and opens `<g clip-path>`. It counts that group in `groups`, so `restore()` (:166-179) closes it.
- **Gradients.**
  - `linearGradient(x1,y1,x2,y2)` and `radialGradient(x1,y1,r1,x2,y2,r2)` return a `CanvasGradient` with `stop(offset, color, opacity?)`.
  - The gradient is accepted by `fill`, `stroke`, `fillAndStroke`, `fillColor`, `strokeColor` and `text`.
  - It is written where it's used with `gradientUnits="userSpaceOnUse"`, so its coordinates are in the current user space, as in pdfkit. Radial: the first circle becomes `fx fy fr`, the second `cx cy r`.
  - The engine already draws all of this, including `fr`, clips and miter limits (pdf/svg.rs:725-742, 497-513, 678).
  - The API follows pdfkit's: the gradient methods return a gradient object with `stop()`.
- **`font(name, size?)`** takes a registered font id or a family name.
  - Standard-14 names such as `Helvetica-Bold` map to the default family with the suffix's weight and style, and give the usual "not registered" warning.
  - `fontSize(n)` sets the size; the default is 12.
- **`text(str, x, y, opts?)`** emits one `<text>`. Options:
  - `align` with `width` sets the anchor;
  - `baseline` defaults to `top`, which becomes `text-before-edge`;
  - `characterSpacing`, `wordSpacing`, `underline`, `strike`, `stroke`.
  - There is no line wrapping.
- The `Painter` constructor (build.ts:1014) receives a font resolver that marks fonts as used.

**What depends on item 1:** only `text`, `font` and `fontSize` (they need engine `<text>`, `dominant-baseline` and font matching). Clipping, gradients and `miterLimit` need nothing new in the engine.

**Security:** the painter's output goes through the same SVG checks, and its strings are escaped with `escapeXml` (canvas.ts:9-10).

**Tests:** Vitest cases next to features.test.tsx:1055-1076 for:
- clip nesting and `restore`;
- gradient reuse;
- miter limit;
- each text option;
- `doc.fonts`;
- errors.

Plus a render smoke test and an example.

**Docs:** MIGRATING.md:24, GUIDE.md:508-522, CHANGELOG, and the `CanvasGradient` export.

**New dependencies:** none.

**Effort:** M, about 4 days (2 before item 1, 2 after).

**Risks:** pdfkit's baseline is only approximated through SVG `dominant-baseline`.

**Open decisions**
1. Default miter limit: pdfkit leaves PDF's 10, but today's output gets SVG's 4. Recommended: 10, listed as a change.
2. A `\n` in `text()`: recommended throwing ("one line per call"), because React has no font metrics to compute a line height.
3. Default font: recommended the Canvas's inherited family, weight and style, at pdfkit's 12 pt.

## Dependencies and order
- **Other roadmap items:**
  - The deferred HarfRust migration (#83) now touches only `text/prepare.rs`; SVG text follows it automatically.
  - Colour emoji will also cover SVG text.
  - The "Now" item `fontFeatureSettings` could later be applied to SVG text through `TextRun`.
- **Commits, each green:**
  1. React `<Marker>`, the marker props, `fr`, `CAMEL_ATTRS`, and a marker visual fixture.
  2. Canvas `miterLimit`, `clip` and gradients, with `CanvasGradient`.
  3. `fonts/`: `FontSet::select`, `glyph_bbox`, x-height and sub/superscript metrics, and the shared matching table.
  4. `images/svg_text/extract.rs`: cascade, whitespace, positions, `text_chars` limits, and the v1 rejections. Text is still rejected overall, so nothing changes for users.
  5. `text/svg.rs` layout, unit-tested with fixture fonts; no visible change yet.
  6. Rewrite with probes, per-render conversion, and the probe-survival suite; probes draw nothing yet.
  7. `pdf/svg.rs` text output, font resources, `used`/ToUnicode, PDF/A, the validation rule, lifting the rejection, and the `svg-text` fixture with visual references.
  8. The LRU for per-layout trees, plus a benchmark.
  9. Fuzz generator, dictionary and `check_image` fonts; `limits.rs`; `finite_numbers.rs`.
  10. React SVG `Text`/`Tspan`.
  11. Canvas `font`, `fontSize`, `text`.
  12. Docs sweep and the ADR.

### Critical files for implementation
- `crates/core/src/images/svg.rs`
- `crates/core/src/pdf/svg.rs`
- `crates/core/src/text/prepare.rs` (with a new `text/svg.rs`)
- `crates/core/src/fonts/mod.rs`
- `packages/react/src/tree/svg.ts`
