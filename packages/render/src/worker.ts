/**
 * The Web Worker the browser entry renders in (browser-host.ts): it runs `@inkoxide/wasm` and
 * answers the host's calls, one message each, in order. A request is `{ id, op, args }`; the answer is
 * `{ id, value }`, or `{ id, error }` with the error's name, code, path and message. A PDF is
 * transferred, not copied.
 */
import * as engine from '@inkoxide/wasm';

/** What the host may call, by name. */
const OPS = {
  init: engine.init,
  registerFont: engine.registerFont,
  registerHyphenationPatterns: engine.registerHyphenationPatterns,
  validate: engine.validate,
  render: engine.render,
  version: engine.version,
  clearCaches: engine.clearCaches,
} as const;

export type Op = keyof typeof OPS;

export interface Request {
  id: number;
  op: Op;
  args: unknown[];
}

export type Response =
  | { id: number; value: unknown }
  | { id: number; error: { name: string; code?: string; path?: string; message: string } };

const scope = globalThis as unknown as {
  onmessage: ((e: MessageEvent<Request>) => void) | null;
  postMessage(message: Response, transfer?: Transferable[]): void;
};

/** Answers one request. */
async function answer({ id, op, args }: Request): Promise<void> {
  try {
    const f = OPS[op] as (...a: unknown[]) => unknown;
    const value = await f(...args);
    scope.postMessage({ id, value }, value instanceof Uint8Array ? [value.buffer as ArrayBuffer] : []);
  } catch (e) {
    const {
      name = 'Error',
      code,
      path,
      message = String(e),
    } = (e ?? {}) as Record<string, string | undefined>;
    scope.postMessage({ id, error: { name, code, path, message } });
  }
}

// One request at a time, in the order they came: a new worker is set up (`init`, then the fonts)
// before it renders.
let queue = Promise.resolve();
scope.onmessage = ({ data }) => {
  queue = queue.then(() => answer(data));
};
