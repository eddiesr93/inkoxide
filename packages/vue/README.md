# @inkoxide/vue

Vue templates to PDF with the Inkoxide engine, in Node and in the browser. You write `Document`,
`Page`, `View`, `Text`, `Table` and the other components in single-file components or render
functions, style them with `style` or Tailwind classes, and get a PDF: the same PDF, byte for
byte, as the same document written for `@inkoxide/react`.

The [guide](GUIDE.md) says how Vue writes each part of a document, and the API.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later and Vue
3.5 or later; ES modules only.

```sh
npm install @inkoxide/vue vue
```

## Quick start

```vue
<!-- Receipt.vue -->
<script setup lang="ts">
import { Document, Page, Text, View } from '@inkoxide/vue';

const props = defineProps<{ customer: string; lines: { name: string; amount: number }[] }>();
const total = props.lines.reduce((sum, l) => sum + l.amount, 0);
</script>

<template>
  <Document :title="`Receipt for ${customer}`">
    <Page size="A6" class="p-8 text-xs text-gray-900">
      <Text class="text-lg font-bold">Receipt</Text>
      <Text class="mb-4 text-gray-500">for {{ customer }}</Text>
      <View v-for="l in lines" :key="l.name" class="flex-row justify-between border-b border-gray-200 py-1.5">
        <Text>{{ l.name }}</Text>
        <Text>${{ l.amount.toFixed(2) }}</Text>
      </View>
      <View class="mt-3 flex-row justify-between font-bold">
        <Text>Total</Text>
        <Text>${{ total.toFixed(2) }}</Text>
      </View>
    </Page>
  </Document>
</template>
```

```ts
import { h } from 'vue';
import { renderToFile } from '@inkoxide/vue';

import Receipt from './Receipt.vue';

await renderToFile(h(Receipt, { customer: 'Ada Lovelace', lines: [{ name: 'Espresso', amount: 3.2 }] }), 'receipt.pdf');
```

`renderToBuffer`, `renderToFile` and `renderToStream` take a VNode (`h(Receipt, props)`) or a
component. Classes need [`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind).

## In Vue

The components, props, styles, errors and options are those of the
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md), written the Vue
way:

| | React | Vue |
|---|---|---|
| Classes | `className="p-4"` | `class="p-4"` |
| Props | `headerRows={1}` | `:header-rows="1"` or `:headerRows="1"` |
| A prop that is `true` | `hyphenate` | `hyphenate` |
| Content per page | `render={({ pageNumber }) => …}` | `<template #render="{ pageNumber, totalPages }">…</template>` |
| The page, deeper down | `usePage()` | `usePage()` (it injects the page) |
| What to render | `<Invoice invoice={invoice} />` | `h(Invoice, { invoice })` |
| A template for `generate` | `Template: ComponentType<{ data }>` | `Template`: a component with the prop `data` |

A template renders once, synchronously: `<Suspense>`, `async setup()` and async components are
errors. Load the data first and pass it in as props. Listeners (`@click`) and Vapor components are errors.

### Single-file components on a server

A server build (Vite's SSR, Vitest in Node, a Nuxt app's server bundle) compiles `.vue` files to
render HTML strings, which Inkoxide cannot use: it renders the components itself. Compile the PDF
templates for the client with `@inkoxide/vue/vite`:

```ts
// vite.config.ts (or nuxt.config.ts: vite.plugins, for the app's code)
import vue from '@vitejs/plugin-vue';
import { pdfTemplates } from '@inkoxide/vue/vite';

export default defineConfig({ plugins: [pdfTemplates(vue(), /\/pdf\//)] });
```

The second argument picks the template files (here, those in a `pdf` folder); without it, every
`.vue` file is compiled for the client. Templates written as render functions (`h()`) or JSX need
nothing. Nuxt's server routes (`server/`) are built by Nitro, not Vite, and Nitro does not compile
`.vue` files: write the templates they render as render functions.

## In the browser

`@inkoxide/vue/browser` renders in a Web Worker, with
[`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed:

```vue
<script setup lang="ts">
import { h } from 'vue';
import { PdfViewer, PdfDownloadLink } from '@inkoxide/vue/browser';

import Receipt from './Receipt.vue';

const props = defineProps<{ customer: string; lines: { name: string; amount: number }[] }>();
const document = () => h(Receipt, props);
</script>

<template>
  <PdfViewer :document="document()" class="h-[600px] w-full">
    <template #fallback>Rendering…</template>
  </PdfViewer>
  <PdfDownloadLink :document="document()" file-name="receipt.pdf" v-slot="{ loading }">
    {{ loading ? 'Rendering…' : 'Download' }}
  </PdfDownloadLink>
</template>
```

`usePdf(() => h(Receipt, props))` gives `blob`, `url`, `loading` and `error` as refs and renders
again when what the getter reads changes; `<BlobProvider :document v-slot="{ blob, url, loading,
error }">` hands them to its slot. `renderToBlob` and `renderToStream` (a web stream) are there
too. See [In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser)
for where the engine and fonts come from.

## Licence

MIT
