# Client integration (Node.js)

How to add Inkoxide to a Node application. [API.md](API.md) has the full
API and [../schema/EXAMPLES.md](../schema/EXAMPLES.md) has complete
documents. `examples/node-smoke` is a working client that covers
everything on this page.

## Install

Nothing is published to npm. Build the package from a checkout next to
your app, then install it with `file:` dependencies:

```sh
# in ../inkoxide
scripts/build-node.sh        # writes npm/inkoxide.<platform>.node

# in your app
pnpm add file:../inkoxide/npm           # the engine: `inkoxide`
pnpm add file:../inkoxide/npm-schema    # types and JSON Schema: `@inkoxide/schema`
```

- **The native addon is per platform.** Run `scripts/build-node.sh` on
  every OS and CPU you deploy to: a Linux build does not load on macOS.
- **Rebuild after every engine change.** After pulling, run
  `scripts/build-node.sh` again; pnpm then picks up the new files from
  the `file:` dependency.
- **Module systems.** The package is CommonJS and has named exports, so
  `import { render } from 'inkoxide'` works from ESM and TypeScript
  (`module: nodenext`).

Log `version()` once at startup. It returns
`{ engine, schema, gitSha }`, so a bug report can name the exact build.

## Fonts: register once, at startup

```js
import { readFileSync } from 'node:fs';
import { registerFont } from 'inkoxide';

for (const id of ['Inter-Regular', 'Inter-Bold']) {
  registerFont(id, readFileSync(`fonts/${id}.ttf`)); // TrueType or CFF OpenType
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
- **Unsupported fonts.** Variable fonts and CFF2 fonts are rejected with
  `FONT_ERROR`. Register static instances instead.

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
  decoded and recompressed on every render, alpha included. SVG is drawn
  as vectors, which suits logos and charts.
- **Avoid inline `data` for large images.** A node's inline `data` field
  (base64) also works, but it inflates the document JSON. Use
  `options.images` for anything large.
- **Images are not cached between renders.** Pass the same `Buffer`
  each time; keeping the bytes in memory is up to you.

## Documents

- **Types.** Build documents with the types from `@inkoxide/schema`:
  `import type { Document } from '@inkoxide/schema'`. Pass them to
  `render` as `JSON.stringify(doc)` or as a `Uint8Array` of UTF-8 JSON.
- **Large documents.** For many pages, MessagePack is faster to parse:
  pass the encoded bytes with `{ format: 'msgpack' }`. Encode them with
  any library that writes plain maps, arrays and strings.
- **Validating before rendering.** For example, in a form or an editor:
  - `validate(doc)` returns every problem at once, as
    `[{ code, path, message }]`, without rendering.
  - Client-side, ajv with the schema from `@inkoxide/schema` reports the
    same paths through `enginePath(error)`
    ([npm-schema/README.md](../npm-schema/README.md)).

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
  `INTERNAL`.

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
  count. That gives one document per thread with no coordination
  overhead.
- **Few large documents.** For a 500-page export, keep the default.
- **Memory.** Peak memory grows with the document: about 210 MB for a
  435-page table (see [BENCHMARKS.md](BENCHMARKS.md)). The engine
  refuses documents that would exceed its page limit, with
  `LAYOUT_ERROR`.

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

A text node hyphenates when its style has `hyphenate: true`. The language
is the node's `lang`, or else `meta.lang`. The engine includes patterns
for about 50 languages. **Romanian is not among them:** load the
standard TeX patterns yourself, once, at startup:

```js
import { registerHyphenationPatterns } from 'inkoxide';
registerHyphenationPatterns('ro', readFileSync('hyph-ro.tex')); // from hyph-utf8; check its licence
```

- **Lookup order.** Registered patterns take precedence over the
  built-in ones for that language.
- **Bad patterns.** A malformed file fails at registration with
  `HYPHENATION_ERROR`, not later during a render.
- **Soft hyphens.** Soft hyphens (U+00AD) in the text are always
  honoured, with or without patterns.

[API.md](API.md#hyphenation-patterns) says where to download
`hyph-ro.tex`.

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
      `hyphenate: true` but no built-in patterns (Romanian).
