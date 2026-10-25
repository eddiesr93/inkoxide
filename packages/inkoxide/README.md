# inkoxide

Native PDF engine: a declarative document tree (JSON or MessagePack) in,
PDF bytes out.

```sh
npm install inkoxide
```

npm installs the prebuilt binary for the current platform, as an optional
dependency (`@inkoxide/linux-x64-gnu`, `@inkoxide/darwin-arm64`, …);
[Platforms](https://github.com/eddiesr93/inkoxide#platforms) lists them.
Node.js 22 or newer; ES modules only. On another platform, build the addon from
source with `scripts/build-node.sh`, which puts `inkoxide.<platform>.node` next
to the loader.

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { registerFont, render, validate, InkoxideError } from 'inkoxide';

// Once at startup: fonts live in a process-wide cache.
registerFont('Inter-Regular', readFileSync('fonts/Inter-Regular.ttf'));

const doc = {
  version: 1,
  fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
  pages: [{ size: { width: 595.28, height: 841.89 }, margin: { top: 48, right: 48, bottom: 48, left: 48 },
            children: [
              { type: 'image', src: 'logo', style: { width: 120 } },
              { type: 'text', runs: [{ text: 'Hello', font: 'Inter-Regular', size: 24 }] },
            ] }],
};

try {
  // Runs on the libuv pool; the event loop keeps running. `src: 'logo'` is an id in `images`.
  const pdf = await render(JSON.stringify(doc), { images: { logo: readFileSync('logo.jpg') } });
  writeFileSync('out.pdf', pdf);
} catch (e) {
  if (!(e instanceof InkoxideError)) throw e;
  // e.code: 'UNKNOWN_FONT', 'INVALID_VALUE', …, 'INVALID_ARGUMENT' or 'INTERNAL'
  // e.path: 'pages[0].children[0].runs[0].font' ('' when not tied to a location)
  console.error(e.code, e.path, e.message);
}

// The problems at once (at most 100), without rendering: [{ code, path, message }, …]
console.log(validate(JSON.stringify(doc)));
```

Options, defaults and error codes are typed in `index.d.ts` and described in the
[API reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/API.md). The document
format is in the [schema reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md)
and the [example documents](https://github.com/eddiesr93/inkoxide/blob/main/packages/schema/EXAMPLES.md).
To write documents as components, use the package of your framework:
[`@inkoxide/react`](https://www.npmjs.com/package/@inkoxide/react),
[`@inkoxide/vue`](https://www.npmjs.com/package/@inkoxide/vue),
[`@inkoxide/svelte`](https://www.npmjs.com/package/@inkoxide/svelte) or
[`@inkoxide/angular`](https://www.npmjs.com/package/@inkoxide/angular).
