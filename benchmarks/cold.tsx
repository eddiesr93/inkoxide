// One render in a fresh process, from loading the library to the PDF bytes (bench.tsx runs it).
//   tsx cold.tsx <inkoxide|react-pdf> <invoice|report>
import { createElement } from 'react';

import invoice from './invoice.json' with { type: 'json' };
import type { Kit } from './report.tsx';

const [lib, doc] = process.argv.slice(2);
const t0 = performance.now();
let pdf: Uint8Array;
if (lib === 'inkoxide') {
  const ink = await import('@inkoxide/render');
  const el =
    doc === 'invoice'
      ? createElement((await import('./invoice-inkoxide.tsx')).Invoice, invoice)
      : createElement((await import('./report.tsx')).Report, { kit: ink as unknown as Kit });
  pdf = await ink.renderToBuffer(el, { validate: false });
} else {
  const rp = await import('@react-pdf/renderer');
  const report = await import('./report.tsx');
  report.setUpReactPdf(rp.Font);
  const el =
    doc === 'invoice'
      ? createElement((await import('./invoice-react-pdf.tsx')).Invoice, invoice)
      : createElement(report.Report, { kit: rp as unknown as Kit });
  pdf = await rp.renderToBuffer(el as Parameters<typeof rp.renderToBuffer>[0]);
}
console.log(JSON.stringify({ ms: performance.now() - t0, bytes: pdf.byteLength }));
