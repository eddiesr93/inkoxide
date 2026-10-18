# Testing

Every pull request runs `.github/workflows/ci.yml` on Linux: formatting,
clippy, the Rust tests, the schema checks, the lockstep versions of the
packages released together (`scripts/check-versions.py`), the Node
binding tests, the WebAssembly build and the TypeScript packages; and `.github/workflows/deny.yml`:
cargo-deny (`deny.toml`), which also runs every night for new advisories. The Rust tests check the PDFs with qpdf, poppler's
`pdftotext` and veraPDF, which the workflow installs. Locally a test
skips the checks of a tool that is not installed; with `CI` set, a
missing tool fails it. `scripts/ci.sh` runs the same checks and more
(visual regression, a fuzz smoke) on your machine:

```sh
scripts/ci.sh          # everything
scripts/ci.sh --quick  # everything but clippy, cargo-deny, the fuzz smoke and the visual diff
```

`.github/workflows/test-windows.yml` runs the Node binding tests on
Windows against the binary a release run built. It builds nothing and runs
only on demand, from the Actions tab, with the ID of that release run.

## Suites

- Unit tests next to each module (`validate/tests.rs`, `lru.rs`, …).
  A test of how fast something runs never sets a time limit, which a
  slow or busy runner would miss: `crates/core/src/test_timing.rs`
  compares an input with one four times larger (linear time, or a
  limit that stops both), or a hostile input with an ordinary one. The
  integration tests share it through `crates/core/tests/common`.
- `crates/core/tests/fixtures.rs`: every `fixtures/docs/*.json`
  validates, renders byte-identically twice (and with one thread), renders
  identically from MessagePack, and passes `qpdf --check`. Each also
  renders as PDF/A-1b, 2b and 3b, with the header, XMP part and `CIDSet`s
  of its level, except the fixtures that need transparency, which PDF/A-1b
  must reject with `PDFA_CONFLICT` at a given path (`NOT_PDF_A_1B`);
  `pdfa-1b` is a document 1b accepts, with fonts, forms, links,
  bookmarks and images; `pdfa-edge` holds the smallest page PDF/A
  allows, 3 × 3 points. With `VERAPDF` pointing at a veraPDF command
  (`scripts/ci.sh` sets it when `verapdf` is on `PATH`; CI, which sets
  `CI`, fails without it), every PDF/A output must pass veraPDF's
  profile for its level. The fixtures that
  carry files render only as PDF/A-3b, and the other levels reject them
  with `PDFA_CONFLICT` at the first file (`ONLY_PDF_A_3B`).
- `crates/core/tests/accessibility.rs`: `fixtures/docs/accessible.json`
  (from `scripts/fixtures/accessible.py`) as PDF/UA-1 and PDF/A-2a and
  3a passes veraPDF (and its PDF/UA-2 report is printed, not checked:
  inkoxide writes PDF 1.7); its structure tree, read back with qpdf, is
  in document order, with its lists and their numbering, table headers
  and spans, figures and where they are, links and what they say, fields
  and notes; images and links in text are elements in place among the
  text; a split table's repeated header is read once, headers and
  footers are pagination artifacts, and their annotations come in page
  order; reading order is document order whatever `zIndex` and floats
  draw first; tagged output is the same on any number of threads, and
  reads and looks as untagged (with poppler); a document without what
  PDF/UA requires fails with `UA_CONFLICT` at its path, and every
  fixture under PDF/UA either does or passes veraPDF.
- `crates/core/tests/output_hashes.rs`: every fixture's output, untagged
  and tagged, has the SHA-256 pinned in `output_hashes.sha256` (but those
  whose numbers depend on the platform's math library,
  `fixtures/wasm-parity.json`). A change on purpose is blessed with
  `INKOXIDE_BLESS=1 cargo test -p inkoxide --test output_hashes`.
