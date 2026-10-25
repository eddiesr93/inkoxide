# @inkoxide/vue guide

How to write Inkoxide templates in Vue. The [README](README.md) covers installing and the quick
start. The components, their props, styles, fonts, images, pagination, forms, options and errors
are the same in every front end, and the
[React guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md) is their
reference; this guide says how Vue writes them.

- [Templates](#templates)
- [Components and props](#components-and-props)
- [Styling](#styling)
- [Content per page](#content-per-page)
- [Errors](#errors)
- [On a server](#on-a-server)
- [In the browser](#in-the-browser)
- [Serving templates](#serving-templates)
- [API](#api)
- [Limitations](#limitations)

## Templates

A template is a Vue component whose render gives a `<Document>`: a single-file component, a
component with a render function (`h`), or JSX through `@vitejs/plugin-vue-jsx`. What it renders
is not DOM: Inkoxide renders it with a renderer of its own, once, and reads the document from what
the components made.

```vue
<script setup lang="ts">
import { Document, Page, Text } from '@inkoxide/vue';

defineProps<{ customer: string }>();
</script>

<template>
  <Document title="Receipt">
    <Page size="A6" class="p-8">
      <Text class="text-lg font-bold">Receipt for {{ customer }}</Text>
    </Page>
  </Document>
</template>
```

Any Vue feature that renders synchronously works: props, `v-if`, `v-for`, slots, `provide` and
`inject`, computed values, components of your own. The white space of a template is read as JSX
reads it, so a document gives the same tree, and the same PDF, in every front end.

## Components and props

The components are those of the React guide, with the same names (`View`, `Text`, `Table`,
`TextField`, `Path`, …) and the same props, written as Vue attributes:

| | React | Vue |
|---|---|---|
| A string | `size="A4"` | `size="A4"` |
| Another value | `headerRows={1}` | `:header-rows="1"` or `:headerRows="1"` |
| `true` | `hyphenate` | `hyphenate` |
| Content | children | the default slot |
| A function (`paint`, `hyphenationCallback`) | `paint={fn}` | `:paint="fn"` |

Kebab-case attributes are camelized. The components are typed with their props (`InkComponent`), for render functions and the editor.

## Styling

`class` takes Tailwind classes (with
[`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind) installed), as a string,
an array or an object of flags, as Vue normalizes it. `:style` takes the style object of the React
guide, `@media` blocks and lists of styles included. Classes apply first, then `style`.

## Content per page

The `render` prop of `Text` and `View` is a slot in Vue, given the page:

```vue
<Footer>
  <Text>
    <template #render="{ pageNumber, totalPages }">Page {{ pageNumber }} of {{ totalPages }}</template>
  </Text>
</Footer>
```

In a component inside that slot, `usePage()` gives the page. The slot's content gets what the
components around the element provide. `<PageNumber>` and `<TotalPages>` are simpler where they
do.

## Errors

A problem in a template is a `TemplateError` (a `RenderError` with the code `TEMPLATE_ERROR` from
the render functions), whose `componentStack` names the components, yours included, from the
element at fault outwards:

```text
<View> cannot be inside <Text>. A Text contains only strings, numbers, <Text>, <Link>, …
    in View
    in Address
    in Page
    in Document
    in Invoice
```

A template renders once, synchronously, so these are errors too:

- `<Suspense>`, `async setup()` and async components: load the data first and pass it in as props;
- Vapor components (`<script setup vapor>`), which render without the renderer Inkoxide gives;
- listeners (`@click`): nothing happens in a PDF.

## On a server

`renderToBuffer`, `renderToFile` and `renderToStream` take a VNode (`h(Invoice, { invoice })`) or a
component. A server build (Vite's SSR, Vitest in Node, a Nuxt app's server bundle) compiles `.vue`
files to render HTML strings, which Inkoxide cannot use; `@inkoxide/vue/vite` compiles the
templates for the client in it:

```ts
// vite.config.ts, or nuxt.config.ts's vite.plugins for the app's code
import vue from '@vitejs/plugin-vue';
import { pdfTemplates } from '@inkoxide/vue/vite';

export default defineConfig({ plugins: [pdfTemplates(vue(), /\/pdf\//)] });
```

The second argument picks the template files; without it, every `.vue` file is compiled for the
client. Templates written as render functions or JSX need nothing. Nuxt's server routes
(`server/`) are built by Nitro, which does not compile `.vue` files: the templates they render are
render functions.

## In the browser

`@inkoxide/vue/browser` renders in a Web Worker, with
[`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed:

- `usePdf(document, options?)` renders `document` (a VNode, a component, or a ref or getter of one)
  at once and whenever it changes, and gives `blob`, `url`, `loading` and `error` as refs, and
  `update()`. A render that starts stops the one still running; the end of the scope stops it and
  revokes the URL.
- `<PdfViewer :document>` shows the PDF in an `<iframe>`, in the browser's viewer (`#fallback`
  before the first render, `:show-toolbar="false"` to hide the viewer's toolbar);
  `<PdfDownloadLink :document file-name="…">` is a link that downloads it, its default slot given
  the render's state; `<BlobProvider :document v-slot="{ blob, url, loading, error }">` hands the
  state to its slot.
- `renderToBlob` and `renderToStream` (a web stream).

They render again only when the document is another one: a VNode of another component or key, or
with other prop values. Where the engine and the fonts come from, a CDN and Content Security
Policy are in the React guide's
[In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser).

## Serving templates

`@inkoxide/vue/templates` is the React guide's
[Serving templates](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#serving-templates)
for Vue: a template is `{ meta: { id }, schema, Template, images? }`, whose `Template` is a
component with the prop `data`.

```ts
import { generate, type TemplateLike } from '@inkoxide/vue/templates';
import { z } from 'zod';

import Invoice from './Invoice.vue'; // defineProps<{ data: { number: string; total: number } }>()

const schema = z.object({ number: z.string(), total: z.number() });
const invoice: TemplateLike<typeof schema> = { meta: { id: 'invoice' }, schema, Template: Invoice };

const { pdf } = await generate(invoice, { number: '2026-001', total: 120 });
```

## API

| Name | What it does |
|---|---|
| `renderToBuffer(input, options?)`, `renderToFile(input, path, options?)`, `renderToStream(input, options?)` | A VNode or a component to a PDF (Node). |
| `renderTemplate(input, options?)`, `renderToTree(input)` | A template to its document tree, without the engine; the tree's `dispose()` releases what the template rendered. |
| `usePage()` | The page, in a `render` slot. |
| `Renderable`, `VueProps`, `VueSlots` | What renders to a document (a VNode or a component), and a component's props and slots in Vue. |
| `Font`, `init`, `countPages`, `version`, `engineStatus`, `RenderError`, `TemplateError` | As in the React guide. |
| `@inkoxide/vue/browser` | The above without what needs Node, with `usePdf`, `PdfViewer`, `PdfDownloadLink`, `BlobProvider`, `renderToBlob`, `generate`; `PdfState` is the state the components' slots get. |
| `@inkoxide/vue/templates` | `generate`, `renderTree`, `parseData`, `renderSettled`, `renderDocument`. |
| `@inkoxide/vue/vite` | `pdfTemplates(vuePlugin, include?)`: `VuePluginLike` is what it takes of `@vitejs/plugin-vue`, and `TemplateFilter` picks the template files (a `RegExp` or a function of the file name). |

The options of each are the React guide's
[API](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#api).

## Limitations

- A template renders once: what an effect or a watcher changes after the first render does not
  reach the document.
- Templates are VDOM components: Vapor components are refused.
