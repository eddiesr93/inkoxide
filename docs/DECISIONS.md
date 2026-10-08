# Architecture decision log

Format: context → decision → consequences. Newest last. Anything not
obvious from the build brief goes here.

## ADR-001 — No hosted CI; checks run locally (M0)

**Context.** The brief specifies GitHub Actions for CI and releases. The
project owner asked that no Actions minutes be used for now.

**Decision.** No `.github/workflows`. `scripts/ci.sh` runs every check
(fmt, clippy `-D warnings`, tests, schema conformance, `qpdf --check`,
cargo-deny, visual diff) and must pass before pushing. The release
matrix (M5) will be written as a script/docs, not a workflow, until hosted
CI is approved.

**Consequences.** Linux-only verification for now (no macOS/Windows runs).
Re-introducing workflows later is mechanical: each workflow step maps to a
step of `scripts/ci.sh`.

## ADR-002 — JSON input in v1 (M0)

**Context.** JSON vs a binary encoding. **Decision.** JSON (serde_json) in
v1; MessagePack later behind `format`, sharing the same serde structs.
**Consequences.** Debuggable fixtures and error paths; parse cost is small
next to layout (measure in M6).

## ADR-003 — Nodes deserialise through one flat raw struct (M0)

**Context.** serde's internally tagged enums (`#[serde(tag = "type")]`)
buffer each node into an intermediate `Content` value. That copies nested
subtrees once per depth level and, worse, loses the error path below the
first tagged enum, so `serde_path_to_error` could only point at the node.

**Decision.** `RawNode` holds every field any node type can carry, with
`deny_unknown_fields`, and is converted (`try_from`) into the typed `Node`.
Fields that exist but are not valid for the node's type produce
`UNKNOWN_FIELD` at the field's own path.

**Consequences.** Precise paths (`pages[0].children[3].style.colour`), no
buffering. The schema-parity test checks that the union of node properties
in the JSON Schema equals the raw struct's fields.

## ADR-004 — `serde_path_to_error` dependency (M0)

**Context.** Errors need a JSON path (§4.8); serde alone gives line/column.
**Decision.** Add `serde_path_to_error` (dtolnay, ~500 lines, no
transitive deps beyond serde). **Consequences.** Works with any serde
deserializer, so MessagePack input gets paths too.

## ADR-005 — Structured codes from serde errors (M0)

**Context.** Serde errors are strings. **Decision.** Our own deserialisers
prefix messages with `[CODE]` or `[CODE@field]`; serde's built-in messages
(`unknown field`, `missing field`, `invalid type`, …) are classified by
prefix. All of it lives in `validate::classify`. **Consequences.** One
small, tested mapping; serde message wording changes would show up in the
validation tests.

## ADR-006 — Schema shape choices (M0)

Choices the brief left open, made when writing `document.v1.json`:

- **Edges are objects**, `{top, right, bottom, left}` with each side
  optional, for `padding`, `margin`, `borderWidth`, `borderColor` and
  page `margin`. One shape everywhere, no shorthand precedence rules.
- **Positioned offsets** live in `inset: {top, right, bottom, left}`
  rather than four top-level style keys, for the same reason.
- **`gap`** sets both gaps; `rowGap`/`columnGap` override it (CSS rule).
- **`flexDirection` defaults to `column`** (document flow is vertical;
  react-pdf does the same). Everything else follows CSS defaults.
- **Lengths** accept `"auto"` besides points and percentages wherever CSS
  does (`width`, `height`, `flexBasis`, `margin`, `inset`, min/max).
- **Grid placement** is `{start, end, span}` with 1-based lines, not CSS
  strings.
- **Header/footer placement**: laid out at the full page width; header at
  y = 0, footer bottom-aligned to the page. Insets come from their own
  padding; margins must leave room.
- **Placeholder runs** carry `placeholder` *instead of* `text` (exactly one
  of the two).
- **Text-only style properties on other nodes are errors**
  (`STYLE_NOT_APPLICABLE`), consistent with "unknown properties are
  errors": silent ignoring hides client bugs.
- **`break` is only valid in the flow**, not inside headers, footers or
  table cells, where it would have no meaning.
- **Image intrinsic size** is 96 dpi (1 px = 0.75 pt), matching CSS, and
  `fit` defaults to `fill` like CSS `object-fit`.
- **Table rows** carry `minHeight` and `backgroundColor` directly; cell
  styling goes on cells (`style`, including `verticalAlign`).

## ADR-007 — No creation date unless asked (M0)

**Context.** Determinism (§1.2) vs a useful Info dictionary.
**Decision.** `/CreationDate` is written only when `options.creationDate`
or `meta.creationDate` is set; there is no implicit "now", and the core
never reads the clock. No `/ID` is written. **Consequences.** Output is
byte-identical by default; callers that want a date pass one.

## ADR-008 — Top-left display-list coordinates (M0)

**Decision.** Layout and the display list use a top-left origin with y down.
Each content stream starts with one `1 0 0 -1 0 H cm` flip; text uses a
flipped text matrix and images a flipped image matrix. All numbers are
rounded to 1/1000 pt before writing (≤ 3 decimals; pdf-writer then prints
the shortest round-trip form).

## ADR-009 — Taffy through its high-level tree, rounding off (M1)

**Context.** Taffy can run on its own `TaffyTree` or on a custom tree that
implements its traits. **Decision.** Use `TaffyTree<NodeIdx>` built from our
BFS arena (back to front, so children exist before parents), with
`disable_rounding()` — PDF coordinates are fractional and pixel snapping
would only add error. Leaves carry their arena index as context; the
measure closure dispatches through our `LeafMeasure` trait. Absolute
geometry is copied back into the arena and the Taffy tree is dropped.
**Consequences.** ~100 lines of glue behind `LayoutEngine`. If profiling
(M6) shows tree construction matters, implement Taffy's traits directly on
the arena without changing callers.

## ADR-010 — Opacity via ExtGState, not transparency groups (M1)

**Context.** CSS `opacity` composites a subtree as a group. PDF's `ca/CA`
apply per painted object, so overlapping children of a translucent box
show through each other; true group opacity needs a Form XObject with a
transparency `/Group`. **Decision.** v1 uses `ExtGState` alpha. The painter
tracks cumulative opacity (PDF `gs` sets an absolute value, it does not
multiply) and one shared `ExtGState` exists per distinct value (quantised
to 1/1000). Colour alpha folds into the same mechanism. **Consequences.**
Correct for the common case (a translucent box with non-overlapping
content). Group opacity becomes a form XObject later if needed (M7).

## ADR-011 — Border model (M1)

**Decision.** Borders are painted inside the border box. With one colour
and solid style, the border is a single even-odd fill of the ring between
the outer (border-box) and inner (padding-box) rounded rectangles; inner
radii are the outer radii minus the adjacent widths. With several colours,
each side is a mitred trapezoid clipped by that ring, so corners and radii
come out right without per-corner geometry. Dashed/dotted borders stroke
the middle of each side (dash `3w/3w`, dots as zero-length dashes with
round caps); a uniform dashed border with radius strokes one rounded path.
Default border colour is black (CSS `currentColor` has no meaning here).
**Consequences.** Dashed borders with mixed widths and radii are
approximate at the corners; acceptable for v1.

## ADR-012 — M1 overflow: clip with a warning (M1)

**Decision.** Until pagination (M3), each page template renders on one page
and content taller than the content box is clipped to it, with
`tracing::warn!`. Replaced by real pagination in M3.