- `crates/core/tests/encryption.rs`: every fixture, encrypted, renders
  deterministically and, with `qpdf` installed, passes `qpdf --check`
  with either password; `qpdf --json` of it, decrypted, equals that of
  the unencrypted render object for object, minus the `Encrypt`
  dictionary, so a string or stream left unencrypted fails it. Font
  widths are compared glyph by glyph instead: encrypted, a font lists
  the widths of all its glyphs, the same whatever the text. Also the
  permission flags qpdf reports, wrong passwords, a file with an owner
  password only, canary strings that must not appear in plain text, and
  errors that name the option but never the password.
- `crates/core/tests/attachments.rs`: files a PDF carries come out as
  they went in, through `pdfdetach` and qpdf (`--list-attachments`,
  `--show-attachment`), encrypted or not, with their `/ModDate` (the
  declared date, or else the creation date); the PDF/A-3b output, with
  Factur-X metadata, passes veraPDF, and the files change the file
  identifier. A document without files renders the same bytes with or
  without a store, and none of the attachments' objects. Also levels
  that refuse files, a file too large to read, and bad declarations,
  icons and XMP extensions at their paths.
- `crates/core/tests/limits.rs`: memory guards (page limit, content
  repeated on every page, glyphs from shaping, in SVG text too, copies
  of SVG text, the work budget of one `COLR` glyph, deepest accepted
  nesting on a small stack). Variable font instances render in time
  linear in their number, up to the 64 a document declares, and more are
  refused before any is made. SVG text at scale (thousands of texts on
  one point, of spans with their own paint, of text nodes) renders in
  linear time and bounded memory, measured with a counting allocator;
  its placeholders are bounded before they are written; and validation
  refuses what a render would. The unit tests of `images/svg_text` push
  reading and rewriting to hundreds of thousands of texts, spans and
  text nodes, those of `text/svg` the layout of tens of thousands of
  spans, with `textLength` and decorations. The budgets of a render's colour glyphs
  (how many, and the bytes of their images) are tested in
  `text/color.rs`, with smaller limits.
- `crates/core/tests/finite_numbers.rs`: every number in the PDF is
  finite (one `inf` or `NaN` operand makes a page invalid) and one
  readers parse (an integer within 32 bits or a real within ±10¹¹), with
  every property at its validation bound, percentages compounding past
  `f32`, and SVG numbers that overflow or are huge (`scale(1e30)`, text
  at 1e30 or sized 1e30); the SVG cases also pass `qpdf --check` without
  warnings.
- `crates/core/src/images/svg_text/survival.rs`: the probe survival
  suite. SVG text becomes placeholder paths ("probes") that `usvg`
  converts; these check that a probe comes out with the text's paint in
  every context `usvg` copies shapes (`use`, `symbol`, markers, patterns,
  masks, clip paths, `switch`, nested SVGs, groups, author CSS, with
  `!important` too). They
  depend on `usvg` internals: run them first after a `usvg` upgrade.
- `crates/core/src/schema/parity_tests.rs`: serde structs accept exactly
  the properties the JSON Schema declares, and validation enforces the
  schema's bounds, read from the schema: ids, font families, `pages`,
  grid placements, every bound `validate/rules` names as a constant,
  and the integer limits the schema states.
- `packages/inkoxide/test/schema-parity.test.js`: ajv with the schema and
  the engine accept the same percentages, `fr` sizes and integers.
- `crates/core/tests/threads.rs`: concurrent one-thread renders each run
  on their calling thread, at the same time.
- `crates/core/tests/clear_caches.rs`: after `clear_caches`, a render
  with something in every shared cache (a font subset, a JPEG, an SVG
  with text) gives the same bytes as before.
- `crates/core/tests/deferred_fonts.rs`: with `deferred_fonts`, a render
  that needs none of them renders the bytes it would with them
  registered, and one that does (a character only a deferred fallback
  has, or a run in a deferred font) fails with `NOT_LOADED` naming it. A
  missing font that is not deferred is still `FONT_NOT_REGISTERED`.
- `crates/core/tests/warnings.rs`: a text node or a run that hyphenates
  without a language logs one `tracing` warning, and text with a
  language, or that does not hyphenate, logs none.
