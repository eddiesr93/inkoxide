#!/usr/bin/env node
// Prints the download size of the WebAssembly engine (packages/wasm/inkoxide.wasm, from
// scripts/build-wasm.sh, and inkoxide-threads.wasm, from scripts/build-wasm.sh --threads, when it
// is built) and fails when one is over BUDGET with brotli. The budget is a little over 1.37 MB,
// the build without its hyphenation patterns, which it fetches as documents need them, with
// variable fonts (subsetter's instancer), and with the Unicode tables of SASLprep (NFKC), which
// prepares passwords (about 50 kB), PDF/UA-2 and the tagging of the parts of SVG images. The
// module with threads is within a few kilobytes of the other, and has the same budget. Raise it deliberately, in the pull request that needs it.
// In CI the sizes also go to the job summary, and a module not built fails.
//   node scripts/wasm-size.mjs
import { appendFileSync, existsSync, readFileSync } from 'node:fs';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const BUDGET = 1_530_000;

const mb = (n) => `${(n / 1e6).toFixed(2)} MB`;
let failed = false;
for (const name of ['inkoxide.wasm', 'inkoxide-threads.wasm']) {
  const path = new URL(`../packages/wasm/${name}`, import.meta.url);
  if (name !== 'inkoxide.wasm' && !existsSync(path)) {
    if (process.env.CI) {
      console.error(`${name} is not built: run scripts/build-wasm.sh --threads`);
      failed = true;
    } else {
      console.log(`${name} is not built; not measured`);
    }
    continue;
  }
  const wasm = readFileSync(path);
  const brotli = brotliCompressSync(wasm, {
    params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: wasm.length },
  }).length;
  const gzip = gzipSync(wasm, { level: 9 }).length;
  const table = [
    `| ${name} | Size |`,
    '|---|---|',
    `| raw | ${mb(wasm.length)} |`,
    `| gzip -9 | ${mb(gzip)} |`,
    `| brotli 11 | ${mb(brotli)} (budget ${mb(BUDGET)}) |`,
  ].join('\n');
  console.log(table);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${table}\n\n`);
  if (brotli > BUDGET) {
    console.error(`wasm-size: ${name} is ${mb(brotli)} with brotli, over the budget of ${mb(BUDGET)}`);
    failed = true;
  }
}
if (failed) process.exit(1);
