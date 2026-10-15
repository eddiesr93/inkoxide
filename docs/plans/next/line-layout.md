# Plan: floats and optimal line breaking

Part of [Plans for "Next"](README.md). Draft for review; line references point to `f038fed`.

**Taffy's floats.** The pinned Taffy (=0.14.0) does have floats, but the engine turns that support off (`Cargo.toml:22` disables default features). This plan does not use it, for three reasons:
- The `TaffyTree` measure callback never receives the block context (`taffy-0.14.0/src/tree/taffy_tree.rs:316-323`). Wrapping text would mean rewriting `layout_subtree` (`crates/core/src/layout/mod.rs:374-534`) on Taffy's low-level tree API.
- Taffy floats only work inside `Display::Block` containers. Those collapse margins and ignore `gap`, unlike every other box in the engine.
- `FloatContext::find_content_slot` answers for a single y and always returns `height: INFINITY` (`taffy-0.14.0/src/compute/float.rs:587-637`). It cannot answer "what width is free over this line's height".

## Floats

**Scope and API**
- Style properties:
  - `float: "left" | "right" | "none"`, on any child of a column box: Box, Text, Image, Field.
  - `clear: "left" | "right" | "both" | "none"`.
  - Names and values match the migration guide.
- `shapeOutside` is **out of scope for v1**. MIGRATING.md keeps listing it. The design leaves room for it: each float contributes an inset per y-range, so circle, ellipse, polygon and inset shapes can plug in later.
- Behaviour:
  - A float is shrink-to-fit and sits at the start or end edge, following CSS 2.2 §9.5.1 for rectangles: its top is no higher than the current flow position or any earlier float, and it moves down when it does not fit beside the floats already there.
  - Text siblings wrap line by line.
  - Box siblings (every View is a flex container, which is an independent formatting context in CSS) are narrowed beside floats, or moved below them when their min-content does not fit.
  - `clear` puts the border box below the float's margin box.
  - The container's height includes its floats.
  - The API MIGRATING.md targets differs here: it lets box siblings and the container overlap floats. MIGRATING.md should say so.

**Engine design**
1. **Placement: new `crates/core/src/layout/float.rs`, about 200 lines, pure.**
   - `FloatContext` with:
     - `place(size, min_y, side, clear) -> (x, y)`
     - `clear_y(clear)`
     - `band(y, h) -> (left_inset, right_inset)`, a range query
     - `next_change(y)`
   - Floats whose bottom is above the cursor are dropped from the active list.
   - Also define `LineBoxes { width, bands: Arc<[Band]> }` here, with `Band { top, bottom, left, right }` relative to a paragraph's content box. Text already imports from layout (`text/mod.rs:26`), so the dependency direction stays the same.
2. **Float containers laid out like tables.**
   - A float container is any Root, Box or Cell with an in-flow floated child. Mark them in a bitset built by `Tree::build` (`layout/mod.rs:187-232`).
   - `layout_subtree` treats a float container as a Taffy leaf, the same way it treats tables (`mod.rs:394-399`), the root included. The measure closure (`mod.rs:478-484`) sends it to a new `LayoutCtx::flow(tree, idx, space)`, cached per (node, `SpaceKey`) like `table()` (`mod.rs:558-571`).
   - New `crates/core/src/layout/flow.rs`:
     - Stacks in-flow children with flex-style margins (no collapsing), `rowGap`, `alignItems`/`alignSelf` and auto margins.
     - Places floats with `FloatContext`, applies `clear`, and positions absolutely positioned children from the padding box.
     - Text children: resolves their own box, builds `LineBoxes` from the active floats, and measures through a new `LeafMeasure::measure_lines(node, &LineBoxes)`. That method extends the trait at `mod.rs:245-253` with a default; `Leaves` (`pipeline.rs:68-88`), `TextCache` and `PageText` forward it.
     - Other children go through `layout_subtree` with a new `RootMode::Item { width, parent_h }` (next to `RootMode` at `mod.rs:299-305`). Percentage heights resolve as `to_taffy_on_page` does (`style.rs:38-49`).
     - Intrinsic sizes: min-content is the widest child or float, min-content. Max-content is the widest in-flow child plus the floats beside it, compared with the floats' row width; the row width resets at each clear.
   - `compute_tree` gains a second branch next to the table loop (`mod.rs:679-717`) that writes the children's geometry. It also stores `Tree.wraps: BTreeMap<NodeIdx, Arc<LineBoxes>>` and `Tree.float_groups` (each float's group bottom: overlapping floats plus the lines they wrap), next to `tables` (`mod.rs:184`).
   - The fast cell path must turn floats away: add `float`/`clear` to `plain_item` (`fast.rs:64-85`).
