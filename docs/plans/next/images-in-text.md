# Plan: inline images, colour emoji and background images

Part of [Plans for "Next"](README.md). Draft for review; line references point to `f038fed`.

## 1. Inline images inside `Text`

**Scope and API**
- **React.** `<Image>` becomes allowed inside `<Text>`, and inside a `<Link>` that sits in a `<Text>`. Today the `INLINE` set (`host-config.ts:53`, `:86-87`) and the error at `:147-150` forbid it.
  - Props: `src` (every `ImageSrc` form), `fit`, and a new `alt?: string`.
  - Style: `width`/`height` in pt or em (percentages rejected), `objectFit`, `objectPosition` and `verticalAlign`.
  - Box keys (margin, padding, border, background) are rejected, the way nested Text rejects them (`build.ts:774-786`).
- **Schema.** `TextRun.image` points to a new `$defs/InlineImage`:
  - `src` | `data`, `width` and `height` in (0, 14400], `fit`, `objectPosition`, `alt` (at most 1024 characters).
  - The run's `oneOf` (`document.v1.json:1060-1081`) gets a third branch: text, placeholder or image.
  - `font` and `size` stay required: they are the strut and the em.
- **Vertical alignment.** Reuse the run-level `verticalAlign` that the "Now" superscript/subscript item adds. Extend it with `middle`, `textTop`, `textBottom`, `top`, `bottom` and a number (points, up). Values other than `super`/`sub` are valid only on image runs.
- **Default size.** With neither width nor height, the image is 1em high (`run.size`) and its width follows the aspect ratio. With one of them, the other follows the ratio.

**Engine design**
- **Schema type.** `TextRun` (`schema/mod.rs:467`) gets `image: Option<Box<RunImage>>`, reusing `ImageSource` (`schema/mod.rs:408`) so base64 decoding is shared.
- **Validation.** `rules.rs:776 run()` checks that exactly one of text, placeholder or image is set. It then calls `image_source()` (`:531`) and `object_position()` (`:567`) on `runs[i].image`, and checks the bounds.
- **Image loading.** `ImageSet::load` (`images/mod.rs:205-255`) also walks text runs and fills a `by_run` map. It hands out an `Arc<InlineObjects>` (run → key and intrinsic size), so the shaper never borrows the `ImageSet`.
  - In `lib.rs:304-306`, load images before building the shaper, and add `RustybuzzShaper::with_objects`.
