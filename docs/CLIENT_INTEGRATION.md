# Client integration (Node.js)

How to use the engine directly from a Node application, with documents
built as JSON. To write documents as React components, use
[`@inkoxide/render`](../packages/render/README.md) instead; it does all of
this for you. [API.md](API.md) has the full API and
[EXAMPLES.md](../packages/schema/EXAMPLES.md) has complete documents. In a browser,
[`@inkoxide/wasm`](../packages/wasm/README.md) has the same functions, on one
thread: see [API.md](API.md#webassembly-inkoxidewasm).

## Install

```sh
npm install inkoxide @inkoxide/schema
```

- **The native addon is per platform.** npm installs the prebuilt binary
  for the current platform (see the platform table in the
  [README](../README.md#platforms)). Install on the platform you deploy
  to, or with `--os`/`--cpu` when building a container image elsewhere.
- **Other platforms.** Build the addon from a checkout with
  `scripts/build-node.sh`, which writes
  `packages/inkoxide/inkoxide.<platform>.node`.
- **ES modules only.** Import it with `import { render } from 'inkoxide'`;
  it needs Node.js 22 or later.

Log `version()` once at startup. It returns
`{ engine, schema, gitSha }`, so a bug report can name the exact build.

## Fonts: register once, at startup

```js
import { readFileSync } from 'node:fs';
import { registerFont } from 'inkoxide';

for (const id of ['Inter-Regular', 'Inter-Bold']) {
  registerFont(id, readFileSync(`fonts/${id}.ttf`)); // TrueType, CFF OpenType, WOFF or WOFF2
}
```

- **Process-wide registry.** The font registry belongs to the process
  and is shared by every render. Register each font once, when the app
  starts, not once per request: registration parses the font, and each
  font is compressed only once and then cached.
- **The id is the link to the document.** Use the same string in the
  document's `fonts[].id` and `runs[].font`. If a document names a font
  that is not registered, you get `UNKNOWN_FONT`, with the path of the
  first run that uses it.
- **Changing fonts is safe mid-render.** Each render takes a snapshot of
  the registry when it starts, so `registerFont` or `unregisterFont`
  never affects a render already in progress.
- **Unsupported fonts.** These are rejected with `FONT_ERROR`: variable
  fonts and CFF2 fonts (register static instances instead), font
  collections (`.ttc`; register each font as its own file), fonts with
  neither TrueType nor embeddable CFF outlines, fonts without a
  Unicode `cmap`, fonts whose `GSUB` or `GPOS` lookups point to
  missing or malformed coverage tables, and CFF fonts with a number out
  of range in a DICT.
- **Colour fonts.** Glyphs in `COLR` (versions 0 and 1) and PNG bitmaps
  in `CBDT` or `sbix` are drawn in colour, as images that copy as their
  text. A font with colour bitmaps and no outlines (Noto Color Emoji's
  CBDT build) registers as a colour-only font: it is never embedded, its
  glyphs without colour are not drawn (they still copy as their text),
  and a form field cannot use it.
  List an emoji font in the document's `fallbackFonts`: emoji go to the
  first colour fallback that has them. `SVG` glyph tables are not read.

## Images: per render, through `options.images`

```js
const pdf = await render(doc, {
  images: { logo: readFileSync('logo.jpg'), chart: readFileSync('chart.svg') },
});
// in the document: { "type": "image", "src": "logo", "fit": "contain", ... }
```

- **Ids.** The keys of `options.images` are the `src` ids in the
  document. A `src` with no matching key is `UNKNOWN_IMAGE`.
- **Prefer JPEG for photos.** JPEG bytes go into the PDF unchanged:
  nothing is decoded, so the image is fast and stays small. PNG is
  decoded and recompressed when the engine has not seen the same bytes lately, alpha included. SVG is drawn
  as vectors, which suits logos and charts.
- **Avoid inline `data` for large images.** A node's inline `data` field
  (base64) also works, but it inflates the document JSON. Use
  `options.images` for anything large.
- **Decoded images are cached between renders.** The engine keeps up to
  64 MB of decoded images in memory for the whole process, found by the
  content of the bytes, and drops the least recently used first. The same
  logo is decoded once, not on every render. Keeping the bytes themselves
  in memory is up to you: pass the same `Buffer` each time.

## Documents

- **Types.** Build documents with the types from `@inkoxide/schema`:
  `import type { Document } from '@inkoxide/schema'`. Pass them to
  `render` as `JSON.stringify(doc)` or as a `Uint8Array` of UTF-8 JSON.
- **Large documents.** For many pages, MessagePack is faster to parse:
  pass the encoded bytes with `{ format: 'msgpack' }`. Encode them with
  any library that writes plain maps, arrays and strings.
- **Validating before rendering.** For example, in a form or an editor:
  - `validate(doc)` returns the problems at once (at most 100), as
    `[{ code, path, message }]`, without rendering.
  - Client-side, ajv with the schema from `@inkoxide/schema` reports the
    same paths through `enginePath(error)`
    ([@inkoxide/schema](../packages/schema/README.md)).

## Errors

`render` rejects with an `InkoxideError`, and `renderSync` throws one.
`validate` returns problems instead of throwing; it throws only for
`INVALID_ARGUMENT` or `INTERNAL`. An `InkoxideError` has the following
fields:

| Field | Value |
|---|---|
| `code` | a stable string; see the table in [API.md](API.md#errors) |
| `path` | the document location, like `pages[0].children[3].runs[1].font`, or `''` |
| `message` | a human-readable description |

```js
import { render, InkoxideError } from 'inkoxide';

try {
  return await render(doc, options);
} catch (e) {
  if (!(e instanceof InkoxideError)) throw e;
  switch (e.code) {
    case 'INVALID_ARGUMENT': // a bug in the calling code: wrong option type or range
    case 'INTERNAL':         // a bug in the engine; the process is fine. Report it with version()
      throw e;
    default:                 // the document or its fonts/images: show e.path and e.message to the author
      return { error: { code: e.code, path: e.path, message: e.message } };
  }
}
```

- `render` reports only the first validation problem. Call `validate`
  when you want all of them.
- New codes may be added in minor versions, so always handle the
  `default` case.
- An engine panic never takes the process down: it surfaces as
  `INTERNAL`, or as `FONT_ERROR` naming the font when the shaper panicked
  on it.

## Threading

- **`render` is already asynchronous.** It runs on the libuv thread pool
  and never blocks the event loop. You do not need `piscina` or worker
  threads. `renderSync` is for CLIs and tests only.
- **Each render is parallel inside.** By default a render uses all cores
  for shaping, painting and compression, through one pool shared by all
  renders in the process. Concurrent renders share those cores rather
  than multiplying threads.
- **How many renders run at once.** The libuv pool size limits this:
  4 by default, or `UV_THREADPOOL_SIZE`. Renders beyond that wait in
  line.
- **Many small documents.** For a server producing invoices or forms,
  pass `{ threads: 1 }` and raise `UV_THREADPOOL_SIZE` to about the core
  count. Each render then runs entirely on its own libuv thread, so
  concurrent renders use one core each with no coordination overhead.
- **Few large documents.** For a 500-page export, keep the default.
- **Memory.** Peak memory grows with the document: about 210 MB for a
  435-page table (see [BENCHMARKS.md](BENCHMARKS.md)). The engine
  refuses documents that would exceed its page limit (100,000 pages), or whose
  headers, footers and fixed elements would add more than about 64 MB
  repeated on every page, with `LAYOUT_ERROR`. It also refuses, with
  `FONT_ERROR`, a font whose layout tables shape the text to millions of
  glyphs more than it has bytes.

## Determinism

The output depends only on the document, the font, image and pattern
bytes, and the options. It does not depend on the thread count, the
machine load, or JSON versus MessagePack input. The creation date is the
only time-dependent input:

- With `creationDate: '2026-01-01T00:00:00Z'` (an option) or
  `meta.creationDate` (in the document), the same input gives
  byte-identical PDFs. The option wins when both are set.
- With neither, no date is written, which is also deterministic.

Use a fixed date in snapshot tests, and for caching or deduplicating
PDFs by hash.

## Hyphenation

A text node hyphenates when its style has `hyphenate: true`; a run with
its own `hyphenate` overrides that for its words. The language is the
node's `lang`, or else `meta.lang`. The engine includes patterns
for about 50 languages, English included. For a language without
built-in patterns, or to replace the built-in ones, load standard TeX
patterns yourself, once, at startup:

```js
import { registerHyphenationPatterns } from 'inkoxide';
registerHyphenationPatterns('en', readFileSync('hyph-en-us.tex')); // from hyph-utf8; check its licence
```

- **Lookup order.** Registered patterns take precedence over the
  built-in ones for that language. Text uses the patterns of its own
  tag, else of the nearest less specific one: `en-GB` text uses
  `en-GB` patterns, else `en` patterns, never `en-US` ones.
- **Bad patterns.** A malformed file fails at registration with
  `HYPHENATION_ERROR`, not later during a render.
- **Soft hyphens.** Soft hyphens (U+00AD) in the text are always
  honoured, with or without patterns.

[API.md](API.md#hyphenation-patterns) says where to download
pattern files such as `hyph-en-us.tex`.

## Checklist

- [ ] `scripts/build-node.sh` runs on every deploy platform, after every
      engine update.
- [ ] Fonts are registered once at startup, and the ids match the
      documents.
- [ ] Images go in `options.images`, JPEG for photos.
- [ ] `InkoxideError` is handled: author errors are shown with `path`,
      while `INTERNAL` and `INVALID_ARGUMENT` are logged as bugs.
- [ ] Choose a thread setup: `threads: 1` plus `UV_THREADPOOL_SIZE` for
      many small documents, or the default for a few large ones.
- [ ] `creationDate` is fixed wherever outputs are compared or hashed.
- [ ] Hyphenation patterns are registered for every language that has
      `hyphenate: true` but no built-in patterns.
