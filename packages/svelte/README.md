# @inkoxide/svelte

Svelte templates to PDF with the Inkoxide engine, in Node and in the browser. You write
`Document`, `Page`, `View`, `Text`, `Table` and the other components in Svelte 5 components,
style them with `style` or Tailwind classes, and get a PDF: the same PDF, byte for byte, as the
same document written for `@inkoxide/react`.

The [guide](GUIDE.md) says how Svelte writes each part of a document, and the API.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later and
Svelte 5.39 or later; ES modules only.

```sh
npm install @inkoxide/svelte svelte
```

## Quick start

```svelte
<!-- Receipt.svelte -->
<script lang="ts">
  import { Document, Page, Text, View } from '@inkoxide/svelte';

  let { customer, lines }: { customer: string; lines: { name: string; amount: number }[] } = $props();
  const total = $derived(lines.reduce((sum, l) => sum + l.amount, 0));
</script>

<Document title={`Receipt for ${customer}`}>
  <Page size="A6" class="p-8 text-xs text-gray-900">
    <Text class="text-lg font-bold">Receipt</Text>
    <Text class="mb-4 text-gray-500">for {customer}</Text>
    {#each lines as l (l.name)}
      <View class="flex-row justify-between border-b border-gray-200 py-1.5">
        <Text>{l.name}</Text>
        <Text>${l.amount.toFixed(2)}</Text>
      </View>
    {/each}
    <View class="mt-3 flex-row justify-between font-bold">
      <Text>Total</Text>
      <Text>${total.toFixed(2)}</Text>
    </View>
  </Page>
</Document>
```

```ts
// A SvelteKit endpoint, or any server code built with Vite.
import { renderToBuffer } from '@inkoxide/svelte';

import Receipt from './Receipt.svelte';

const pdf = await renderToBuffer({ component: Receipt, props: { customer: 'Ada Lovelace', lines: [] } });
```

`renderToBuffer`, `renderToFile` and `renderToStream` take a component with its props, or a
component alone. They render it with `svelte/server`, so the component must be compiled for the
server, as SvelteKit and Vite's SSR do. Classes need
[`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind).

## In Svelte

The components, props, styles, errors and options are those of the
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md), written the
Svelte way:

| | React | Svelte |
|---|---|---|
| Classes | `className="p-4"` | `class="p-4"` (a list or an object of flags too) |
| Content per page | `render={({ pageNumber }) => …}` | `{#snippet render(page)}{page.pageNumber}{/snippet}` inside the `Text` or `View` |
| The page, deeper down | `usePage()` | `usePage()`, while the component is created |
| What to render | `<Invoice invoice={invoice} />` | `{ component: Invoice, props: { invoice } }` |
| A template for `generate` | `Template: ComponentType<{ data }>` | `Template`: a component with the prop `data` |

How it works: Svelte has no custom renderers yet, so each component renders an element of its own
(`<ink-view>`), and puts its props in a table of the render, which Inkoxide reads with what Svelte
rendered. A template renders once; component stacks in errors name the Inkoxide components around
the element at fault, not your own.

## In the browser

`@inkoxide/svelte/browser` mounts the template in an element of no document and renders the PDF in
a Web Worker, with [`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed:

```svelte
<script lang="ts">
  import { PdfDownloadLink, PdfViewer } from '@inkoxide/svelte/browser';

  import Receipt from './Receipt.svelte';

  let { customer, lines } = $props();
  const document = $derived({ component: Receipt, props: { customer, lines } });
</script>

<PdfViewer {document} class="h-[600px] w-full">
  {#snippet fallback()}Rendering…{/snippet}
</PdfViewer>
<PdfDownloadLink {document} fileName="receipt.pdf">
  {#snippet children(pdf)}{pdf.loading ? 'Rendering…' : 'Download'}{/snippet}
</PdfDownloadLink>
```

`createPdf(() => document)` gives `blob`, `url`, `loading` and `error` and renders again when what
the function reads changes; `<BlobProvider {document}>` hands them to its children snippet.
`renderToBlob` and `renderToStream` (a web stream) are there too. See
[In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser)
for where the engine and fonts come from.

## Licence

MIT
