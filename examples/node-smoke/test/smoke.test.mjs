// Uses `inkoxide` exactly as a client app would: installed from the local
// package, fonts registered once, async render.
import test from 'node:test';
import assert from 'node:assert';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';

import { InkoxideError, registerFont, render, version } from 'inkoxide';
import { report } from '../report.mjs';

const repo = (p) => fileURLToPath(new URL(`../../../${p}`, import.meta.url));
const out = fileURLToPath(new URL('../out/', import.meta.url));
mkdirSync(out, { recursive: true });

registerFont('Inter-Regular', readFileSync(repo('fixtures/fonts/Inter-Regular.ttf')));
registerFont('Inter-Bold', readFileSync(repo('fixtures/fonts/Inter-Bold.ttf')));

function qpdfCheck(path) {
  try {
    execFileSync('qpdf', ['--version'], { stdio: 'ignore' });
  } catch {
    console.log('qpdf not installed; --check skipped');
    return;
  }
  execFileSync('qpdf', ['--check', path], { stdio: 'pipe' });
}

test('renders the invoice fixture', async () => {
  const v = version();
  console.log(`inkoxide ${v.engine} (schema ${v.schema}, ${v.gitSha})`);
  const doc = readFileSync(repo('fixtures/docs/invoice.json'));
  const t = performance.now();
  const pdf = await render(doc);
  console.log(`invoice: ${pdf.length} bytes in ${(performance.now() - t).toFixed(1)} ms`);
  assert.strictEqual(Buffer.from(pdf.subarray(0, 5)).toString('latin1'), '%PDF-');
  const path = `${out}invoice.pdf`;
  writeFileSync(path, pdf);
  qpdfCheck(path);
});

test('the event loop keeps ticking during a 100-page render', async () => {
  const doc = JSON.stringify(report());
  let ticks = 0;
  const timer = setInterval(() => ticks++, 10);
  const t = performance.now();
  const pdf = await render(doc, { threads: 1 });
  const ms = performance.now() - t;
  clearInterval(timer);
  console.log(`report: ${pdf.length} bytes in ${ms.toFixed(0)} ms, ${ticks} ticks of 10 ms`);
  writeFileSync(`${out}report.pdf`, pdf);
  qpdfCheck(`${out}report.pdf`);
  // A blocked loop would tick at most once. Allow for timer slack.
  assert.ok(ticks >= Math.floor(ms / 10 / 2), `${ticks} ticks in ${ms.toFixed(0)} ms`);
});

test('a validation error has code, path and message', async () => {
  const doc = {
    version: 1,
    fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
    pages: [
      {
        size: { width: 200, height: 200 },
        children: [{ type: 'text', runs: [{ text: 'x', font: 'Missing', size: 10 }] }],
      },
    ],
  };
  await assert.rejects(render(JSON.stringify(doc)), (e) => {
    assert.ok(e instanceof InkoxideError && e instanceof Error);
    assert.strictEqual(e.code, 'UNKNOWN_FONT');
    assert.strictEqual(e.path, 'pages[0].children[0].runs[0].font');
    assert.match(e.message, /Missing/);
    return true;
  });
});

test('a bad argument is INVALID_ARGUMENT', async () => {
  await assert.rejects(render('{}', { threads: 'many' }), { code: 'INVALID_ARGUMENT' });
});

test('anything else is INTERNAL, and the process survives', async () => {
  const ink = await import('inkoxide');
  assert.throws(() => ink.default.__testPanic(), { name: 'InkoxideError', code: 'INTERNAL', path: '' });
  // Still usable afterwards.
  assert.ok((await render(readFileSync(repo('fixtures/docs/invoice.json')))).length > 0);
});
