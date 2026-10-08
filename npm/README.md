# inkoxide

Native PDF engine: a declarative document tree (JSON or MessagePack) in,
PDF bytes out. Built locally with `scripts/build-node.sh`; not published.

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { registerFont, render, validate, InkoxideError } from 'inkoxide';

// Once at startup: fonts live in a process-wide cache.
registerFont('Inter-Regular', readFileSync('fonts/Inter-Regular.ttf'));

const doc = {
  version: 1,
  fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
  pages: [{ size: { width: 595.28, height: 841.89 }, margin: { top: 48, right: 48, bottom: 48, left: 48 },
            children: [{ type: 'text', runs: [{ text: 'Hello', font: 'Inter-Regular', size: 24 }] }] }],
};

try {
  // Runs on the libuv pool; the event loop keeps running.
  const pdf = await render(JSON.stringify(doc), { images: { logo: readFileSync('logo.jpg') } });
  writeFileSync('out.pdf', pdf);
} catch (e) {
  if (!(e instanceof InkoxideError)) throw e;
  // e.code: 'UNKNOWN_FONT', 'INVALID_VALUE', …, 'INVALID_ARGUMENT' or 'INTERNAL'
  // e.path: 'pages[0].children[0].runs[0].font' ('' when not tied to a location)
  console.error(e.code, e.path, e.message);
}

// All problems at once, without rendering: [{ code, path, message }, …]
console.log(validate(JSON.stringify(doc)));
```

Options, defaults and error codes are typed in `index.d.ts` and described
in `docs/API.md`. Schema reference: `docs/SCHEMA.md` and `schema/EXAMPLES.md`.
