// One render in a fresh process, from loading the library to the PDF bytes (bench.tsx runs it).
//   tsx cold.tsx <invoice|report>
import { createElement } from 'react';

import invoice from './invoice.json' with { type: 'json' };

const [doc] = process.argv.slice(2);
const t0 = performance.now();
const ink = await import('@inkoxide/render');
const el =
  doc === 'invoice'
    ? createElement((await import('./invoice.tsx')).Invoice, invoice)
    : createElement((await import('./documents/report.tsx')).Report);
const pdf = await ink.renderToBuffer(el, { validate: false });
console.log(JSON.stringify({ ms: performance.now() - t0, bytes: pdf.byteLength }));
