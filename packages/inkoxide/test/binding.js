// The binding the contract tests run against: this package, or `@inkoxide/wasm` with
// INKOXIDE_BINDING=wasm (its `test` script runs them that way). Both keep one contract.
export const wasm = process.env.INKOXIDE_BINDING === 'wasm';

const ink = wasm ? await import('../../wasm/index.js') : await import('../index.js');
if (wasm) await ink.init();

export default ink;
