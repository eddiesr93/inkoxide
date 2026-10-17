// A thread of the engine's pool (index.js, `init(source, { threads })`), in a Web Worker or, in
// Node, a worker thread. It instantiates the module with threads on the memory the engine shares,
// sets its stack and thread-locals in the block the engine gave it (crates/wasm/src/threads.rs),
// says it is ready, and runs one of the pool's threads until the engine ends.
const nodeThreads = globalThis.process?.getBuiltinModule?.('node:worker_threads');
const parent = nodeThreads ? nodeThreads.parentPort : globalThis;

function start({ module, memory, at, trapChannel }) {
  let e;
  try {
    e = new WebAssembly.Instance(module, { env: { memory } }).exports;
    // Before any Rust code runs here.
    e.__stack_pointer.value = at;
    e.__wasm_init_tls(at);
  } catch (err) {
    parent.postMessage({ error: String(err?.message ?? err) });
    return;
  }
  parent.postMessage({ ready: true });
  try {
    e.ink_thread_run();
  } catch (err) {
    // A panic, or memory running out. The render waiting for this thread never ends, and the
    // thread that runs it cannot hear about it: whoever can end that thread is told.
    if (trapChannel) {
      const channel = new BroadcastChannel(trapChannel);
      channel.postMessage({ message: String(err?.message ?? err) });
      channel.close();
    }
    throw err;
  }
}

if (nodeThreads) parent.once('message', start);
else globalThis.onmessage = ({ data }) => start(data);
