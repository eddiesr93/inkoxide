/**
 * The engine host in a browser: `@inkoxide/wasm`, in a Web Worker (worker.ts) so that the page
 * keeps responding while it renders, and the bundled assets fetched as documents need them.
 * Importing it installs it. Without a worker (`worker: false`, or where there is no `Worker`),
 * the engine runs on the calling thread.
 */
import * as wasm from '@inkoxide/wasm';
import type { WasmSource } from '@inkoxide/wasm';

import { type EngineHost, type EngineIssue, type EngineVersion, setEngineHost } from './host.ts';
import type { Op, Request, Response } from './worker.ts';

export interface BrowserOptions {
  /** The engine's WebAssembly module, as `@inkoxide/wasm`'s `init` takes it. Default: next to that package. */
  wasm?: WasmSource;
  /**
   * The folder of the bundled fonts and hyphenation patterns (`fonts/`, `hyphenation/`), to serve
   * them from elsewhere. Default: the files of this package, which bundlers copy.
   */
  assets?: string | URL;
  /** Render in a Web Worker. Default `true` where there is `Worker`. */
  worker?: boolean;
}

/**
 * The bundled assets, each by a URL a bundler can see (and copy into the build): keep in step
 * with fonts.ts and hyphenation.ts (test/browser.test.tsx checks).
 */
const BUNDLED: Readonly<Record<string, () => URL>> = {
  'fonts/Inter-Regular.ttf': () => new URL('../assets/fonts/Inter-Regular.ttf', import.meta.url),
  'fonts/Inter-Italic.ttf': () => new URL('../assets/fonts/Inter-Italic.ttf', import.meta.url),
  'fonts/Inter-Bold.ttf': () => new URL('../assets/fonts/Inter-Bold.ttf', import.meta.url),
  'fonts/InterDisplay-Bold.otf': () => new URL('../assets/fonts/InterDisplay-Bold.otf', import.meta.url),
  'fonts/NotoSans-Regular.ttf': () => new URL('../assets/fonts/NotoSans-Regular.ttf', import.meta.url),
  'fonts/RobotoMono-Regular.ttf': () => new URL('../assets/fonts/RobotoMono-Regular.ttf', import.meta.url),
  'hyphenation/hyph-en-us.tex': () => new URL('../assets/hyphenation/hyph-en-us.tex', import.meta.url),
};

/** The asset paths the host knows, for the test that keeps BUNDLED in step. */
export const BUNDLED_ASSETS: readonly string[] = Object.keys(BUNDLED);

let options: BrowserOptions = {};
let started = false;

/** Sets the options of the host; they are read when it starts, so only before the first render counts. */
export function configureBrowser(opts: BrowserOptions = {}): void {
  if (!started) options = opts;
}

/** An error of the engine, as the worker sends it back. */
class EngineError extends Error {
  constructor(
    readonly code: string,
    readonly path: string,
    message: string,
  ) {
    super(message);
    this.name = 'InkoxideError';
  }
}

function errorOf(e: { name: string; code?: string; path?: string; message: string }): Error {
  if (e.name === 'InkoxideError' && e.code !== undefined)
    return new EngineError(e.code, e.path ?? '', e.message);
  const err = new Error(e.message);
  err.name = e.name;
  return err;
}

/** Calls into the engine, wherever it runs. */
interface Engine {
  call(op: Op, args: unknown[], signal?: AbortSignal): Promise<unknown>;
}

/** The engine on the calling thread. */
const direct: Engine = {
  async call(op, args) {
    const f = (wasm as unknown as Record<Op, (...a: unknown[]) => unknown>)[op];
    return f(...args);
  },
};

/**
 * `source` in a form a worker can be sent: a URL as text, resolved against the page as the
 * caller meant it, and a response compiled here. Bytes and a compiled module go as they are.
 */
async function forWorker(source: WasmSource): Promise<unknown> {
  if (source instanceof URL) return source.href;
  if (typeof source === 'string') return new URL(source, globalThis.location?.href).href;
  if (source instanceof WebAssembly.Module || ArrayBuffer.isView(source) || source instanceof ArrayBuffer) {
    return source;
  }
  const res = await source;
  return WebAssembly.compile(await res.arrayBuffer());
}

interface Pending {
  message: Request;
  resolve(value: unknown): void;
  reject(e: unknown): void;
}

/**
 * The engine in a worker. What the worker was set up with (the module, fonts and patterns) is
 * kept, to set up the next one: a render that is stopped ends its worker, and the calls waiting
 * for it go to a new one.
 */
class WorkerEngine implements Engine {
  private worker: Worker | undefined;
  private next = 0;
  private readonly pending = new Map<number, Pending>();
  private readonly setup: Array<{ op: Op; args: unknown[] }> = [];

