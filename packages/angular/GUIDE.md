# @inkoxide/angular guide

How to write Inkoxide templates in Angular. The [README](README.md) covers installing and the
quick start. The components, their props, styles, fonts, images, pagination, forms, options and
errors are the same in every front end, and the
[React guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md) is their
reference; this guide says how Angular writes them.

- [Templates](#templates)
- [Elements and props](#elements-and-props)
- [Styling](#styling)
- [Content per page](#content-per-page)
- [Errors](#errors)
- [On a server](#on-a-server)
- [In the browser](#in-the-browser)
- [Serving templates](#serving-templates)
- [API](#api)

## Templates

A template is a standalone component whose template is an `<ink-document>`, with
`CUSTOM_ELEMENTS_SCHEMA`, so that Angular passes the attributes and property bindings of the
elements on:

```ts
import { Component, CUSTOM_ELEMENTS_SCHEMA, input } from '@angular/core';

@Component({
  selector: 'app-receipt',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document title="Receipt">
      <ink-page size="A6" class="p-8">
        <ink-text class="text-lg font-bold">Receipt for {{ customer() }}</ink-text>
      </ink-page>
    </ink-document>
  `,
})
export class Receipt {
  readonly customer = input.required<string>();
}
```

Inputs, `@if`, `@for`, `ng-content`, dependency injection and components of your own work; a
component's host element is replaced by its content. Change detection runs once. The white space of
a template is read as JSX reads it, so a document gives the same tree, and the same PDF, in every
front end.

## Elements and props

Each component of the React guide is an element: `ink-` and its name in kebab case (`<ink-view>`,
`<ink-text>`, `<ink-table>`, `<ink-path>`, `<ink-text-field>`, `<ink-page-number>`). Its props:

| | React | Angular |
|---|---|---|
| A string | `size="A4"` | `size="A4"` |
| Another value | `headerRows={1}` | `[headerRows]="1"` (an attribute is a string: bind numbers) |
| `true` | `hyphenate` | `hyphenate` |
| A function (`paint`, `hyphenationCallback`) | `paint={fn}` | `[paint]="fn"` |

The elements are not type-checked in templates: `CUSTOM_ELEMENTS_SCHEMA` lets any property
through, and the props' errors come when the document is built.

## Styling

`class`, `[class]` and `[class.x]` take Tailwind classes (with
[`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind) installed).
`[style]="{ padding: 20 }"` takes the style object of the React guide's flat properties; Angular
refuses nested objects there, so `@media` blocks and lists of styles go in `[inkStyle]`, which
applies after `[style]`. Classes apply first.

## Content per page

The `render` prop of `ink-text` and `ink-view` is an `ng-template` in Angular, given the page as
its context:

```html
<ink-footer>
  <ink-text [render]="footer" />
  <ng-template #footer let-page>Page {{ page.pageNumber }} of {{ page.totalPages }}</ng-template>
</ink-footer>
```

`let-n="pageNumber"` reads one field. In a component inside the template, `injectPage()` (or
`inject(INK_PAGE)`) gives the page. `<ink-page-number>` and `<ink-total-pages>` are simpler where
they do.

## Errors

A problem in a template is a `TemplateError` (a `RenderError` with the code `TEMPLATE_ERROR` from
the render functions), whose `componentStack` names the components from the element at fault
outwards: an element in the wrong place, from the renderer of each component; what a template
throws, from the error's trace (each template is a function named after its component) where the
code keeps its names. Listeners (`(click)`) are errors: nothing happens in a PDF.

## On a server

`renderToBuffer`, `renderToFile` and `renderToStream` take a component with the values of its
inputs (`{ component: Invoice, inputs: { invoice } }`), or a component alone. The component is
compiled like any other: by the Angular build of your server (Angular SSR, Analog), or just in time
in a plain Node script, with `import '@angular/compiler';` first. Signal inputs need the AOT
compiler; a script compiled just in time uses `@Input()`.

## In the browser

`@inkoxide/angular/browser` renders in a Web Worker, with
[`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed:

- `injectPdf(document, options?)` renders the document (a value, a signal, or a function that reads
  signals) at once and whenever it changes, and gives `blob`, `url`, `safeUrl` (trusted for
  `<iframe [src]>`), `loading` and `error` as signals, and `update()`. Call it in an injection
  context; destroying the injector stops the render and revokes the URL.
- Three directives, for the elements they drive:
  - `<iframe [inkPdfViewer]="document">` shows the PDF in the browser's viewer
    (`[inkPdfViewerToolbar]="false"` hides its toolbar);
  - `<a [inkPdfDownload]="document" inkPdfDownloadFile="invoice.pdf">` downloads it once it has
    rendered (`aria-disabled` until then); `#link="inkPdfDownload"` gives `link.pdf`, the state;
  - `<ng-container *inkBlobProvider="document; let pdf">` shows its content with the state.
- `renderToBlob` and `renderToStream` (a web stream).

```ts
import { Component, input } from '@angular/core';
import { InkPdfDownload, InkPdfViewer } from '@inkoxide/angular/browser';

import { Receipt } from './receipt';

@Component({
  selector: 'app-preview',
  imports: [InkPdfViewer, InkPdfDownload],
  template: `
    <iframe [inkPdfViewer]="document()" title="Receipt"></iframe>
    <a [inkPdfDownload]="document()" inkPdfDownloadFile="receipt.pdf">Download</a>
  `,
})
export class Preview {
  readonly customer = input.required<string>();
  readonly document = () => ({ component: Receipt, inputs: { customer: this.customer() } });
}
```

They render again only when the document is another one: another component, or other input
values. The directives are declared as a library compiled for linking is, so the application's
Angular build links them. Where the engine and the fonts come from, a CDN and Content Security
Policy are in the React guide's
[In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser);
an application compiled ahead of time needs no `'unsafe-eval'`.

## Serving templates

`@inkoxide/angular/templates` is the React guide's
[Serving templates](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#serving-templates)
for Angular: a template is `{ meta: { id }, schema, Template, images? }`, whose `Template` is a
component with the input `data`.

## API

| Name | What it does |
|---|---|
| `renderToBuffer(input, options?)`, `renderToFile(input, path, options?)`, `renderToStream(input, options?)` | A component (with its inputs) to a PDF (Node). |
| `renderTemplate(input, options?)`, `renderToTree(input)` | A template to its document tree, without the engine. |
| `injectPage()`, `INK_PAGE` | The page, in a `render` template. |
| `Font`, `init`, `countPages`, `version`, `engineStatus`, `RenderError`, `TemplateError` | As in the React guide. |
| `@inkoxide/angular/browser` | The above without what needs Node, with `injectPdf`, `InkPdfViewer`, `InkPdfDownload`, `InkBlobProvider`, `renderToBlob`, `generate`. |
| `@inkoxide/angular/templates` | `generate`, `renderTree`, `parseData`, `renderSettled`, `renderDocument`. |

The options of each are the React guide's
[API](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#api).
