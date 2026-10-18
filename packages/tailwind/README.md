# @inkoxide/tailwind

Turns Tailwind class strings into Inkoxide style objects, with the Tailwind v4 compiler itself.
Units are tuned for print: `1rem` is 12pt and `1px` 0.75pt. Page variants (`md:`, `landscape:`,
`print:`) read the page the element is on. An unsupported class (a variant such as `hover:`, a
shadow, a gradient) throws outside production, and is logged once and ignored in production.

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

## In a browser bundle

In a browser, `@inkoxide/tailwind` loads Tailwind's compiler, about 90 KB with brotli. The build
plugin replaces it with a table of the classes your sources use (a few KB): it scans them when
the bundle is built, and in client bundles points `@inkoxide/tailwind` at
`@inkoxide/tailwind/precompiled`, which reads the table. Classes are still resolved when a
document renders, with the same results; server bundles keep the compiler.

```ts
// vite.config.ts (Vite, and Rollup or Rolldown the same way)
import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';

export default { plugins: [inkoxideTailwind()] };
```

```js
// esbuild, for builds with `platform: 'browser'` (its default)
import { inkoxideTailwindEsbuild } from '@inkoxide/tailwind/plugin';

await esbuild.build({ bundle: true, plugins: [inkoxideTailwindEsbuild()], … });
```

```js
// webpack 5 and Rspack, for builds that target a browser
import { inkoxideTailwindWebpack } from '@inkoxide/tailwind/plugin';

export default { plugins: [inkoxideTailwindWebpack()], … };
```

```ts
// next.config.ts (Next.js 15.3 or later, with Turbopack or webpack)
import { withInkoxideTailwind } from '@inkoxide/tailwind/plugin';

const nextConfig = { … };

export default withInkoxideTailwind(nextConfig, { safelist: ['bg-red-500'] });
```

The options are all optional: `css`, the theme (or `cssFile`, a file whose `@import`s resolve
from its folder), which `initTailwind({ css })` in the browser must then be given as well;
`sources`, the directories and files to scan, relative to the project root (by default the root,
without `node_modules`, build output and hidden directories; in a directory, files of the types
the scanner knows, such as `.tsx`, `.vue` or `.html`, and a file named here whatever its type);
and `safelist`, classes to keep whatever the scan finds. A Vite dev server scans each file that
is created or changes and reloads the page when it brings a class the table lacks.

A webpack or Rspack build scans again on each run, and in watch mode on each rebuild. Builds whose
`target` is Node (`node`, `async-node`, `electron-main` and the like) are left alone and keep the
compiler. `withInkoxideTailwind` takes the config as an object or a function, and its second
argument is the plugin's options. With Turbopack (the default in Next.js 16) it writes the table
to `node_modules/.cache/@inkoxide/tailwind/browser.js` at `next dev` and `next build`, and sets
`turbopack.resolveAlias` so that the browser gets it; with `next build --webpack` it adds the
webpack plugin to the client builds. Server bundles keep the compiler. Turbopack has no hook to
scan again: `next dev` scans when the server starts, so a class that is new since then needs a
restart, or a place in `safelist`.

A class the scan does not see is unknown in the browser: write each class whole
(`bg-red-500`, not `` `bg-${color}-500` ``), or put it in `safelist`. It fails as an unknown
class does, in strict mode with a message that says so.

### `@source` directives

When the theme comes from `cssFile` or `css`, its `@source` directives name sources as Tailwind
reads them, and add to the scan. Paths are relative to the CSS file that has them, an imported
file too.

```css
@import 'tailwindcss' source('../app'); /* start the scan here, not at the project root */
@source '../lib'; /* a directory or a file */
@source '../src/**/*.tsx'; /* a glob */
@source not '../src/legacy'; /* leave these out */
@source inline('bg-red-{100..900..100}'); /* keep these classes */
@source not inline('bg-red-500'); /* never keep these */
```

`@import 'tailwindcss' source(none)` scans only what the directives name. The `sources` option,
when given, replaces the project root as before; the directives add to it. In a Vite dev server,
a change to the theme file, or to a CSS file it imports, scans again.

The scan reads the sources' text, not `.gitignore`: it leaves out `node_modules`, hidden
directories and build output directly under a root (`dist`, `build`, `out`, …), and the
bundle's output directory. A Vite dev server reads each file that is created or changes; the
watch modes of Rollup, esbuild, webpack and Rspack scan again on each build, but watch only the
files there were at the last scan. In a plain Rollup config, put the plugin before `@rollup/plugin-node-resolve`, so that it
sees `@inkoxide/tailwind` first. `@inkoxide/tailwind/precompiled` has the entry's documented
API; the deprecated `TailwindCompiler`, `SUPPORTED` and `UNSUPPORTED` are not in it.

The plugin needs nothing beyond this package. In this repository's workspace, where packages are
TypeScript sources, Vite loads a config that imports it with `--configLoader runner`.

## Licence

MIT
