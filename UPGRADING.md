# Upgrading

## From 0.10 to 1.0

1.0 adds front ends for Vue, Svelte and Angular beside React. React templates render as before,
to the same PDFs; what changes is where they import from.

### React templates import from `@inkoxide/react`

The components and the render functions moved from `@inkoxide/render` to `@inkoxide/react`, so
that every framework has one package of its own:

```sh
npm uninstall @inkoxide/render
npm install @inkoxide/react
```

| 0.10 | 1.0 |
|---|---|
| `import { Document, Page, Text, renderToFile } from '@inkoxide/render'` | `from '@inkoxide/react'` |
| `from '@inkoxide/render/browser'` | `from '@inkoxide/react/browser'` |
| `from '@inkoxide/render/templates'` | `from '@inkoxide/react/templates'` |

Every name is there, with the same props and options. `@inkoxide/render` still exists: it is the
engine every front end uses, and it renders documents you have as JSON (`renderDocument`). Its
`Font`, `init`, `countPages`, `version` and `RenderError` are the ones `@inkoxide/react` exports,
so code that imports `Font` from either sees the same registry.

### `timings.reactMs` is `timings.templateMs`

`generate`, and the tree of `renderTemplate`, report the time the template took to render as
`templateMs`, whatever the framework. `reactMs` still reads it, deprecated, but a spread or
`JSON.stringify` of the timings leaves it out: read `templateMs`.

### `@inkoxide/react` keeps its other names

`@inkoxide/react` still exports what it exported in 0.10 (`fonts`, `FontRegistry`,
`loadTailwind`, `validateTree`, `renderTemplate`, `renderToTree`, `HOST`, …), so code that built
document trees with it runs as before. Every front end shares them, from `@inkoxide/tree`.

### SVG markup

The SVG image an `<Svg>` becomes writes its attributes by name, so the markup is the same in every
framework. The drawing is the same; code that compared the markup as text sees another order.

### The other frameworks

To move a template to Vue, Svelte or Angular, see the README of
[`@inkoxide/vue`](packages/vue/README.md), [`@inkoxide/svelte`](packages/svelte/README.md) or
[`@inkoxide/angular`](packages/angular/README.md): each shows how its templates write the
components, the per-page content and the browser helpers.
