<div align="center">

# Inkoxide

**A PDF layout engine for Node.js, written in Rust.**

Typeset documents from React components, or from a JSON document tree.

[![CI](https://github.com/eddiesr93/inkoxide/actions/workflows/ci.yml/badge.svg)](https://github.com/eddiesr93/inkoxide/actions/workflows/ci.yml)
[![@inkoxide/render on npm](https://img.shields.io/npm/v/@inkoxide/render?color=4f46e5&label=%40inkoxide%2Frender)](https://www.npmjs.com/package/@inkoxide/render)
[![inkoxide on npm](https://img.shields.io/npm/v/inkoxide?color=0f766e&label=inkoxide)](https://www.npmjs.com/package/inkoxide)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-0ea5e9)](LICENSE)

[Getting started](#getting-started) · [Documentation](#documentation) · [Packages](#packages) · [Performance](#performance) · [Contributing](#contributing)

</div>

<p align="center">
  <a href="https://inkoxide.dev/playground/invoice"><img src="https://inkoxide.dev/demo/playground.gif" alt="The playground: the invoice's title is edited in the source, the PDF renders again in the browser, and Layout shows the boxes the engine laid out, with their sizes in points." width="860"></a>
  <br>
  <sub>Edit a document in the <a href="https://inkoxide.dev/playground/invoice">playground</a>: it renders again in the browser, and Layout shows the boxes the engine laid out.</sub>
</p>

<table>
<tr>
<td align="center" valign="top" width="33%"><img src="packages/react/media/invoice.png" alt="Invoice PDF: logo, sender and client addresses, a four-row line-item table, subtotal, tax and a highlighted total due." width="260"><br><sub><b>Invoice</b></sub></td>
<td align="center" valign="top" width="33%"><img src="packages/react/media/report.png" alt="First page of a sales report PDF: three KPI cards, a monthly revenue bar chart and the start of a regional sales table." width="260"><br><sub><b>Multi-page report</b></sub></td>
<td align="center" valign="top" width="33%"><img src="packages/react/media/certificate.png" alt="Landscape certificate PDF: a double border, Certificate of Completion, the recipient name, two signature lines and a gold seal." width="260"><br><sub><b>Certificate</b></sub></td>
</tr>
<tr>
<td align="center" valign="top" width="33%"><img src="packages/react/media/letter.png" alt="Business letter PDF: a letterhead, the date and address block, four justified paragraphs and a signature." width="260"><br><sub><b>Business letter</b></sub></td>
<td align="center" valign="top" width="33%"><img src="packages/react/media/resume.png" alt="Résumé PDF: a dark sidebar with initials, contact details and skill tags, and a main column with profile, experience and education." width="260"><br><sub><b>Résumé</b></sub></td>
<td align="center" valign="top" width="33%"><img src="packages/react/media/form.png" alt="Membership application PDF with text fields, a plan select, two radio buttons and two consent checkboxes." width="260"><br><sub><b>Fillable form</b></sub></td>
</tr>
</table>

Inkoxide does the typesetting of a PDF: text shaping, line breaking, layout, pagination and PDF
output, in native code. Documents are written as React components with
[`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render), styled with Tailwind
classes or style objects, or built as JSON by any program and rendered with
[`inkoxide`](https://www.npmjs.com/package/inkoxide). Render times, on the server and in the
browser, are in [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Features

- **Typesetting.** OpenType shaping, justification, hyphenation with TeX patterns, orphans and
  widows, font fallback and right-to-left text.
- **Layout.** Flexbox and CSS grid, absolute positioning, transforms, borders and radii.
- **Pagination.** Tables that split across pages with repeating header rows, headers and
  footers, page numbers, keep-together blocks and page breaks.
- **Content.** JPEG, PNG, GIF, WebP and SVG images, vector drawing, links, bookmarks,
  fillable form fields, text notes and file attachments.
- **Output.** Font subsetting (TrueType and CFF), variable fonts as static instances, PDF/A-1b,
  2b, 2a, 3b and 3a, tagged PDF and PDF/UA-1, AES-256 encryption with passwords and permissions,
  parallel rendering across pages, and byte-identical output for identical input.
- **Validation.** Every document is checked against a JSON Schema, with the path of each error.
  In React, errors carry the component stack.

## Getting started

### With React

```sh
npm install @inkoxide/render
```

To style with Tailwind classes (`className`), as the example does, add:

```sh
npm install @inkoxide/tailwind
```

Save this as `receipt.tsx`:

```tsx
/** @jsxRuntime automatic */
import { Document, Page, renderToFile, Text, View } from '@inkoxide/render';

function Receipt({ customer, lines }: { customer: string; lines: { name: string; amount: number }[] }) {
  const total = lines.reduce((sum, l) => sum + l.amount, 0);
  return (
    <Document title={`Receipt for ${customer}`}>
      <Page size="A6" className="p-8 text-xs text-gray-900">
        <Text className="text-lg font-bold">Receipt</Text>
        <Text className="mb-4 text-gray-500">for {customer}</Text>
        {lines.map((l) => (
          <View key={l.name} className="flex-row justify-between border-b border-gray-200 py-1.5">
            <Text>{l.name}</Text>
            <Text>${l.amount.toFixed(2)}</Text>
          </View>
        ))}
        <View className="mt-3 flex-row justify-between font-bold">
          <Text>Total</Text>
          <Text>${total.toFixed(2)}</Text>
        </View>
      </Page>
    </Document>
  );
}

renderToFile(
  <Receipt
    customer="Ada Lovelace"
    lines={[
      { name: 'Espresso', amount: 3.2 },
      { name: 'Almond croissant', amount: 4.5 },
      { name: 'Sparkling water', amount: 2 },
    ]}
  />,
  'receipt.pdf',
);
```

<img src="packages/react/media/receipt.png" alt="The resulting receipt.pdf: an A6 page with 'Receipt for Ada Lovelace', three items with prices and a bold total of $9.70." width="280">

Run it with `npx tsx receipt.tsx`. The fonts (Inter, Noto Sans, Roboto Mono) ship with the
package, and `@inkoxide/tailwind` brings the Tailwind compiler. The six documents above, with
their code, are in [`examples/`](examples) and in the
[@inkoxide/react README](packages/react/README.md).

### With JSON

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { registerFont, render } from 'inkoxide';

registerFont('Inter-Bold', readFileSync('Inter-Bold.ttf'));

const doc = {
  version: 1,
  fonts: [{ id: 'Inter-Bold', family: 'Inter', weight: 700 }],
  pages: [
    {
      size: { width: 595.28, height: 841.89 },
      margin: { top: 56, right: 56, bottom: 56, left: 56 },
      children: [{ type: 'text', runs: [{ text: 'Hello, PDF', font: 'Inter-Bold', size: 28 }] }],
    },
  ],
};

writeFileSync('hello.pdf', await render(JSON.stringify(doc)));
```

The document tree is a fully resolved description of boxes, text, images and tables, as JSON or
MessagePack. [`@inkoxide/schema`](packages/schema) has its TypeScript types and JSON Schema.

## Documentation

| Topic | Document |
|---|---|
| Writing documents in React | [Guide](packages/react/GUIDE.md): components, styling, fonts, images, pagination, tables, forms, API, errors |
| Tailwind classes | [Supported utilities](docs/TAILWIND_SUPPORT.md) |
| Using the engine from Node | [Client integration](docs/CLIENT_INTEGRATION.md): fonts, images, errors, threading, determinism |
| The engine API | [API reference](docs/API.md): the Rust and Node APIs, options and error codes |
| The document tree | [Schema reference](docs/SCHEMA.md) and [example documents](packages/schema/EXAMPLES.md) |
| How it works | [Architecture](docs/ARCHITECTURE.md) |
| Testing and benchmarks | [Testing](docs/TESTING.md) and [benchmarks](docs/BENCHMARKS.md) |

## Packages

| Package | What it is |
|---|---|
| [`@inkoxide/render`](packages/render) | React components to PDF: `renderToBuffer`, `renderToFile`, `Font.register`, bundled fonts |
| [`@inkoxide/react`](packages/react) | The React components and reconciler that build document trees |
| [`@inkoxide/tailwind`](packages/tailwind) | Optional: Tailwind class strings (`className`) to engine styles, with the Tailwind v4 compiler, and a build plugin that puts a table of your classes in the browser bundle in its place |
| [`@inkoxide/style`](packages/style) | Style values: CSS lengths, colours and transforms read into engine styles |
| [`inkoxide`](packages/inkoxide) | The engine for Node.js: a JSON or MessagePack document tree in, PDF bytes out |
| [`@inkoxide/wasm`](packages/wasm) | The engine as WebAssembly, for browsers: a JSON document tree in, PDF bytes out |
| [`@inkoxide/schema`](packages/schema) | TypeScript types and JSON Schema of the document tree |
| [`inkoxide` crate](crates/core) | The engine as a Rust library |

## Performance

Layout and PDF writing run in parallel across pages, and fonts are parsed once per process.
[docs/BENCHMARKS.md](docs/BENCHMARKS.md) has the render times, on the server and in the browser,
with the machines and the methodology.

## Platforms

Prebuilt binaries; npm installs the one for the current platform. The packages are ES modules.
The Node packages declare Node.js 22 or later (`engines`), and CI tests them on Node 22.

| OS | Architectures |
|---|---|
| macOS | arm64, x64 |
| Linux | x64 (glibc 2.35+ and musl), arm64 (glibc 2.35+) |
| Windows | x64 |

In browsers (current Chrome, Edge, Firefox and Safari), the engine is WebAssembly
(`@inkoxide/wasm`), and `@inkoxide/render/browser` renders in a Web Worker; see
[In the browser](packages/react/GUIDE.md#in-the-browser).

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to build
the project, run the tests and propose a change, and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
how the code is organised. Please report security issues privately, as described in
[SECURITY.md](SECURITY.md). Changes are listed in [CHANGELOG.md](CHANGELOG.md).

## Licence

[MIT](LICENSE). The bundled fonts are under the SIL Open Font License
(`fixtures/fonts/*-OFL.txt`).
