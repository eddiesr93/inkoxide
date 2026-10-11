<div align="center">

# Inkoxide

**Fast PDFs from React components, or from JSON.**

A native PDF layout engine written in Rust, for Node.js. Write `Document`, `Page`, `View` and
`Text` with Tailwind classes or react-pdf style objects, and render them **14–50× faster than
react-pdf**, even on a single thread.

[![@inkoxide/render on npm](https://img.shields.io/npm/v/@inkoxide/render?color=4f46e5&label=%40inkoxide%2Frender)](https://www.npmjs.com/package/@inkoxide/render)
[![inkoxide on npm](https://img.shields.io/npm/v/inkoxide?color=0f766e&label=inkoxide)](https://www.npmjs.com/package/inkoxide)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-0ea5e9)](#licence)
[![Node.js 22 or later](https://img.shields.io/badge/node-%E2%89%A522-339933)](#quick-start)

[Quick start](#quick-start) · [Examples](#examples) · [From react-pdf](#coming-from-react-pdf) · [Performance](#performance) · [The engine](#the-engine) · [Development](#development)

</div>

<a id="examples"></a>

<table>
<tr>
<td align="center" valign="top" width="33%"><img src="docs/media/invoice.png" alt="Invoice PDF: logo, sender and client addresses, a four-row line-item table, subtotal, tax and a highlighted total due." width="260"><br><sub><b>Invoice</b></sub></td>
<td align="center" valign="top" width="33%"><img src="docs/media/report.png" alt="First page of a sales report PDF: three KPI cards, a monthly revenue bar chart and the start of a regional sales table." width="260"><br><sub><b>Multi-page report</b></sub></td>
<td align="center" valign="top" width="33%"><img src="docs/media/certificate.png" alt="Landscape certificate PDF: a double border, Certificate of Completion, the recipient name, two signature lines and a gold seal." width="260"><br><sub><b>Certificate</b></sub></td>
</tr>
<tr>
<td align="center" valign="top" width="33%"><img src="docs/media/letter.png" alt="Business letter PDF: a letterhead, the date and address block, four justified paragraphs and a signature." width="260"><br><sub><b>Business letter</b></sub></td>
<td align="center" valign="top" width="33%"><img src="docs/media/resume.png" alt="Résumé PDF: a dark sidebar with initials, contact details and skill tags, and a main column with profile, experience and education." width="260"><br><sub><b>Résumé</b></sub></td>
<td align="center" valign="top" width="33%"><img src="docs/media/form.png" alt="Membership application PDF with text fields, a plan select, two radio buttons and two consent checkboxes." width="260"><br><sub><b>Fillable form</b></sub></td>
</tr>
</table>

<p align="center"><sub>Each picture is the first page of a PDF rendered by Inkoxide from a single React file, unedited. The code of all six is in the <a href="https://www.npmjs.com/package/@inkoxide/render">@inkoxide/render README</a>.</sub></p>

## Why

- **Fast.** A one-page invoice renders in about 1.3 ms and a 43-page report in about 33 ms:
  14–50× faster than react-pdf on one CPU thread, up to 68× with all cores.
  [How it was measured](#performance).
- **Familiar.** `Document`, `Page`, `View`, `Text`, `Image`, `StyleSheet.create`, `Font.register`
  and style objects in points work as in react-pdf, and Tailwind classes work as on the web.
- **Made for print.** Real tables whose header repeats on every page, headers and footers that
  line up with the page, page numbers, blocks kept together, hyphenation, fillable forms and
  PDF/A.
- **Errors that point at your code.** A `View` inside a `Text`, an unknown class or an unknown
  style key fails with the component that caused it, instead of a blank page.

## Two ways to use it

| | Package | You write | Start here |
|---|---|---|---|
| **React** | [`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render) | Components, styled with Tailwind classes or style objects | [Quick start](#quick-start) |
| **Engine** | [`inkoxide`](https://www.npmjs.com/package/inkoxide) | A JSON document tree, from any language that can call Node | [The engine](#the-engine) |

`@inkoxide/render` is built on `inkoxide`: it turns your components into the engine's JSON and
ships the fonts, so most people want it.

## Quick start

**1. Install** the package and React (plus `tsx`, to run TypeScript directly):

```sh
npm install @inkoxide/render react
npm install -D tsx @types/react
```

**2. Save** this as `receipt.tsx`:

```tsx
import { Document, Page, renderToFile, Text, View } from '@inkoxide/render';

interface Line {
  name: string;
  amount: number;
}

export default function Receipt({ customer, lines }: { customer: string; lines: Line[] }) {
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

await renderToFile(
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

**3. Run it:**

```sh
npx tsx receipt.tsx
```

<img src="docs/media/receipt.png" alt="The resulting receipt.pdf: an A6 page with 'Receipt for Ada Lovelace', three items with prices and a bold total of $9.70." width="300">

A PDF is a React component and its data are props: there is nothing to register and no build
step. In TypeScript, set `"jsx": "react-jsx"` in `tsconfig.json`. In a server, call
`renderToBuffer(<Receipt … />)` and send the bytes.

The [@inkoxide/render README](https://www.npmjs.com/package/@inkoxide/render) has the six
examples above with their code, the API, the error codes and an FAQ. Its guide covers the
components, styling, fonts, images, pagination, tables and forms.

## Coming from react-pdf

Change the import. `Document`, `Page`, `View`, `Text`, `Image`, `StyleSheet.create`,
`Font.register`, style objects, `wrap={false}`, `break`, `minPresenceAhead` and
`render={({ pageNumber, totalPages }) => …}` work as they are. The differences:

- **Repeating content.** A footer is `<Footer>` (or `<Header>`), not a `View` with `fixed`, which
  throws and points you there.
- **Links.** `<Link src>` becomes `href` on `Text` or `View`.
- **Tables.** They can be a real `<Table>`, whose header repeats on every page.
- **Not available:** browser rendering (`<PDFViewer>`, `usePDF`), `renderToStream`, colour emoji
  and `<Note>`.

The [migration table](https://www.npmjs.com/package/@inkoxide/render) in the package README
goes row by row.

## Performance

The same documents rendered by both libraries, with the same font, page size and page count:

| Document | Pages | @inkoxide/render | On 1 thread | @react-pdf/renderer 4.9 | Faster |
|---|---:|---:|---:|---:|---:|
| Invoice (react-pdf with `react-pdf-tailwind`) | 1 | 1.3 ms | 1.0 ms | 19.2 ms | 19× |
| Invoice (react-pdf with `StyleSheet`) | 1 | 1.2 ms | 1.3 ms | 17.6 ms | 14× |
| Report: 40 sections, 1,000 table rows | 43 | 33 ms | 44 ms | 2,214 ms | 50× (68× on all cores) |

<sub>Apple M4 Pro (14 cores), 24 GB, Node 24.14, macOS. Median of 20 renders after a warm-up (10
for the report), in one process. "Faster" compares react-pdf with Inkoxide on one thread, since
react-pdf renders on one.</sub>

The first render in a new process includes loading the library: 164 ms for the invoice against
231 ms for react-pdf, and 191 ms against 2,407 ms for the report.

The engine on its own renders a 98-page report with 40 tables in about 130 ms on 4 cores
([docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

## The engine

`inkoxide` takes a declarative, fully resolved document tree (boxes, text, images, tables) as
JSON or MessagePack and returns PDF bytes:

```
render(document_tree, options) -> pdf_bytes
```

It paginates properly: tables split with repeated headers, blocks stay together, headers and
footers repeat, and page numbers are filled in. Use it directly when you generate documents
from another language, or want full control of the tree:

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

`@inkoxide/schema` has the TypeScript types and the JSON Schema of the tree.
[docs/CLIENT_INTEGRATION.md](docs/CLIENT_INTEGRATION.md) covers fonts, images, errors,
threading, determinism and hyphenation.

### Status

**v1 is feature-complete and ready for integration.** The public API is frozen (ADR-042), so
changes from now on are additive. npm releases (`inkoxide`, its per-platform binaries and
`@inkoxide/schema`) are built by the manually dispatched `.github/workflows/release.yml`
(ADR-051), with the macOS bindings optionally built on a Mac (ADR-052); nothing goes to
crates.io.

What works:

- **Input:** schema validation of JSON and MessagePack, with error paths.
- **Layout:** flex and grid layout.
- **Text:** shaping, wrapping, justification, hyphenation (including runtime TeX patterns), font
  fallback, and selectable text.
- **Pagination:** splitting, keep-together, forced breaks, headers and footers, page numbers.
- **Tables:** spans, collapsed or separate borders, and repeated header rows.
- **Images:** JPEG passthrough, PNG with alpha, GIF and WebP, SVG as vectors.
- **Output:** font subsetting (TrueType and CFF), links and bookmarks, form fields, PDF/A-2b, and
  parallel rendering.

The output is deterministic for a fixed creation date.

### Platforms

| Platform | Status |
|---|---|
| Linux x86_64 (glibc) | ✅ `scripts/ci.sh` green |
| macOS arm64 | ✅ `scripts/ci.sh` green, visual diff at the default threshold ([docs/TESTING.md](docs/TESTING.md#macos-apple-silicon)) |
| Linux x86_64 (musl), Linux arm64 (glibc) | ✅ Node binding tests pass in Docker before each release (ADR-052) |
| macOS x86_64 | ✅ Node binding tests pass under Rosetta before each release (ADR-052) |
| Windows x86_64 | ✅ Node binding tests pass on GitHub Actions before each release (ADR-053) |

## Development

You need Rust (rustup picks up 1.97.0 from `rust-toolchain.toml`), Node.js 22 or later, pnpm,
Python 3 with `jsonschema`, `numpy` and `Pillow`, and poppler (`pdftoppm`) for the visual diff.
Optionally, install `qpdf`, `cargo-deny`, and nightly Rust with `cargo-fuzz`; `ci.sh` skips
those checks when the tools are missing.

```sh
git clone https://github.com/eddiesr93/inkoxide && cd inkoxide
scripts/ci.sh                                          # build and test everything
scripts/build-node.sh                                  # build the Node package in npm/
(cd examples/node-smoke && pnpm install && pnpm test)  # use it like a client app
```

`examples/node-smoke` installs `inkoxide` and `@inkoxide/schema` with `file:` dependencies,
exactly as a client would.

```sh
scripts/ci.sh --quick      # fmt + tests + schema
scripts/bench-headline.sh  # headline benchmark (median of 10)
```

### Command line

```sh
cargo run --release -p inkoxide-cli -- validate fixtures/docs/invoice.json --fonts fixtures/fonts
cargo run --release -p inkoxide-cli -- render fixtures/docs/invoice.json --fonts fixtures/fonts -o invoice.pdf
```

### Repository layout

- `schema/document.v1.json`: the input contract (JSON Schema 2020-12).
- `crates/core`: the engine (crate `inkoxide`).
- `crates/cli`: the `inkoxide` command-line tool.
- `crates/node`: the napi-rs binding. `npm/` is the JS package around it.
- `npm-schema/`: `@inkoxide/schema`, with TypeScript types and the bundled JSON Schema.
- `fixtures/`: input documents, OFL fonts, images and expected renders.
- `examples/node-smoke`: a client-style smoke test.
- `docs/media/`: the README's example pictures, rendered by `@inkoxide/render`.

### Documentation

- [docs/CLIENT_INTEGRATION.md](docs/CLIENT_INTEGRATION.md): how to add Inkoxide to a Node app
  (fonts, images, errors, threading, determinism, hyphenation).
- [docs/API.md](docs/API.md): the Rust and Node APIs side by side, with options and error codes.
- [schema/EXAMPLES.md](schema/EXAMPLES.md): complete example documents.
  [docs/SCHEMA.md](docs/SCHEMA.md) is the full schema reference.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/DECISIONS.md](docs/DECISIONS.md): design
  and the decision log.
- [docs/TESTING.md](docs/TESTING.md): test suites, fixtures and fuzzing.
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md): performance numbers.
- [docs/DEPENDENCY_WATCH.md](docs/DEPENDENCY_WATCH.md): the pending rustybuzz/ttf-parser swap
  (`scripts/dep-watch.sh`).

## Licence

MIT. The fixture fonts are under the SIL Open Font License (see `fixtures/fonts/*-OFL.txt`).