3. **Text.**
   - `break_lines(para, &LineBoxes)` replaces `max_width` (`lines.rs:12`). The first-line limit (`lines.rs:46-50`) becomes the line box at the current y, over the paragraph's tallest line height. That is exact for single-size text and errs narrow for mixed sizes.
   - Heights are computed per line while breaking (today `metrics` runs afterwards, `lines.rs:115-139`).
   - When nothing fits beside floats, the line moves down to `next_change(y)` (CSS §9.5). It falls back to breaking anywhere (`lines.rs:86-111`) only once no floats are left.
   - `Line.top` already allows gaps.
   - `LineLayout` gains `boxes: Vec<(x, avail)>`, empty when there are no floats.
   - The ellipsis limit (`lines.rs:146-152`) and the painter's indent, justification and alignment (`paint.rs:113-134`, `204-212`) use the line box instead of `cw`.
   - The cache key becomes `(width bits, bands)`, compared by value (`text/mod.rs:356`, `403-423`).
   - Lookups take the boxes:
     - `TextCache::paragraph` (`text/mod.rs:639-649`)
     - `PageText::paragraph` (`text/mod.rs:703-713`)
     - `LineSource::lines` (`paginate/mod.rs:183-192`)
     - `numbered_text` (`pipeline.rs:465`)
     - `ContentCtx` (`paint/boxes.rs:23-36`, built at `294-301`), from `tree.wraps`
4. **Pagination.** Today pagination never lays anything out again (`paginate/mod.rs:3-11`). `place_bands` is unsuitable: it starts every child of a band at the band top (`677-681`), so stacked paragraphs would overlap. Instead:
   - Floats are atomic (`atomic`, `386-405`): moved whole, and sliced with the existing warning when taller than a page. Splitting a float could leave text overlapping it; slicing only shifts text down.
   - `need()` (`529-534`) for a float covers its whole group, so the float and the lines beside it move to the next page together.
   - In `split_text` (`800-837`), the first chunk's orphans become at least the number of wrapped lines, so the widow replay (`821-827`) cannot carry wrapped lines away.
   - Accepted artifact: in other cases, lines carried to the next page keep their inset.
5. **Paint.** Floats paint after their in-flow siblings with the same `zIndex`, so a paragraph's background does not cover them (`paint_order`/`has_z`, `paint/boxes.rs:715-724`).
6. **Validation** (`validate/rules.rs`, using the parent-aware `children()` at `348-375`):
   - `float` under a row, grid, wrapping or reversed box, on a table cell (list at `1066-1100`), with `position: absolute`, on a `pageBreak`, or with `break.before/after: page` (`447-455`): error.
   - Forced breaks inside a float: `Ctx::Whole` (`457-467`).
   - `flexGrow` or `flexBasis` on a float container's children, and `justifyContent`/`alignContent` on the container: `STYLE_NOT_APPLICABLE`.

**Limits and security**
- At most 1,000 floated children per box: `OUT_OF_RANGE`, documented in `docs/API.md` next to the `gridTemplateAreas` cap.
- Each line's move-down strictly increases y through a finite list of float bottoms, so it ends; say so in a comment and fuzz it.
- Bands only exist at definite widths. Max-content and min-content measurement uses the plain path, so there are no infinite widths.
- Fuzz: add `float`/`clear` to `BOX_PROPS` and `prop()` (`fuzz/src/lib.rs:528-678`), and bias the generator towards floats in column parents with text siblings. Add a `debug_assert!` that no wrapped line box intersects a float.

**Tests**
- `float.rs` unit tests: stacking, wrapping to a new row, clear, zero-size floats.
- `layout/tests.rs`:
  - wrap, narrowing, move-below, clear, container height, root and cell containers
  - a **differential test**: the flow algorithm on random column boxes without floats must match Taffy, like `simple_cell_fast_path_matches_taffy` (`layout/tests.rs:193`)
- `text/tests.rs`: banded greedy, move-down, ellipsis, RTL, indent.
- `paginate/tests.rs`: float group pushed, sliced tall float, orphans.
- Fixtures `fixtures/docs/floats.json` and `floats-pagination.json` with PNG references. They are also benchmarked automatically (`benches/render.rs:35-55`).

**Docs.** Regenerate with `scripts/check-generated.sh:22-38`. Then update:
- `docs/SCHEMA.md` and `packages/schema/index.d.ts` (both generated)
- `docs/API.md` (limit)
- `docs/TAILWIND_SUPPORT.md` (generated)
- `packages/render/GUIDE.md` (Styling, Limitations)
- MIGRATING.md: move float and clear out of "Not available", note the containment difference
- `docs/ARCHITECTURE.md` (float containers sit next to tables)
- `ROADMAP.md`, both CHANGELOGs