- `crates/core/tests/builtin_hyphenation.rs`: built-in patterns given at
  run time, as the WebAssembly build gets them. `builtin_hyphenation_file`
  names the file of a tag, and `register_builtin` rejects a file that is
  not the language's. Without the `embedded-hyphenation` feature, a render
  that needs patterns it has not been given fails with `NOT_LOADED` naming
  them, and renders once they are registered; with the feature, it renders
  as it is. Both PDFs must match the digest in
  `builtin_hyphenation.sha256`. Romanian needs no file: it is
  hyphenated by rule, with or without the feature. `ci.yml` and
  `scripts/ci.sh` run it without default features as well.
- `scripts/check-schema.py`: fixtures conform to the JSON Schema.
- `scripts/visual-diff.py`: rasterises fixture PDFs and compares them with
  `fixtures/expected/*.png`; `scripts/update-expected.sh` regenerates the
  references for review.
- Integration fixtures modelled on client documents:
  `enrollment-request` (a school enrollment form: two-column field grid,
  checkboxes, a justified declaration and a signature line) and `report-100p-realistic` (98 pages: 40 tables
  of four kinds, bold and linked runs, header, and a footer with page
  numbers). The report is generated by
  `scripts/fixtures/report_realistic.py`; `scripts/check-generated.sh`
  regenerates it and fails if the committed JSON differs. It is also the input of
  `scripts/bench-headline.sh`.
- `fixtures/hyphenation/<fixture>.<lang>.tex`: hyphenation patterns
  registered only while rendering `<fixture>`.
- `fixtures/fonts/woff/`: Roboto Mono converted to WOFF and WOFF2 with
  fontTools (OFL, `fixtures/fonts/RobotoMono-OFL.txt`). The font tests
  check that each decodes to the same metrics and outlines as the `.ttf`.
- `fixtures/fonts/malformed/`: fuzz findings, minimised, that registration
  must reject. Each is a broken copy of a font above, under its licence.
- `fixtures/fonts/matching.json`: CSS font matching cases (family stacks,
  style and weight fallbacks), which the engine's `FontSet::select` and
  `FontRegistry.resolve` in `@inkoxide/react` both pass.
- `fixtures/fonts/limits/glyph-doubling.ttf`: a font made with fontTools
  for the shaping limit in `limits.rs`. Its GSUB substitutes `A` with
  `AA` in 14 lookups, so `A` shapes to 16 384 glyphs.
- `fixtures/fonts/EmojiTest-COLR.ttf`, `EmojiTest-SVG.ttf`, `EmojiTest-CBDT.ttf` and
  `EmojiTest-sbix.ttf`: small colour emoji fonts written from scratch by
  `scripts/fixtures/emoji_fonts.py` (outlines with `COLR` v0 and v1
  glyphs, outlines with an `SVG` table, and PNG bitmaps without outlines, the sbix ones in two strikes
  so that the largest is the one drawn), with ligatures for a ZWJ
  sequence, a flag, a keycap and a skin tone. The script also writes
  `fixtures/fonts/limits/colr-fanout.ttf`, a `COLR` glyph whose layers
  paint the next level 255 times, eight levels deep, for the colour glyph
  work budget in `limits.rs`, and `colr-fanout-empty.ttf`, the same with a
  last level that paints nothing, so that only counting the paint graph
  stops it.
- `fixtures/fonts/VarTest-glyf.ttf` and `VarTest-CFF2.otf`: variable
  fonts written from scratch by `scripts/fixtures/variable_fonts.py`
  (rectangle letters, `wght` and `wdth` axes, `gvar` or CFF2 `blend`,
  `HVAR`, `MVAR` and `avar` 1.0), with `fixtures/docs/variable-fonts.json`,
  and `VarTest-layout.ttf`, the glyf one with a `GSUB` whose `rvrn` varies
  (FeatureVariations) and a `GPOS` kerning with a variation device, which
  the shaping tests read at instances, and `VarTest-COLRv1.ttf`, a square
  whose `COLR` v1 colour fades with `wght` (`PaintVarSolid`);
  the script also writes, in `fixtures/fonts/limits/`, fonts registration
  must refuse: `var-avar2.ttf` (`avar` 2.0), `var-64-axes.ttf`,
  `var-no-gvar.ttf` (`fvar` and `glyf`, no `gvar`), `var-gvar-cut.ttf`
  (a `gvar` cut short) and `var-cff-and-cff2.otf` (a `CFF ` table beside
  `CFF2`). An instance at an axis the font lacks draws without it, with a
  warning.
