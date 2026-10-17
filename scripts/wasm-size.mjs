#!/usr/bin/env node
// Prints the download size of the WebAssembly engine (packages/wasm/inkoxide.wasm, from
// scripts/build-wasm.sh) and fails when it is over BUDGET with brotli. The budget is a little over
// 1.31 MB, the build without its hyphenation patterns, which it fetches as documents need them;
// variable fonts (subsetter's instancer) took it from 1.18 MB. Raise it deliberately, in the pull
// request that needs it. In CI the sizes also go to the job summary.
//   node scripts/wasm-size.mjs
import { appendFileSync, readFileSync } from 'node:fs';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const BUDGET = 1_450_000;

const wasm = readFileSync(new URL('../packages/wasm/inkoxide.wasm', import.meta.url));
const brotli = brotliCompressSync(wasm, {
  params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: wasm.length },
}).length;
const gzip = gzipSync(wasm, { level: 9 }).length;
const mb = (n) => `${(n / 1e6).toFixed(2)} MB`;

const table = [
  '| inkoxide.wasm | Size |',
  '|---|---|',
  `| raw | ${mb(wasm.length)} |`,
  `| gzip -9 | ${mb(gzip)} |`,
  `| brotli 11 | ${mb(brotli)} (budget ${mb(BUDGET)}) |`,
].join('\n');
console.log(table);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${table}\n`);
if (brotli > BUDGET) {
  console.error(`wasm-size: ${mb(brotli)} with brotli is over the budget of ${mb(BUDGET)}`);
  process.exit(1);
}
