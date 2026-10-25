# @inkoxide/angular

Angular templates to PDF with the Inkoxide engine, in Node and in the browser. A template is a
component whose template uses the Inkoxide elements (`<ink-document>`, `<ink-page>`,
`<ink-view>`, `<ink-text>`, `<ink-table>` and the rest), styled with classes or `[style]`; the PDF
is the same, byte for byte, as the same document written for `@inkoxide/react`.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later and
Angular 21 or 22; ES modules only.

```sh
npm install @inkoxide/angular
```

## Quick start

```ts
import { Component, CUSTOM_ELEMENTS_SCHEMA, input } from '@angular/core';
import { renderToFile } from '@inkoxide/angular';

@Component({
  selector: 'app-receipt',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Receipt for ' + customer()">
      <ink-page size="A6" class="p-8 text-xs text-gray-900">
        <ink-text class="text-lg font-bold">Receipt</ink-text>
        <ink-text class="mb-4 text-gray-500">for {{ customer() }}</ink-text>
        @for (l of lines(); track l.name) {
          <ink-view class="flex-row justify-between border-b border-gray-200 py-1.5">
            <ink-text>{{ l.name }}</ink-text>
            <ink-text>\${{ l.amount.toFixed(2) }}</ink-text>
          </ink-view>
        }
      </ink-page>
    </ink-document>
  `,
})
export class Receipt {
  readonly customer = input.required<string>();
  readonly lines = input.required<{ name: string; amount: number }[]>();
}

await renderToFile(
  { component: Receipt, inputs: { customer: 'Ada Lovelace', lines: [{ name: 'Espresso', amount: 3.2 }] } },
  'receipt.pdf',
);
```

`renderToBuffer`, `renderToFile` and `renderToStream` take a component with the values of its
inputs, or a component alone. Classes need
[`@inkoxide/tailwind`](https://www.npmjs.com/package/@inkoxide/tailwind).

The template is compiled like any other: by your Angular build (the CLI, Analog, `ng build` of a
server), or just in time in a plain Node script, with `import '@angular/compiler';` first (signal
inputs need the AOT compiler; use `@Input()` there).

## In Angular

The components, props, styles, errors and options are those of the
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md), written the
Angular way:

| | React | Angular |
|---|---|---|
| Components | `<View>`, `<Path>`, `<TextField>` | `<ink-view>`, `<ink-path>`, `<ink-text-field>`: `ink-` and the name in kebab case |
| Classes | `className="p-4"` | `class="p-4"`, `[class]="…"` |
| Props | `headerRows={1}`, `size="A4"` | `[headerRows]="1"`, `size="A4"` (an attribute is a string: bind numbers) |
| A prop that is `true` | `hyphenate` | `hyphenate` |
| Style | `style={{ padding: 20 }}` | `[style]="{ padding: 20 }"`; `[inkStyle]` for `@media` blocks and lists of styles |
| Content per page | `render={({ pageNumber }) => …}` | `[render]="footer"` with `<ng-template #footer let-page>{{ page.pageNumber }}</ng-template>` |
| The page, deeper down | `usePage()` | `injectPage()` |
| What to render | `<Invoice invoice={invoice} />` | `{ component: Invoice, inputs: { invoice } }` |
| A template for `generate` | `Template: ComponentType<{ data }>` | `Template`: a component with the input `data` |

Templates need `schemas: [CUSTOM_ELEMENTS_SCHEMA]`, so that Angular passes the attributes and
property bindings of the elements on. A template renders once, synchronously: change detection
runs once, and listeners are ignored.

## In the browser

`@inkoxide/angular/browser` renders in a Web Worker, with
[`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) installed. `injectPdf` renders a
document whenever the signals it reads change:

```ts
import { Component, input } from '@angular/core';
import { injectPdf } from '@inkoxide/angular/browser';

import { Receipt } from './receipt';

@Component({
  selector: 'app-preview',
  template: `
    @if (pdf.safeUrl(); as src) {
      <iframe [src]="src" title="Receipt"></iframe>
      <a [href]="pdf.url()" download="receipt.pdf">Download</a>
    } @else {
      <p>Rendering…</p>
    }
  `,
})
export class Preview {
  readonly customer = input.required<string>();
  readonly pdf = injectPdf(() => ({ component: Receipt, inputs: { customer: this.customer(), lines: [] } }));
}
```

It gives `blob`, `url`, `safeUrl` (trusted for `<iframe [src]>`), `loading` and `error` as signals,
and `update()`. `renderToBlob` and `renderToStream` (a web stream) are there too. See
[In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/react/GUIDE.md#in-the-browser)
for where the engine and fonts come from.

## Licence

MIT