- `fixtures/fonts/VertTest.ttf`: a font for vertical text written from
  scratch by `scripts/fixtures/vertical_font.py` (ideographs, kana and
  punctuation drawn as boxes and bars, with `vhea`, `vmtx`, `VORG` and the
  vertical forms of `、` `。` `「` `」` `ー` under `vert`), for
  `fixtures/docs/svg-text-vertical.json` and the layout tests of
  `text/svg.rs`. `fixtures/docs/svg-text-path.json` draws text on paths.
- `crates/core/src/text/vertical.rs`: the Unicode `Vertical_Orientation`
  table, generated by `scripts/gen-vertical-orientation.py` from a copy of
  `VerticalOrientation.txt` (not in the repository, so not regenerated by
  `scripts/check-generated.sh`).
- `packages/inkoxide/test`: the Node binding (`scripts/build-node.sh`
  first): rendering, error shapes and the libuv thread pool. They are the
  contract of `@inkoxide/wasm` too: `pnpm test` in `packages/wasm`
  (`scripts/build-wasm.sh` first, with the `wasm32-unknown-unknown`
  target) runs them against it, skipping the two about threads, and its own
  tests of the pool in `packages/wasm/test`; `pnpm test:threads` runs them
  against the module with threads (`scripts/build-wasm.sh --threads`, on
  the nightly it prints with `--toolchain`: `rustup toolchain install
  "$(scripts/build-wasm.sh --toolchain)" --profile minimal --component
  rust-src`), on a pool of 3.
- The WebAssembly build, in the `wasm` job of `ci.yml` and in
  `scripts/ci.sh`:
  - `scripts/wasm-size.mjs` fails when a module is over its download
    budget with brotli, and prints their sizes in the job summary;
  - `scripts/wasm-parity.mjs` renders every fixture document, and every
    fixture image on a page, with both builds and compares the bytes.
    Those in `fixtures/wasm-parity.json` may differ in the last digits of
    numbers (trigonometry in usvg, and for text on a path, uses another
    libm there) and in the file
    identifier, compared with their streams decompressed. It also registers
    the fuzz seed fonts, renders the attachments fixture as PDF/A-3b, and
    renders documents at the engine's limits from `limits.rs`, with both,
    and compares the outcomes. With `--threads` it compares the module
    with threads, without a pool and on pools of 3 and 8, with the
    one-thread module, and every render must give the same bytes;
  - `crates/wasm/src/threads.rs` and `alloc.rs` test natively the queue the
    pool's threads wait on, their blocks of stack and thread-locals, and the
    allocator's cache of blocks;
  - the engine's tests that need MessagePack are left out of a build
    without its feature (`--no-default-features`), which reads JSON only.
- `examples/browser`, in the `wasm` job of `ci.yml` only (`scripts/ci.sh`
  does not run it): `pnpm test` there, after `scripts/build-node.sh`,
  `scripts/build-wasm.sh`, `scripts/build-wasm.sh --threads` and
  `pnpm exec playwright install`. Playwright renders the page's invoice in a worker in
  Chromium, Firefox and WebKit, under a strict Content Security Policy
  and without cross-origin isolation, and compares the PDF with the one
  Node renders; a render's `signal` ends its worker. `tests/threads.spec.ts`
  does the same on the page served cross-origin isolated, where the
  engine renders on a pool of threads, and checks that a bug on a pool
  thread fails the render with `INTERNAL` and the next one renders.
- `packages/render/test/browser.test.tsx`: `@inkoxide/render/browser` on
  the calling thread renders the bytes the main entry renders. It needs
  `scripts/build-wasm.sh` first, and skips without it outside CI.
