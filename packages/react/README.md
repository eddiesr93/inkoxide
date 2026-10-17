# @inkoxide/react

React components (`Document`, `Page`, `View`, `Text`, `Image`, `Table` and the rest) and a
custom reconciler that build Inkoxide document trees: the JSON the engine turns into a PDF.
Style with `style`, or with `className` when `@inkoxide/tailwind` is installed; fonts come from
a registry.

To make PDFs, install [`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render)
instead: it re-exports these components and renders them, with the bundled fonts. Its
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/render/GUIDE.md) describes the
components and their props.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later; ES
modules only.

```sh
npm install @inkoxide/react react
```

## Usage

```js
import { readFileSync } from 'node:fs';
import { createElement as h } from 'react';
import { Document, Page, Text, fonts, renderToTree } from '@inkoxide/react';

fonts.register({ id: 'Inter-Regular', family: 'Inter' }, readFileSync('Inter-Regular.ttf'));
const doc = await renderToTree(
  h(Document, null, h(Page, { size: 'A4', margin: '20mm' }, h(Text, { style: { fontSize: 18 } }, 'Hello'))),
);
console.log(JSON.stringify(doc)); // an Inkoxide document (schema v1)
```

The tree follows the
[schema reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md); the
[`inkoxide`](https://www.npmjs.com/package/inkoxide) engine renders it.

## Licence

MIT