**TS, React, Tailwind**
- `packages/style`: `ENUMS` (`reader.ts:36-75`) and `FlatStyle` types.
- React: `SIMPLE_KEYS` (`react/src/tree/style.ts:121-153`).
- Tailwind:
  - `float` and `clear` cases in `parse.ts` (beside `overflow`, `197-207`) and families in `supported.ts`.
  - `float-left/right/none` and `clear-left/right/both/none` are supported.
  - `float-start/end` and `clear-start/end` go in `UNSUPPORTED` (`supported.ts:683`), since boxes have no direction.

**New dependencies:** none.

**Effort: L, about 15–18 days.**
- Risks:
  - The flow algorithm repeats column-flex rules; the differential test is the mitigation.
  - Inset lines after a page break.
  - Cache growth from band keys.
  - Interaction with nested float containers inside tables.

**Open decisions**
1. Own placement module or `taffy::FloatContext` behind the `float_layout` feature → **own module**: it needs range queries, and Taffy features stay off.
2. Boxes beside a float: narrowed, overlapping (the MIGRATING.md API), or passing the float context into nested column boxes → **narrowed in v1**. Passing the context down is a follow-up.
3. Flex grow/justify in float containers → **reject in v1**, possibly support later with a second pass.
4. Floats taller than a page → **slice, with a warning**.
5. `shapeOutside` → **follow-up (S–M)**.

## Optimal line breaking (Knuth–Plass)

**Scope and API**
- Engine style `textWrap: "wrap" | "pretty"`, text nodes only, added to `text_props_set` (`schema/mod.rs:876-895`). Default `"wrap"` keeps first-fit and today's output, byte for byte.
- In React it cascades: add it to `TEXT_INHERITED` (`packages/style/src/types.ts:175`) and emit it in `textNodeStyle` (`react/src/tree/style.ts:386`). `textWrap: 'pretty'` on a `Page` then applies to the whole page.
- Tailwind: `text-pretty` → `pretty`, `text-wrap` → `wrap`, `text-nowrap` → `whiteSpace: 'nowrap'`. `text-balance` is unsupported for now, and the `"balance"` value is reserved.
- The name follows CSS `text-wrap: pretty`, which WebKit implements as whole-paragraph optimisation.

**Engine design**
1. Refactor `break_lines` (`lines.rs:12-167`) into per-chunk breaking between mandatory breaks, plus the shared finishing steps (metrics, `maxLines`/ellipsis, tops). Each chunk starts at a known line index and y, so floats and indent widths stay exact.
2. **New `crates/core/src/text/optimal.rs`.** It works directly on `Prepared` breaks (`prepare.rs:485-494`) with prefix sums of glyph advances and of space advances; no node list is built.
   - Glue: the trailing spaces at each `Allowed` break (as `visible_end` does, `text/mod.rs:128-134`).
   - Penalties:
     - `Hyphen` breaks: 50, flagged.
     - `Allowed` breaks right after `-` or U+2010: 50, flagged.
     - Other breaks: 0.
     - Chunk end: forced, with infinite stretch (the painter never justifies forced lines, `paint.rs:130`).
   - Stretch:
     - `justify`: half of each space's advance.
     - Other alignments: a ragged stretch of 2 em of the largest run size.
   - Badness is 100·r³. Demerits use TeX constants (line penalty 10, double-hyphen 10,000, final-hyphen 5,000, adjacent fitness classes 10,000), with four fitness classes, in f64, ties broken by lowest position.
   - Active nodes are keyed by (position, line, fitness) and store their exact y. Each line's width comes from `LineBoxes` at that y, minus the indent on line 0.
   - Once widths stop varying (past the indent and the last band), line numbers merge, as in TeX's easy-line rule.
   - Two passes, with tolerance r ≤ 1.26 and then 10; after that the chunk falls back to greedy. Unbreakable overflow, CJK text under `justify` (it has no stretch), and `pre`/`nowrap` all use greedy.
   - Looseness is out of scope.
3. `ParaStyle.text_wrap` (`text/mod.rs:186-226`). `min_content` is unchanged (`lines.rs:171-192`). A chunk that fits on one line skips the optimiser, so max-content costs nothing.
4. Optional shrink: in `pretty`, a justified line may shrink spaces by up to a third. The painter must accept a negative `justify_extra` (`paint.rs:129-134`), and the ellipsis check must allow for shrink (`lines.rs:148`).