- `packages/tailwind/test`: `plugin.test.ts` builds a page with Vite and
  esbuild and checks the browser bundle has the table of its classes and
  none of Tailwind's compiler, the server bundle the compiler, and that a
  dev server reloads the page when a file brings a new class;
  `precompiled.test.ts` maps every class string of the mapper's tests, the
  documented subset, the examples, benchmarks and React and render tests
  through a table and through the compiler, and expects the same styles,
  errors and order; `scan.test.ts` the scanner, and which files it reads.
- `packages/*/test` (Vitest, `pnpm test`): the style value readers,
  Tailwind class mapping and `initTailwind` options, the React tree
  builder, `renderToBuffer`, `Font.register`, error messages, the
  export lists of `@inkoxide/style` and `@inkoxide/render`, and the
  examples and benchmark invoices, each run through a symlinked path.
- `scripts/check-generated.sh`: regenerates every committed generated
  file (`packages/schema/index.d.ts`, `docs/SCHEMA.md`,
  `docs/TAILWIND_SUPPORT.md`, `fuzz/render_json.dict`, the fixture
  documents and the emoji, variable and vertical test fonts that
  `scripts/fixtures` writes, the version `@inkoxide/react` reports and the
  Tailwind theme) and
  fails when one is stale; `ci.yml` and `scripts/ci.sh` run it. The
  images of `scripts/fixtures/images.py` and its document are the
  exception: it needs Pillow, whose PNG bytes change between versions.
  With `--python-only` it checks only the files python3 writes, which is
  what `scripts/ci.sh` does where pnpm is not installed.
- `cargo bench -p inkoxide`: criterion benchmarks (docs/BENCHMARKS.md).

## Fuzzing

Needs the nightly toolchain and cargo-fuzz:

```sh
rustup toolchain install "$(scripts/fuzz.sh --toolchain)"   # the dated nightly CI uses
cargo install cargo-fuzz
scripts/fuzz.sh 60                    # every target, 60 s each
scripts/fuzz.sh 3600                  # a one-hour run
scripts/fuzz.sh 600 render_svg_tree   # one target
```

The targets run without a sanitizer: the core crate has no unsafe code,
and AddressSanitizer's own memory and time would blur the hangs and
memory use they look for.

Targets (in `fuzz/`, outside the workspace, with the workspace's lints;
the `ci` workflow builds them and runs clippy on them):

- `render_json`: raw bytes, seeded with `fixtures/docs`, with a dictionary
  of schema keys and enum values (`fuzz/render_json.dict`, generated from
  the schema by `scripts/gen-fuzz-dict.py`). The harness registers every
  fixture font and image under its file name, as the fixture tests do, and
  a unit test checks that every seed validates, so each reaches layout.
