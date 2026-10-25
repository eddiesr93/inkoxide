// The benchmark on the server: the invoice and the report rendered by Inkoxide.
//   pnpm bench [warm runs]
// Warm: median of N renders after one warm-up, in one process, with all cores (the default) and
// with threads: 1. Cold: one render in a fresh process, including loading the library, median of
// 5 processes.
import { execFileSync } from 'node:child_process';
import { cpus, totalmem } from 'node:os';

import * as inkoxide from '@inkoxide/react';
import type { ReactElement } from 'react';

import { Report } from './documents/report.tsx';
import invoice from './invoice.json' with { type: 'json' };
import { Invoice } from './invoice.tsx';

const runs = Number(process.argv[2] ?? 20);

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

function cold(doc: string): number {
  const times = Array.from({ length: 5 }, () => {
    const out = execFileSync('npx', ['tsx', 'cold.tsx', doc], {
      cwd: import.meta.dirname,
      encoding: 'utf8',
    });
    return (JSON.parse(out.trim().split('\n').pop() as string) as { ms: number }).ms;
  });
  return median(times);
}

const ink = (el: ReactElement, threads?: number) => () =>
  inkoxide.renderToBuffer(el, threads ? { threads, validate: false } : { validate: false });

const reportRuns = Math.min(runs, 10);

const rows = [
  {
    doc: 'Invoice',
    all: await warm(ink(<Invoice {...invoice} />)),
    one: await warm(ink(<Invoice {...invoice} />, 1)),
  },
  {
    doc: 'Report: 40 sections, 1,000 table rows',
    all: await warm(ink(<Report />), reportRuns),
    one: await warm(ink(<Report />, 1), reportRuns),
  },
];

const cpu = cpus()[0]?.model ?? 'unknown CPU';
console.log(
  `${cpu}, ${cpus().length} cores, ${Math.round(totalmem() / 2 ** 30)} GB, Node ${process.version}, ` +
    `median of ${runs} warm renders (${reportRuns} for the report)\n`,
);
console.log('| Document | Pages | All cores | 1 thread | Cold start |');
console.log('|---|---:|---:|---:|---:|');
const f = (ms: number) =>
  ms >= 100 ? `${Math.round(ms).toLocaleString('en-US')} ms` : `${ms.toFixed(1)} ms`;
for (const [r, doc] of rows.map((r, i) => [r, i === 0 ? 'invoice' : 'report'] as const)) {
  console.log(`| ${r.doc} | ${r.all.pages} | ${f(r.all.ms)} | ${f(r.one.ms)} | ${f(cold(doc))} |`);
}
console.log('\nCold start: one render in a fresh process, including loading the library (median of 5).');
