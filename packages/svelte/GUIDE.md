# @inkoxide/svelte guide

How to write Inkoxide templates in Svelte. The [README](README.md) covers installing and the
quick start. The components, their props, styles, fonts, images, pagination, forms, options and
errors are the same in every front end, and the
[React guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md) is their
reference; this guide says how Svelte writes them.

- [Templates](#templates)
- [Components and props](#components-and-props)
- [Styling](#styling)
- [Content per page](#content-per-page)
- [Errors](#errors)
- [On a server](#on-a-server)
- [In the browser](#in-the-browser)
- [Serving templates](#serving-templates)
- [API](#api)
- [How it works](#how-it-works)

## Templates

A template is a Svelte 5 component whose markup is a `<Document>`:

```svelte
<script lang="ts">
  import { Document, Page, Text } from '@inkoxide/svelte';

  let { customer }: { customer: string } = $props();
</script>

<Document title="Receipt">
  <Page size="A6" class="p-8">
    <Text class="text-lg font-bold">Receipt for {customer}</Text>
  </Page>
</Document>
```

Props, `{#if}`, `{#each}`, snippets, contexts and components of your own work. The white space of
the markup is read as JSX reads it, so a document gives the same tree, and the same PDF, in every
front end.

## Components and props

The components are those of the React guide, with the same names (`View`, `Text`, `Table`,
`TextField`, `Path`, …) and the same props: `headerRows={1}`, `hyphenate`, `paint={fn}`. Content
is the component's children. The components are typed with their props.

## Styling

`class` takes Tailwind classes (with
[`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind) installed), as a string,
a list or an object of flags. `style` takes the style object of the React guide (a component prop,
not CSS text), `@media` blocks and lists of styles included. Classes apply first, then `style`.

## Content per page

The `render` prop of `Text` and `View` is a snippet in Svelte, given the page:

```svelte
<Footer>
  <Text>
    {#snippet render(page)}Page {page.pageNumber} of {page.totalPages}{/snippet}
  </Text>
</Footer>
```

In a component inside that snippet, `usePage()` gives the page, while the component is created.
The snippet's content gets the contexts of the components around the element. `<PageNumber>` and
`<TotalPages>` are simpler where they do.

## Errors

A problem in a template is a `TemplateError` (a `RenderError` with the code `TEMPLATE_ERROR` from
the render functions), whose `componentStack` names the components from the element at fault
outwards. Svelte keeps no component stack: Inkoxide reads it from the error's trace, where each
component is the function of its file, so it names your components where the code keeps its file
names (a development build, a server build), and the Inkoxide components around the element
otherwise.

An Inkoxide element its component did not render (`<ink-view>` written in the markup, or in
`{@html}`) is an error.

## On a server

`renderToBuffer`, `renderToFile` and `renderToStream` take a component with its props
(`{ component: Invoice, props: { invoice } }`), or a component alone. They render it with
`svelte/server`, so the component must be compiled for the server, as SvelteKit's endpoints and
Vite's SSR do; the package ships its components as `.svelte` files, which the app's build compiles
(`@sveltejs/vite-plugin-svelte` does it for the packages with a `svelte` export condition).

```ts
// src/routes/invoice.pdf/+server.ts
import { renderToBuffer } from '@inkoxide/svelte';

import Invoice from '$lib/pdf/Invoice.svelte';

export async function GET() {
  const pdf = await renderToBuffer({ component: Invoice, props: { invoice: await loadInvoice() } });
  return new Response(pdf, { headers: { 'Content-Type': 'application/pdf' } });
}
```

## In the browser

`@inkoxide/svelte/browser` mounts the template in an element of no document and renders the PDF in
a Web Worker, with [`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed:

- `createPdf(() => document, () => options)` renders the document at once and whenever what the
  function reads changes, and gives `blob`, `url`, `loading` and `error`, and `update()`. Call it
  while a component is created; destroying the component stops the render and revokes the URL.
- `<PdfViewer {document}>` shows the PDF in an `<iframe>`, in the browser's viewer (a `fallback`
  snippet before the first render, `showToolbar={false}` to hide the viewer's toolbar);
  `<PdfDownloadLink {document} fileName="…">` is a link that downloads it, its children snippet
  given the render's state; `<BlobProvider {document}>` hands the state to its children snippet.
- `renderToBlob` and `renderToStream` (a web stream).

They render again only when the document is another one: another component, or other prop values.
Where the engine and the fonts come from, a CDN and Content Security Policy are in the React
guide's [In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser).

## Serving templates

`@inkoxide/svelte/templates` is the React guide's
[Serving templates](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#serving-templates)
for Svelte: a template is `{ meta: { id }, schema, Template, images? }`, whose `Template` is a
component with the prop `data`.

```ts
import { generate, type TemplateLike } from '@inkoxide/svelte/templates';
import { z } from 'zod';

import Invoice from './Invoice.svelte'; // let { data } = $props();

const schema = z.object({ number: z.string(), total: z.number() });
const invoice: TemplateLike<typeof schema> = { meta: { id: 'invoice' }, schema, Template: Invoice };

const { pdf } = await generate(invoice, { number: '2026-001', total: 120 });
```

## API

| Name | What it does |
|---|---|
| `renderToBuffer(input, options?)`, `renderToFile(input, path, options?)`, `renderToStream(input, options?)` | A component (with its props) to a PDF (Node). |
| `renderTemplate(input, options?)`, `renderToTree(input)` | A template to its document tree, without the engine; the tree's `dispose()` releases what the template rendered. |
| `usePage()` | The page, in a `render` snippet. |
| `Font`, `init`, `countPages`, `version`, `engineStatus`, `RenderError`, `TemplateError` | As in the React guide. |
| `@inkoxide/svelte/browser` | The above without what needs Node, with `createPdf`, `PdfViewer`, `PdfDownloadLink`, `BlobProvider`, `renderToBlob`, `generate`. |
| `@inkoxide/svelte/templates` | `generate`, `renderTree`, `parseData`, `renderSettled`, `renderDocument`. |

The options of each are the React guide's
[API](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#api).

## How it works

Svelte has no custom renderers yet. Each component renders an element of its own (`<ink-view
data-ink="3">`), whose number is its place in a table of the render, given by context, where the
component puts its props (style objects, functions, bytes and snippets, which markup cannot
carry). On a server, Inkoxide reads the HTML `svelte/server` writes; in a browser, the DOM `mount`
builds. A template renders once; what an effect changes after does not reach the document.