- `render_tree`: a structure-aware generator of schema-shaped documents
  (edge values, long words, mixed scripts, big tables, form fields, page
  filters, document metadata, text at variable font instances, and boxes
  nested up to and past the parser's limit of 128 levels). A unit test checks that its valid
  documents reach the newer parts of the schema; extend its list with the
  generator.
- `render_svg`: raw bytes registered as an image and drawn twice; seeded
  with `fixtures/images`, with an SVG dictionary (`fuzz/render_svg.dict`).
- `render_svg_tree`: a structure-aware SVG generator that combines what
  `usvg` expands (references, markers, patterns, style sheets, entities,
  nested SVG images) with text (spans, position lists, fonts, anchors,
  baselines, `textLength`), so a timeout or an out-of-memory shows a way
  past the complexity limits. Images are drawn in a document that
  declares the fixture fonts, so SVG text is laid out.
- `register_font`: raw bytes registered as a font, as they are and with
  the WOFF and WOFF2 signatures over its first bytes, so every input reaches both
  decoders; each font that registers sets a line of text, and is the
  fallback of a line of emoji sequences (flags, keycaps, skin tones, ZWJ
  sequences), which a colour font draws from its colour tables. Seeded with
  `fixtures/fonts`, its WOFF files and its malformed and limit fonts.

All share one oracle: no panic; `render` succeeds only when `validate`
is clean and vice versa for validation errors, except `PDFA_CONFLICT`,
which only a PDF/A-1b render reports (by input length, a quarter of the
inputs render as plain PDF and a quarter at each PDF/A level); the
writer never refuses transparency at 1b, which would mean validation
missed it (for SVG, that its scan disagrees with the writer). What only
a render knows is not a finding either: PDF/A-2b refusing files, a file
`src` the store lacks (`ATTACHMENT_NOT_REGISTERED`), and XMP extensions
without PDF/A to go in. A quarter of the documents also render tagged
(an eighth as PDF/UA-1), and render or fail as untagged does, with the
same code; PDF/UA may refuse more only with `UA_CONFLICT`. Every
form script (`/JS`, read from the parsed object syntax, so a script
written as a hex string is checked and a document title that looks like
one is not) is a call of a standard form function (`AFNumber_Format`
and the like) with only integers, booleans and string literals as
arguments, and PDF/A output has no scripts and no `/AA` (`check_scripts`);
the output is a complete
PDF with no `inf` or `NaN` operand in its object syntax or uncompressed
content streams; a document that renders from JSON renders the same bytes
from MessagePack (the image and font targets included), and a document
rendered as plain PDF also renders encrypted exactly when it renders
plain, without its title in plain text (only the encrypted file's
object syntax is scanned for numbers). An input that runs
longer than 10 seconds is a finding (30 for `render_json` and `render_tree`:
a document at the engine's limits renders in about 2 seconds, and they
render it twice).

Findings land in `fuzz/artifacts/`. To inspect one:

```sh
cd fuzz
cargo run --release --bin repro -- tree artifacts/render_tree/crash-…   # prints the document, runs the oracle
cargo run --release --bin repro -- json artifacts/render_json/crash-…
cargo run --release --bin repro -- image artifacts/render_svg/crash-…
cargo run --release --bin repro -- svg artifacts/render_svg_tree/timeout-…  # prints the SVG, its time and verdict
cargo run --release --bin repro -- font artifacts/register_font/crash-…
cargo run --release --bin repro -- stats corpus/render_tree             # generator validity rate
```

`scripts/ci.sh` runs a 30-second smoke per target when that nightly and
cargo-fuzz are installed and skips it otherwise.

### Nightly

`.github/workflows/fuzz.yml` runs every target for an hour each night,
one job per target (the targets `scripts/fuzz.sh --list` prints), with
each corpus carried over from the night before (also on demand, from the
Actions tab, for up to 5400 seconds per target). A target that does not
build fails its job without sealing anything. Findings are security issues
(SECURITY.md) and a run's logs and artifacts are public, so a run with
findings fails without showing them: `scripts/fuzz-findings.sh seal`
encrypts `fuzz/artifacts` and `fuzz/logs` to the maintainers' public key,
`.github/fuzz-findings.pub.pem`, and the run uploads only that. A
maintainer opens them with the private key
(`~/.config/inkoxide/fuzz-findings.key.pem`, or `INKOXIDE_FUZZ_KEY`):

```sh
scripts/fuzz-findings.sh open RUN_ID   # into fuzz/findings/RUN_ID/fuzz-findings-TARGET/findings
```

To make a key pair (the private key never leaves your machine):

```sh
mkdir -p ~/.config/inkoxide
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out ~/.config/inkoxide/fuzz-findings.key.pem
openssl pkey -in ~/.config/inkoxide/fuzz-findings.key.pem -pubout -out .github/fuzz-findings.pub.pem
```

## Platform notes

Output is byte-identical between runs on one platform. Across platforms it
is expected to match too, but this is not guaranteed: rotation, SVG arcs and
the sRGB curve call `sin`/`cos`/`powf`, which come from the platform's libm
and may differ in the last bit. Poppler and FreeType builds may also
antialias differently from the Linux build that produced
`fixtures/expected/`. If the visual step fails while `cargo test` passes,
compare `target/visual/*-diff.png`; for antialiasing noise, rerun with a
looser threshold, e.g. `INKOXIDE_VISUAL_THRESHOLD=0.003 scripts/ci.sh`.
Regenerate the references on Linux only.
