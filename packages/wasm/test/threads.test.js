// The module with threads (scripts/build-wasm.sh --threads) in Node, whose worker threads run the
// pool as Web Workers do in a page. The contract tests run against it too
// (INKOXIDE_BINDING=wasm-threads), and scripts/wasm-parity.mjs --threads compares its bytes.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import test from 'node:test';
import { Worker } from 'node:worker_threads';

const MODULE = new URL('../inkoxide-threads.wasm', import.meta.url);
// CI builds it, so there (`CI` set) its absence fails instead of skipping.
if (!existsSync(MODULE) && process.env.CI)
  throw new Error('packages/wasm/inkoxide-threads.wasm is missing: run scripts/build-wasm.sh --threads');
const skip = !existsSync(MODULE) && 'not built: scripts/build-wasm.sh --threads';
const fixture = (path) => readFileSync(new URL(`../../../fixtures/${path}`, import.meta.url));

// An instance of the package of its own: `init` takes effect once per instance.
let instances = 0;
const engine = () => import(`../index.js?${instances++}`);

/** An engine started with `init(...args)`, with the fonts and patterns the fixtures use. */
async function started(...args) {
  const ink = await engine();
  await ink.init(...args);
  for (const id of ['Inter-Regular', 'Inter-Bold']) ink.registerFont(id, fixture(`fonts/${id}.ttf`));
  await ink.loadHyphenation(['en']);
  return ink;
}

test('a render on the pool gives the bytes of a render on one thread', { skip }, async () => {
  const ink = await started(undefined, { threads: 3 });
  assert.equal(ink.threadCount(), 3);
  const doc = fixture('docs/table-split-header-repeat.json');
  const options = { creationDate: '2026-10-08T00:00:00Z' };
  assert.deepEqual(ink.renderSync(doc, options), ink.renderSync(doc, { ...options, threads: 1 }));
  assert.deepEqual(ink.renderSync(doc, { ...options, threads: 256 }), ink.renderSync(doc, options));
});

test('the module with threads runs without a pool, on one thread', { skip }, async () => {
  const ink = await started(MODULE, { threads: 1 });
  assert.equal(ink.threadCount(), 1);
  assert.ok(ink.renderSync(fixture('docs/invoice.json')).length > 0);
  assert.throws(() => ink.renderSync(fixture('docs/invoice.json'), { threads: 2 }), {
    code: 'INVALID_ARGUMENT',
    path: 'options.threads',
  });
});

test('init refuses a pool size out of range', { skip }, async () => {
  const ink = await engine();
  for (const threads of [0, 257, 1.5, NaN]) {
    await assert.rejects(ink.init(undefined, { threads }), {
      code: 'INVALID_ARGUMENT',
      path: 'options.threads',
    });
  }
});

test('after a trap on the render thread, a new engine starts its own pool', { skip }, async () => {
  const ink = await started(undefined, { threads: 2 });
  const pdf = ink.renderSync(fixture('docs/invoice.json'));
  assert.throws(() => ink.InkoxideError.__testPanic(), { code: 'INTERNAL' });
  // Until the new pool has started, renders run on one thread, those that ask for more too. A
  // device that cannot reserve the memory again gets a new engine that may grow less far.
  const Memory = WebAssembly.Memory;
  const maxima = [];
  WebAssembly.Memory = function (desc) {
    maxima.push(desc.maximum);
    if (desc.maximum > 4096) throw new RangeError('out of memory');
    return new Memory(desc);
  };
  try {
    assert.deepEqual(ink.renderSync(fixture('docs/invoice.json'), { threads: 2 }), pdf);
  } finally {
    WebAssembly.Memory = Memory;
  }
  assert.deepEqual(maxima, [32768, 16384, 8192, 4096]);
  // Meanwhile a value out of range is refused as before, and validate takes what render takes.
  for (const threads of [1.5, 257, Infinity]) {
    assert.throws(() => ink.renderSync(fixture('docs/invoice.json'), { threads }), {
      code: 'INVALID_ARGUMENT',
      path: 'options.threads',
    });
  }
  assert.deepEqual(ink.validate(fixture('docs/invoice.json'), undefined, undefined, { threads: 2 }), []);
  for (let i = 0; i < 100 && ink.threadCount() === 1; i++) await new Promise((r) => setTimeout(r, 50));
  assert.equal(ink.threadCount(), 2);
  assert.deepEqual(ink.renderSync(fixture('docs/invoice.json')), pdf);
});

test('a trap on a pool thread is reported on the trap channel', { skip }, async () => {
  // The thread that waits for the pool cannot hear of it: a worker renders, and this thread,
  // which can end it, listens.
  const name = `inkoxide-trap-${process.pid}`;
  const channel = new BroadcastChannel(name);
  const trapped = new Promise((resolve) => {
    channel.onmessage = ({ data }) => resolve(data);
  });
  const worker = new Worker(
    `const { parentPort, workerData } = require('node:worker_threads');
     import(workerData.url).then(async (ink) => {
       await ink.init(undefined, { threads: 2, trapChannel: workerData.name });
       parentPort.postMessage('started');
       ink.InkoxideError.__testPoolPanic();
     });`,
    { eval: true, workerData: { url: new URL('../index.js?trap', import.meta.url).href, name } },
  );
  try {
    assert.equal(await new Promise((r) => worker.once('message', r)), 'started');
    const { message } = await trapped;
    assert.match(message, /unreachable/);
  } finally {
    channel.close();
    await worker.terminate();
  }
});
