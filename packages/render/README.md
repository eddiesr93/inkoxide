# @inkoxide/render

The Inkoxide engine for every front end: the native engine in Node (the
[`inkoxide`](https://www.npmjs.com/package/inkoxide) package) or WebAssembly in a Web Worker in a
browser (`@inkoxide/render/browser`, with
[`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm)), the bundled fonts and
hyphenation patterns, `Font.register`, image loading under its policies, and the passes that
settle page numbers.

To make PDFs, install the package of your framework instead; each brings this one with it:

| Framework | Package |
|---|---|
| React | [`@inkoxide/react`](https://www.npmjs.com/package/@inkoxide/react) |
| Vue | [`@inkoxide/vue`](https://www.npmjs.com/package/@inkoxide/vue) |
| Svelte | [`@inkoxide/svelte`](https://www.npmjs.com/package/@inkoxide/svelte) |
| Angular | [`@inkoxide/angular`](https://www.npmjs.com/package/@inkoxide/angular) |

Install this package on its own to render documents you already have as JSON (a stored
`result.document`, or one another program wrote), or to write a front end of your own.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later; ES
modules only.

```sh
npm install @inkoxide/render
```

## Rendering a document

```js
import { writeFile } from 'node:fs/promises';
import { renderDocument } from '@inkoxide/render';

const doc = {
  version: 1,
  fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
  pages: [
    {
      size: { width: 595.28, height: 841.89 },
      margin: { top: 72, right: 72, bottom: 72, left: 72 },
      children: [{ type: 'text', runs: [{ text: 'Hello, Inkoxide', font: 'Inter-Regular', size: 24 }] }],
    },
  ],
};
const { pdf, pages } = await renderDocument(doc);
await writeFile('hello.pdf', pdf);
```

The bundled faces (Inter, Noto Sans, Roboto Mono) are registered under their families; the
document follows the
[schema reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md).

## API

| Name | What it does |
|---|---|
| `renderDocument(doc, images?, opts?)` | A document tree to `{ pdf, pages, engineMs }`. Its images are ids: `images` and `opts.images` give their bytes. |
| `init()` | Starts the engine, loads Tailwind when it is installed and the fonts added with `Font.register`. Every render calls it. In a browser it takes where the module and assets are (`@inkoxide/render/browser`). |
| `Font.register(…)` | Adds fonts by family, from bytes, a file or a URL. |
| `countPages(pdf)`, `version()`, `engineStatus()` | The page count of a PDF; the engine and schema versions; what `init` loaded. |
| `RenderError` | Every failure, with a `code`. |
| `createRenderer(frontEnd)` | For a front end: the functions it exports (`renderToBuffer`, `renderToFile`, `renderToStream`, `generate`, `renderTree`; in a browser `renderToBlob` and a web stream), from how it builds a document tree (`FrontEnd`). |
| `PdfRun`, `sameProps` (`@inkoxide/render/browser`) | For a front end's browser helpers: a document rendered to a PDF blob and its URL, again on request, each render stopping the one still running and each URL revoked when another replaces it; and whether two documents have the same props. |
| `@inkoxide/render/templates` | What templates served by id share in every front end: `parseData`, `renderSettled`, `renderDocument`. |

The [guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#api)
describes the options, errors and limits, which are the same in every front end.

## Licence

MIT. The bundled fonts are under the SIL Open Font License; their licences are in `assets/fonts`.