**Limits and security**
- Deterministic per-call caps with a greedy fallback:
  - at most 20,000 break opportunities per chunk
  - at most 512 active nodes
  - at most 2^21 candidate evaluations per `break_lines`
- No render-wide counter: `PageText` breaks lines in parallel paint jobs (`text/mod.rs:685-742`), so a shared counter would make output depend on thread timing, unlike the shaping budget (`prepare.rs:219-223`).
- Document the caps in `docs/API.md` and `docs/SCHEMA.md`.
- Fuzz: add `textWrap` to `TEXT_ONLY` (`fuzz/src/lib.rs:514-526`), including long single-chunk paragraphs.

**Tests**
- `text/tests.rs`:
  - matches greedy when greedy is already optimal
  - avoids a known bad greedy paragraph
  - hyphen demerits
  - greedy fallback for an overlong word, CJK justify and the caps
  - indent and bands
  - `maxLines`/ellipsis
  - every line fits its box
- Fixture `fixtures/docs/text-pretty.json`, justified and ragged, with PNG references; it becomes a bench automatically.
- A Node test that `text-pretty` is inherited.

**Docs.** SCHEMA.md, `index.d.ts` and TAILWIND_SUPPORT.md (generated), GUIDE.md (Text section), API.md (caps), BENCHMARKS.md (pretty compared with wrap), ARCHITECTURE.md (text pipeline diagram), and MIGRATING.md: remove the "first-fit" bullet (`MIGRATING.md:36`), add a row "optimal breaking → `textWrap: 'pretty'`, opt in", and mention `hyphenationPenalty`. Also both CHANGELOGs and ROADMAP.md.

**New dependencies:** none.

**Effort: M, about 7–9 days.**
- Risks:
  - Tuning ragged text.
  - Cost on large paragraphs: target at most 25 µs for a 100-word paragraph.
  - Shrink changes the painter's assumption that a line never exceeds its box.

**Open decisions**
1. Style property, render option, or `lineBreak` name → **`textWrap` style**.
2. Default `pretty` in `@inkoxide/render`, for parity with MIGRATING.md's API → **no**: keep the default, document the opt-in.
3. Shrinking spaces → **yes, in pretty + justify only**.
4. `balance` (bisect the narrowest width that keeps the line count) → **defer**, value reserved.
5. The `hyphenationPenalty` Text prop from MIGRATING.md's API → **defer** until asked.
6. Expose tolerance or penalties → **no**.

## Dependencies and order

- Both items share per-line boxes, so build those first. Optimal breaking is smaller and checks the shared change at lower risk; floats then only add bands.
- Interactions with other roadmap items:
  - "Text outlines in `debug`" (Now) paints per line: land it first, then floats update it for line boxes.
  - `verticalAlign: super/sub` (Now) changes `metrics` (`lines.rs:256-300`). The per-line height precompute should include the baseline shift, so coordinate or land it first.
  - Inline images in `Text` should be glyph-like items so the optimiser stays unchanged.
  - The later WebAssembly build is unaffected (pure Rust, no Taffy features turned on).

**Commit sequence (each green)**
1. text: `LineBoxes` in `layout/float.rs` (type only); `break_lines` per chunk; cache key, `LineSource`, paint and `PageText` take boxes. No change in output: fixtures stay byte-identical.
2. text: greedy over bands (move-down, line boxes, per-line metrics, ellipsis per box); painter uses line boxes; unit tests with made-up bands.
3. text: `optimal.rs` with caps and fallback; `#[serde(skip)] text_wrap`; unit tests.
4. paint: shrink for justified pretty lines.
5. `textWrap`: JSON schema, serde, validation, TS, React, Tailwind, fuzz, fixture and PNGs, docs, CHANGELOGs.
6. layout: `FloatContext` placement with unit tests.
7. layout: `flow.rs` float containers, `RootMode::Item`, `Tree.wraps`/`float_groups`, fast-path guard, `#[serde(skip)] float`/`clear`; differential and layout tests.
8. paint: float paint order; `ContentCtx` carries the line boxes.
9. paginate: floats atomic, group need, orphans over wrapped lines; tests.
10. `float`/`clear`: JSON schema, serde, validation (including the 1,000 limit), TS, React, Tailwind, fuzz, fixtures and PNGs, docs, MIGRATING.md, ROADMAP.md, CHANGELOGs.

### Critical files for implementation
- crates/core/src/text/lines.rs
- crates/core/src/layout/mod.rs
- crates/core/src/text/mod.rs
- crates/core/src/paginate/mod.rs
- crates/core/src/text/paint.rs
