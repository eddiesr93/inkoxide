# @inkoxide/tailwind

Turns Tailwind class strings into Inkoxide style objects, with the Tailwind v4 compiler itself.
Units are tuned for print: `1rem` is 12pt and `1px` 0.75pt. An unsupported class (a variant
such as `md:`, a shadow, a gradient) throws outside production, and is logged once and ignored
in production.

Install it next to [`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render) to
style documents with `className`; without it, `style` works alone. The render
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/render/GUIDE.md#tailwind)
covers styling with classes, and
[TAILWIND_SUPPORT.md](https://github.com/eddiesr93/inkoxide/blob/main/docs/TAILWIND_SUPPORT.md)
lists every supported utility.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later; ES
modules only.

```sh
npm install @inkoxide/tailwind
```

## Usage

`@inkoxide/render` calls it for you. To map classes yourself:

```js
import { DEFAULT_THEME_CSS, initTailwind, setTailwindStrict, tw } from '@inkoxide/tailwind';

await initTailwind(); // once per process
const style = tw('p-4 text-lg font-bold text-slate-700');
console.log(style); // { paddingTop: 12, …, fontSize: 13.5, fontWeight: 700, color: […] }

// `em`, `vw` and `vh` in arbitrary values resolve against the units you give, in points:
// the inherited font size (and the element's, when something else sets it) and the page size.
tw('w-[2em] h-[10vh]', { parentFontSize: 12, page: { width: 595.28, height: 841.89 } }); // { width: 24, height: 84.189 }

// Options, all optional: `css`, a theme (extend the bundled one: its font stacks name the
// bundled fonts); `base`, the directory its @imports resolve from; `strict` (default: outside
// production); and `cacheSize`, how many class strings are cached (default 5000).
await initTailwind({
  css: DEFAULT_THEME_CSS + '@theme { --color-accent: #1e4a99; }',
  strict: true,
  cacheSize: 10000,
});
setTailwindStrict(false); // log unsupported classes once instead of throwing
tw.warm(['p-4 text-lg', 'flex gap-2']); // map known class strings ahead, e.g. at server start
tw.cacheSize(); // 2: the class strings cached
```

## Licence

MIT