- **Preparing the paragraph** (`prepare.rs`):
  - `build_text` (63-118) pushes U+FFFC for an image run. Its line-break class is CB, which allows a break before and after (UAX #14 rule LB20). Its bidi class is ON (neutral), as in CSS.
  - Image runs get no soft hyphens (the condition at line 108).
  - `itemize` (155-205) gives each image run its own segment.
  - The shaping loop (426-479) skips that segment and pushes one `Glyph { advance: w, flags: GLYPH_OBJECT }`.
  - `Segment` gets `object: Option<u32>`, an index into a new `Prepared.objects: Vec<InlineObject{key,w,h,align,fit,position,alt}>`. Indexing by segment means `fill_numbers` (591-704), which clones segments, only has to copy `objects` too.
- **Glyph flag.** Add `GLYPH_OBJECT` next to the flags at `text/mod.rs:30-32`.
- **Line metrics.** `lines.rs metrics` (256-300) adds each object's extents:
  - baseline: above = h + shift, below = -shift;
  - `middle` needs the x-height, so add `x_height` to `FontMetrics` (`fonts/mod.rs:26`);
  - `textTop`/`textBottom` use the strut font;
  - `top`/`bottom` take a second pass after the others.
  - `break_lines`, `min_content` and the ellipsis need no change: an object is one cluster with an advance.
- **Painting.** `paint_line` (`paint.rs:91-276`) leaves object glyphs out of the `TextOp`.
  - It records (x, object) per object, then draws the image with `SetOpacity(ctx.opacity)` and `ImageSet::paint_in`.
  - No decorations on objects. The link areas (218-227) already cover the piece.
  - `TextPainter` (30-37) gains `images: &ImageSet`.
  - Split `ImageSet::paint` (`images/mod.rs:307-362`) into `paint_in(key, box, fit, pos, radii, ops)`.
- **`alt`** becomes ActualText through `Op::Span` (see item 2).
- **Repeated-content estimate.** `pipeline.rs:364` counts a run without text as 8 characters; give image runs a fixed `OBJECT_BYTES` instead.
- **Pagination** needs no change: taller lines paginate as lines (`paginate/mod.rs:803-822`).

**Limits and security**
- Sizes are bounded by `MAX_PAGE_EDGE`.
- Images are deduplicated by hash (`images/mod.rs:237-249`) and decoding is capped by `MAX_PIXELS` (`:39`).
- React resolves `src` through `ImageTable`, so the remote and local image policies apply unchanged.

**Tests**
- Unit tests:
  - breaks around an object;
  - `nowrap`;
  - right-to-left order;
  - `maxLines` and ellipsis;
  - metrics for each alignment;
  - `fill_numbers` keeping its objects.
- Validation tests. `parity_tests` picks up the new constants by itself.
- Fixture `text-inline-images.json` with its expected PNG.
- React tree tests and a Node render test.

**Docs**
- Regenerate `SCHEMA.md` and `index.d.ts`.
- Schema CHANGELOG "Unreleased", the GUIDE Images section and the ARCHITECTURE text pipeline.

**New dependencies:** none.

**Effort:** M–L, 5–7 days.

**Risks**
- It depends on the run-level `verticalAlign` from superscript/subscript.
- The per-line scans over all segments (`lines.rs:284`, `paint.rs:137`) cost O(segments) per line. Many object segments make that slow; use `partition_point`.

**Open decisions**
- **Default size.** Recommended: 1em high, keeping the aspect ratio. Alternatives: CSS intrinsic size, or 1em × 1em.
- **Decoration and letter spacing on image runs.** Recommended: accepting and ignoring them, since the React context fills them in.
- **`top`/`bottom` alignment in v1.** Recommended: yes; it is small.

## 2. Colour emoji

**Scope and API**
- **Emoji from images:** `Font.registerEmojiSource({ url, format='png', withVariationSelectors=false } | { builder: (codePoints) => ImageSrcValue } | null)`. React stores it the way `hyphenationCallback` is stored (`font.ts:177-182`).
- **Emoji from a font:** `Font.register({ family:'Noto Color Emoji', src })`, then list the font in `fallbackFonts`.
- **Schema:** a document-level `emojiImages: { "<sequence>": "<image id>" }`, at most 4096 entries, next to `fallbackFonts` (`schema/mod.rs:32-41`). This keeps a stored `result.document` plus `result.images` re-renderable, and lets `validate` check the ids.

**Engine design**
- **Emoji scanner.** A new `text/emoji.rs` implements the emoji sequences of Unicode's emoji standard (UTS #51):
  - flags (two regional indicators);
  - keycaps (`[0-9#*] FE0F? 20E3`);
  - a base with FE0F, a skin-tone modifier or tags;
  - ZWJ chains.
  - FE0E, or a text-default character without FE0F, is not an emoji.
  - It is built on `unicode_properties::UnicodeEmoji`.
- **Itemisation.** `itemize` (`prepare.rs:155-205`) becomes sequence-aware: one font per sequence. For emoji presentation it tries the first colour fallback font that has the base, then the run font, then the other fallbacks.
  - Today ZWJ, VS16 and skin tones are itemised one character at a time; the neutral test at `:172` only covers whitespace, controls and marks. A sequence can therefore split across fonts and its ligature never forms. This fix helps monochrome emoji fonts too.
- **Emoji as images.** A sequence whose key (FE0F stripped) is in `emojiImages` becomes an inline object (item 1): 1em high, aspect ratio kept, bottom 0.1em below the baseline, alt = the sequence.
  - An unmatched ZWJ sequence tries its parts.
- **Colour fonts.** ttf-parser stays inside `fonts`.
  - Registration (`fonts/mod.rs:85-89`) accepts fonts with CBDT or sbix but no outlines, as colour-only fonts that are never embedded.
  - A new `fonts/color.rs` provides `LoadedFont::color_glyph(gid) -> Option<ColorGlyph>`:
    - `Svg(String)` for COLR v0 and v1. A `ttf_parser::colr::Painter` writes paths, solid, linear and radial fills, clips and transforms. Sweep gradients use their first stop; composite modes other than source-over draw as source-over.
    - `Png{bytes,x,y,w,h,ppem}` from `glyph_raster_image`, PNG only.
  - SVG-in-OpenType comes later: it needs selecting a glyph by id and svgz, and usvg is built without default features.
- **Glyph images.** After `prepare_all` (`pipeline.rs:201`), collect the colour glyphs the text uses.
  - Decode each through `ImageData::new(..).decoded()`, so they get the process-wide cache (`images/cache.rs`) and the SVG limits (`images/svg.rs:31-80`).
  - Add them with `ImageSet::add_glyph`; `pipeline::run` then takes `&mut ImageSet` (today `&ImageSet`, `pipeline.rs:188`).
  - The ActualText for each glyph comes from the shaper's `UnicodeMap` (`text/mod.rs:239-261`).
- **Painter.** A piece in a colour font emits one `Op::Image` per glyph, placed from its bounding box or bitmap metrics times the font scale.
  - A glyph without colour data is drawn as text when the font has outlines, otherwise not at all.
- **Text extraction.** New `Op::Span{actual_text}` and `Op::EndSpan` become `/Span <</ActualText…>> BDC…EMC`. pdf-writer 0.15 supports this: `begin_marked_content_with_properties().properties().actual_text()`.
  - They are written in `content_stream` (`pdf/mod.rs:847-925`).
  - Each span wraps the image plus an invisible (`3 Tr`) space glyph of the strut font, as wide as the object, for extractors that need a text object inside the span.
  - Type 3 fonts would give real selection but are heavier; consider them later.

**JavaScript side**
- **Tree builder.** The run code (`build.ts:733-802`) scans text with `new RegExp('\\p{RGI_Emoji}','gv')`.
  - The TypeScript target, ES2023, rejects the `v` flag in a literal; Node 22 runs it from the constructor.
  - It builds `url + code points + '.' + format`, or calls `builder`.
  - It resolves each one through `ImageTable`, so the remote and local image policies and their messages apply, and fills `document.emojiImages`.
- **Loading.** In `renderSettled` (`pipeline.ts:293-305`), emoji load through `loadImages`.
  - A failed emoji drops its entry and adds a warning instead of failing the render.
  - Add a process-wide LRU cache of emoji bytes keyed by URL.

**Limits and security**
- At most 4096 `emojiImages`. Each key must scan as exactly one sequence of at most 64 bytes.
- At most 4096 distinct colour glyphs per render.
- **COLR traversal.** ttf-parser stops cycles and limits depth to 64 (`colr.rs:1834-1856` in ttf-parser 0.25.1), but nothing limits fan-out.
  - The painter counts operations and unwinds past a budget, as `ShapingFailure` already does (`prepare.rs:229-241`, `:435-447`).
  - That glyph is then drawn without colour.
- Cap the SVG markup per glyph. Bitmap strikes are checked against `MAX_PIXELS` by the probe.
- **Fetches.** Emoji downloads use the `fetchImage` limits (`remote.ts:22-37`): no redirects, a timeout and a size limit.
- **Font size.** `MAX_FONT_BYTES` is 50 MiB (`policy.ts:15`). Noto Color Emoji CBDT (about 10 MB) fits. Apple's sbix font is not redistributable.

**Tests**
- Scanner table tests: ZWJ, skin tones, flags, keycaps, tags, VS15/VS16, a lone ZWJ, a lone modifier.
- Itemisation tests.
- **Fixtures.** Add subsets of Noto Color Emoji COLRv1 and CBDT (OFL, made with fontTools like `limits/glyph-doubling.ttf`) to `fixtures/fonts`; `tests/fixtures.rs:27-41` registers them automatically.
  - `emoji-colr.json` and `emoji-cbdt.json` use them.
  - `emoji-images.json` uses generated PNG/SVG test emoji, which avoids Twemoji's licence.
- A pdftotext check that the ActualText yields the emoji.
- `limits.rs`: a COLR fan-out bomb.
- **Fuzzing.**
  - The `register_font` text (`fuzz/src/lib.rs:136`) gains ZWJ, flag and keycap sequences, and makes the fuzzed font a fallback.
  - The `render_tree` generator gains emoji text and `emojiImages`.
  - Add both to the feature list at `:1257-1273`.

**Docs**
- GUIDE: the Fonts section, and the Limitations bullet at `GUIDE.md:840`.
- CLIENT_INTEGRATION: colour-only fonts.
- SCHEMA, CHANGELOG and ARCHITECTURE.

**New dependencies:** `unicode-properties =0.1.4` with its `emoji` feature, as a direct dependency. It is already in Cargo.lock through rustybuzz (Cargo.lock:953-971), so no new crate is added. Licence MIT/Apache-2.0, already allowed by `deny.toml`. `unicode-segmentation` is in the lockfile too, but only through `convert_case`, and is not needed.

**Effort:** L, 11–14 days:
- scanner and itemisation: 2 days;
- images (after item 1): 3–4 days;
- COLR: 4 days;
- CBDT/sbix: 2 days;
- ActualText: 1 day.

**Risks**
- COLRv1 fidelity: sweep gradients and blend modes.
- Bitmap emoji resolution.
- The planned PDF/A-1b forbids transparency, so emoji with alpha need an opaque fallback or must be refused there.

**Open decisions**
- **Where substitution happens.** Recommended: the engine, through `emojiImages`. The alternative is React splitting runs into image runs; that loses the text and needs a second segmentation for fonts.
- **Emoji URLs and the allowlist.** Recommended: checking them against `remoteImages` ("the same policy"). The alternative is trusting them like `Font.register` URLs (`font.ts:104`).
- **Failed fetch.** Recommended: a warning plus the font glyph.
- **Ship order.** Recommended: images, then COLR, then CBDT/sbix, then SVG-in-OpenType.
- **Text extraction.** Recommended: ActualText over Type 3 fonts.
- **Bundled emoji font.** Recommended: none (size); document Noto Color Emoji instead.

## 3. Background images

**Scope and API**
- `<ImageBackground src fit? style className>` takes View props plus `src`.
- `objectFit` and `objectPosition` place the image, and so do the Tailwind `object-*` classes (`supported.ts:378-405`).
- Schema: `backgroundImage: { src|data, fit (default 'fill'), objectPosition }`, on `box` nodes only.

**Recommendation: an engine field, not a lowering to a View with an absolute `zIndex: -1` `<Image>`.** The lowering has four problems:
- It cannot clip to the rounded padding box without putting `overflow: hidden` on the user's children.
- Percentage radii do not translate.
- An absolute child keeps the shift of its box's top (`paginate/mod.rs:558-562`). A box split across pages grows by `extra` (`boxes.rs:222-223`), and the image does not cover that.
- It adds extra debug outlines and error paths.

**Engine design**
- **Node field.** `Node` gets `background_image: Option<Box<BackgroundImage>>`, boxed like `bookmark` (`schema/mod.rs:227-242`). Putting it in `NodeKind::Box` would touch its 11 match sites.
- **Parsing.** In `de.rs`: add the `RawNode` field (606-627), the present list entry (653-670) and the box fields entry (589).
- **Validation.** Under `at("backgroundImage")`: `image_source` and `object_position`.
- **Loading.** `ImageSet::load` collects a `by_background` map.
- **Painting.** `ContentPainter` (`boxes.rs:39-46`) gains `background(node, rect, clip, ops)`, a no-op by default.
  - `BoxPainter` calls it between the background fill and the borders (`boxes.rs:275-278`).
  - It passes the padding box `border_box.inset(g.border)` and `inset_radii`, as the overflow clip does (280-285). A split box is then sliced like its background colour.
  - `Content` (`pipeline.rs:96-108`) forwards it to `ImageSet::paint_in`.
- **Repeated-content estimate** counts it.

**React**
- New host `ImageBackground`, in the FLOW set with the same children as View (`host-config.ts:40-50`, `:92-97`).
- `box()` (`build.ts:679`) adds `backgroundImage` through `imageTable.resolve`.
- Allow `objectFit`/`objectPosition` on it (`build.ts:258-264` rejects them today).

**Limits:** the same as `<Image>`.

**Tests**
- Validation.
- A paint unit test of op order and clip.
- Fixture `background-image.json`: rounded, bordered, split across pages.
- React tests.

**Docs**
- GUIDE.
- SCHEMA and CHANGELOG.

**New dependencies:** none.

**Effort:** M, about 3 days.

**Risks:** low.

**Open decisions**
- **Engine field or lowering.** Recommended: the engine field.
- **Placement box.** Recommended: the padding box (CSS).
- **Default fit.** Recommended: `fill`, matching `<Image>`.
- **`source`, `imageStyle`, `srcSet`.** Recommended: rejecting them with a message pointing to `src` and style.

## Dependencies and order

**How the items depend on each other and on the roadmap**
- Background images are independent. They share the `ImageSet::paint_in` refactor with inline images.
- Inline images build on superscript/subscript ("Now") for the run `verticalAlign` and per-segment baseline shift. Coordinate with "Text outlines in debug" so image runs get outlines.
- Emoji from images needs the inline-object machinery and the scanner. Font emoji needs the scanner and `Op::Span`.
- **Later roadmap items.**
  - Tagged PDF and PDF/UA (Later) can build on `alt` and the spans.
  - PDF/A-1b (Next) cannot have transparency, which affects alpha emoji.
  - Knuth–Plass line breaking (Next) treats objects as boxes, so it fits.
  - Floats (Next) are unrelated.

**Suggested commit sequence** (each commit green, with its own docs)
1. `images`: split `paint` into `paint_in`, with no behaviour change.
2. The `backgroundImage` field across schema, parsing, validation, `ImageSet` and `BoxPainter`, with generated docs, the fuzz generator and a fixture.
3. React `<ImageBackground>` and GUIDE.
4. PDF: `Op::Span`/`EndSpan` and invisible text.
5. Inline objects and `TextRun.image` (baseline and number alignment), with a fixture.
6. The remaining `verticalAlign` values and `x_height`, after superscript/subscript lands.
7. React `<Image>` inside `<Text>`.
8. `text/emoji.rs` and sequence-aware itemisation.
9. The `emojiImages` schema field, objects and a fixture.
10. `Font.registerEmojiSource`, its loader and warnings.
11. Colour-only font registration and `fonts/color.rs`.
12. COLR → SVG glyph images, the work budget and a fixture.
13. CBDT/sbix and a fixture.
14. Fuzz seeds, `limits.rs`, ROADMAP and CHANGELOG.

### Critical Files for Implementation
- crates/core/src/text/prepare.rs
- crates/core/src/text/paint.rs
- crates/core/src/images/mod.rs
- crates/core/src/paint/boxes.rs
- packages/react/src/tree/build.ts
