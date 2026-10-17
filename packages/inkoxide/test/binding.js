// The binding the contract tests run against: this package, or `@inkoxide/wasm` with
// INKOXIDE_BINDING=wasm (its `test` script runs them that way), or its module with threads, on a
// pool of 3, with INKOXIDE_BINDING=wasm-threads. They all keep one contract.
export const threads = process.env.INKOXIDE_BINDING === 'wasm-threads';
export const wasm = process.env.INKOXIDE_BINDING === 'wasm' || threads;

const ink = wasm ? await import('../../wasm/index.js') : await import('../index.js');
if (wasm) {
  await ink.init(undefined, threads ? { threads: 3 } : undefined);
  if (ink.threadCount() !== (threads ? 3 : 1)) throw new Error(`${ink.threadCount()} threads`);
  // renderSync cannot fetch the built-in hyphenation patterns, and the tests' documents
  // hyphenate English.
  await ink.loadHyphenation(['en']);
}

export default ink;