  call(op: Op, args: unknown[], signal?: AbortSignal): Promise<unknown> {
    signal?.throwIfAborted();
    const id = this.next++;
    const message: Request = { id, op, args };
    return new Promise((resolve, reject) => {
      const done = () => signal?.removeEventListener('abort', abort);
      const abort = () => {
        this.pending.delete(id);
        reject(signal?.reason);
        this.restart();
      };
      this.pending.set(id, {
        message,
        resolve: (v) => {
          done();
          // The calls a new worker replays.
          if (op === 'init' || op === 'registerFont' || op === 'registerHyphenationPatterns') {
            this.setup.push({ op, args });
          }
          resolve(v);
        },
        reject: (e) => {
          done();
          reject(e);
        },
      });
      signal?.addEventListener('abort', abort, { once: true });
      this.post(message);
    });
  }

  private post(message: Request): void {
    this.worker ??= this.spawn();
    this.worker.postMessage(message);
  }

  private spawn(): Worker {
    const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = ({ data }: MessageEvent<Response>) => {
      const p = this.pending.get(data.id);
      if (!p) return;
      this.pending.delete(data.id);
      if ('error' in data) p.reject(errorOf(data.error));
      else p.resolve(data.value);
    };
    worker.onerror = (e) => {
      e.preventDefault();
      const err = new Error(`The engine's worker failed: ${e.message}`);
      for (const p of this.pending.values()) p.reject(err);
      this.pending.clear();
      worker.terminate();
      if (this.worker === worker) this.worker = undefined;
    };
    // A new worker is set up as the last one was, before anything else.
    for (const { op, args } of this.setup) worker.postMessage({ id: -1, op, args } satisfies Request);
    return worker;
  }

  /** Ends the worker, and sends what was waiting for it to a new one. */
  private restart(): void {
    this.worker?.terminate();
    this.worker = undefined;
    for (const p of this.pending.values()) this.post(p.message);
  }
}

let engine: Engine | undefined;
let engineVersion: EngineVersion | undefined;

function current(): Engine {
  if (!engine) throw new Error('@inkoxide/render/browser: the engine has not started; call init().');
  return engine;
}

/** The URL of a bundled asset. */
function assetUrl(path: string): URL | undefined {
  if (options.assets !== undefined) return new URL(path, new URL(options.assets, globalThis.location?.href));
  return BUNDLED[path]?.();
}

/** The bytes at `url`, or undefined when there are none. A `file:` URL is read in Node (the tests). */
async function readUrl(url: URL): Promise<Uint8Array | undefined> {
  try {
    if (url.protocol === 'file:') {
      type Fs = { readFile(url: URL): Promise<Uint8Array> };
      const process = (globalThis as { process?: { getBuiltinModule?(id: string): unknown } }).process;
      const fs = process?.getBuiltinModule?.('node:fs/promises') as Fs | undefined;
      return await fs?.readFile(url);
    }
    const res = await fetch(url);
    return res.ok ? new Uint8Array(await res.arrayBuffer()) : undefined;
  } catch {
    return undefined;
  }
}

const browserHost: EngineHost = {
  async start() {
    started = true;
    const inWorker = options.worker ?? typeof Worker !== 'undefined';
    engine = inWorker ? new WorkerEngine() : direct;
    const { wasm: source } = options;
    const args = source === undefined ? [] : [inWorker ? await forWorker(source) : source];
    await engine.call('init', args);
    engineVersion = (await engine.call('version', [])) as EngineVersion;
  },
  version() {
    if (!engineVersion) throw new Error('@inkoxide/render/browser: the engine has not started; call init().');
    return engineVersion;
  },
  registerFont: async (id, bytes) => void (await current().call('registerFont', [id, bytes])),
  registerHyphenationPatterns: async (lang, bytes) =>
    void (await current().call('registerHyphenationPatterns', [lang, bytes])),
  validate: async (doc, images) => (await current().call('validate', [doc, 'json', images])) as EngineIssue[],
  render: async (doc, options, signal, lazyFonts = {}) => {
    // As URLs, which the engine fetches only if the document needs them.
    const urls = Object.fromEntries(
      Object.entries(lazyFonts).flatMap(([id, path]) => {
        const url = assetUrl(path);
        return url ? [[id, url.href]] : [];
      }),
    );
    const withLazy = Object.keys(urls).length > 0 ? { ...options, lazyFonts: urls } : options;
    return (await current().call('render', [doc, withLazy], signal)) as Uint8Array;
  },
  isEngineError: (e): e is Error & EngineIssue =>
    e instanceof Error &&
    e.name === 'InkoxideError' &&
    typeof (e as Partial<EngineIssue>).code === 'string' &&
    typeof (e as Partial<EngineIssue>).path === 'string',
  readAsset: async (path) => {
    const url = assetUrl(path);
    return url && readUrl(url);
  },
  assetLocation: (path) => assetUrl(path)?.href ?? path,
  lazyFonts: true,
};

setEngineHost(browserHost);
