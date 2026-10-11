// The README's benchmark: the same documents rendered by Inkoxide and by @react-pdf/renderer.
//   pnpm bench [warm runs]
// Warm: median of N renders after one warm-up, in one process. Inkoxide is measured with all
// cores (the default) and with threads: 1, since react-pdf renders on one thread. Cold: one
// render in a fresh process, including loading the library, median of 5 processes.
import { execFileSync } from 'node:child_process';
import { cpus, totalmem } from 'node:os';

import * as inkoxide from '@inkoxide/render';
import * as reactPdf from '@react-pdf/renderer';
import type { ReactElement } from 'react';

import { Invoice as InkoxideInvoice } from './invoice-inkoxide.tsx';
import { Invoice as ReactPdfTailwindInvoice } from './invoice-react-pdf-tailwind.tsx';
import { Invoice as ReactPdfInvoice } from './invoice-react-pdf.tsx';
import invoice from './invoice.json' with { type: 'json' };
import { type Kit, Report, setUpReactPdf } from './report.tsx';

const runs = Number(process.argv[2] ?? 20);
setUpReactPdf(reactPdf.Font);

const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;

async function warm(
  render: () => Promise<Uint8Array | Buffer>,
  n = runs,
): Promise<{ ms: number; pages: number }> {
  let pdf = await render();
  const times: number[] = [];
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    pdf = await render();
    times.push(performance.now() - t0);
  }
  return { ms: median(times), pages: inkoxide.countPages(new Uint8Array(pdf)) };
}

function cold(lib: string, doc: string): number {
  const times = Array.from({ length: 5 }, () => {
    const out = execFileSync('npx', ['tsx', 'cold.tsx', lib, doc], {
      cwd: import.meta.dirname,
      encoding: 'utf8',
    });
    return (JSON.parse(out.trim().split('\n').pop() as string) as { ms: number }).ms;
  });
  return median(times);
}

const ink = (el: ReactElement, threads?: number) => () =>
  inkoxide.renderToBuffer(el, threads ? { threads, validate: false } : { validate: false });
const rp = (el: ReactElement) => () =>
  reactPdf.renderToBuffer(el as Parameters<typeof reactPdf.renderToBuffer>[0]);

const inkReport = <Report kit={inkoxide as unknown as Kit} />;
const rpReport = <Report kit={reactPdf as unknown as Kit} />;
const reportRuns = Math.min(runs, 10);

const rows = [
  {
    doc: 'Invoice; react-pdf with react-pdf-tailwind',
    ink: await warm(ink(<InkoxideInvoice {...invoice} />)),
    one: await warm(ink(<InkoxideInvoice {...invoice} />, 1)),
    rp: await warm(rp(<ReactPdfTailwindInvoice {...invoice} />)),
  },
  {
    doc: 'Invoice; react-pdf with StyleSheet',
    ink: await warm(ink(<InkoxideInvoice {...invoice} />)),
    one: await warm(ink(<InkoxideInvoice {...invoice} />, 1)),
    rp: await warm(rp(<ReactPdfInvoice {...invoice} />)),
  },
  {
    doc: 'Report: 40 sections, 1,000 table rows',
    ink: await warm(ink(inkReport), reportRuns),
    one: await warm(ink(inkReport, 1), reportRuns),
    rp: await warm(rp(rpReport), reportRuns),
  },
];

const cpu = cpus()[0]?.model ?? 'unknown CPU';
console.log(
  `${cpu}, ${cpus().length} cores, ${Math.round(totalmem() / 2 ** 30)} GB, Node ${process.version}, ` +
    `@react-pdf/renderer 4.9.0, median of ${runs} warm renders (${reportRuns} for the report)\n`,
);
console.log('| Document | Pages | Inkoxide | Inkoxide, 1 thread | react-pdf | Faster (1 thread) |');
console.log('|---|---:|---:|---:|---:|---:|');
const f = (ms: number) =>
  ms >= 100 ? `${Math.round(ms).toLocaleString('en-US')} ms` : `${ms.toFixed(1)} ms`;
for (const r of rows) {
  console.log(
    `| ${r.doc} | ${r.ink.pages} | ${f(r.ink.ms)} | ${f(r.one.ms)} | ${f(r.rp.ms)} | ${Math.round(r.rp.ms / r.ink.ms)}× (${Math.round(r.rp.ms / r.one.ms)}×) |`,
  );
}

console.log('\nCold: one render in a fresh process, including loading the library (median of 5)\n');
console.log('| Document | Inkoxide | react-pdf |');
console.log('|---|---:|---:|');
for (const doc of ['invoice', 'report']) {
  console.log(`| ${doc} | ${f(cold('inkoxide', doc))} | ${f(cold('react-pdf', doc))} |`);
}
