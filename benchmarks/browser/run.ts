// The browser benchmark: the invoice and the report rendered by @inkoxide/render/browser
// (WebAssembly, in a Web Worker). Builds the page with Vite, serves it with `vite preview`, and
// opens it in a fresh browser context each time, so that nothing is cached between cold starts.
//   pnpm bench:browser [warm runs]       (after scripts/build-wasm.sh)
// Cold: from the start of navigation to the first invoice, the download of the library, its
// engine and fonts from localhost included; median of 3 page loads. Warm: median of N renders
// after one, in the page (10 for the report).
import { readFileSync } from 'node:fs';
import { cpus, totalmem } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync, constants } from 'node:zlib';

import { type BrowserType, chromium, firefox, webkit } from '@playwright/test';
import { build, preview } from 'vite';

import type { Result } from './measure.ts';

const runs = Number(process.argv[2] ?? 20);
const COLD_LOADS = 3;
const configFile = fileURLToPath(new URL('vite.config.ts', import.meta.url));
const dist = fileURLToPath(new URL('dist/', import.meta.url));

// The runner loads the workspace's TypeScript packages the config imports.
await build({ configFile, configLoader: 'runner', logLevel: 'warn' });
const server = await preview({
  configFile,
  configLoader: 'runner',
  preview: { port: 4180, strictPort: true },
});
const origin = 'http://localhost:4180';

const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;
const brotli = (bytes: Buffer) =>
  brotliCompressSync(bytes, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length;

/** What a page downloaded, with brotli, in bytes: the engine's WebAssembly, JavaScript, fonts. */
interface Download {
  wasm: number;
  js: number;
  fonts: number;
}

interface Load {
  result: Result;
  download: Download;
}

async function load(browser: Awaited<ReturnType<BrowserType['launch']>>): Promise<Load> {
  const context = await browser.newContext();
  const page = await context.newPage();
  const urls = new Set<string>();
  page.on('request', (r) => {
    if (r.url().startsWith(origin)) urls.add(new URL(r.url()).pathname);
  });
  // Worker requests go through the context.
  context.on('request', (r) => {
    if (r.url().startsWith(origin)) urls.add(new URL(r.url()).pathname);
  });
  await page.goto(`${origin}/index.html?runs=${runs}`);
  const result = (
    await page.waitForFunction(() => (globalThis as { bench?: unknown }).bench, null, {
      timeout: 600_000,
    })
  ).jsonValue() as Promise<Result | { error: string }>;
  const value = await result;
  await context.close();
  if ('error' in value) throw new Error(value.error);
  const download: Download = { wasm: 0, js: 0, fonts: 0 };
  for (const path of urls) {
    let bytes: Buffer;
    try {
      bytes = readFileSync(join(dist, path.slice(1)));
    } catch {
      continue; // Not a file of the build (a favicon).
    }
    const kind = path.endsWith('.wasm') ? 'wasm' : /\.(ttf|otf)$/.test(path) ? 'fonts' : 'js';
    download[kind] += brotli(bytes);
  }
  return { result: value, download };
}

const f = (ms: number) =>
  ms >= 100 ? `${Math.round(ms).toLocaleString('en-US')} ms` : `${ms.toFixed(1)} ms`;
const mb = (n: number) => `${(n / 1e6).toFixed(2)} MB`;

console.log(
  `${cpus()[0]?.model ?? 'unknown CPU'}, ${cpus().length} cores, ${Math.round(totalmem() / 2 ** 30)} GB, ` +
    `warm: median of ${runs} renders (10 for the report)\n`,
);
try {
  for (const [name, type] of [
    ['Chromium', chromium],
    ['WebKit', webkit],
    ['Firefox', firefox],
  ] as const) {
    let browser;
    try {
      browser = await type.launch();
    } catch (e) {
      console.log(`${name}: did not launch (${(e as Error).message.split('\n')[0]}); skipped\n`);
      continue;
    }
    const loads: Load[] = [];
    for (let k = 0; k < COLD_LOADS; k++) loads.push(await load(browser));
    console.log(`${name} ${browser.version()}\n`);
    console.log('| Document | Pages | Warm render | Longest main-thread task |');
    console.log('|---|---:|---:|---:|');
    for (const doc of ['invoice', 'report'] as const) {
      const r = loads[0]?.result.warm[doc];
      if (!r) continue;
      // Long tasks are those over 50 ms.
      const task =
        r.longestTaskMs === null ? '—' : r.longestTaskMs === 0 ? 'none over 50 ms' : f(r.longestTaskMs);
      console.log(`| ${doc} | ${r.pages} | ${f(r.ms)} | ${task} |`);
    }
    const cold = median(loads.map((l) => l.result.coldMs));
    const d = loads[0]?.download ?? { wasm: 0, js: 0, fonts: 0 };
    console.log(
      `\nFirst invoice after the page starts loading, from localhost (median of ${COLD_LOADS}): ${f(cold)}.\n` +
        `Downloaded, with brotli: ${mb(d.wasm + d.js + d.fonts)} (${mb(d.wasm)} WebAssembly, ` +
        `${mb(d.js)} JavaScript, ${mb(d.fonts)} fonts).\n`,
    );
    await browser.close();
  }
} finally {
  await new Promise<void>((resolve) => server.httpServer.close(() => resolve()));
}
