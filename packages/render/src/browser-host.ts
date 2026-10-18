/**
 * The engine host in a browser: `@inkoxide/wasm`, in a Web Worker (worker.ts) so that the page
 * keeps responding while it renders, and the bundled assets fetched as documents need them.
 * Importing it installs it. Without a worker (`worker: false`, or where there is no `Worker`),
 * the engine runs on the calling thread.
 */
import * as wasm from '@inkoxide/wasm';
import type { InitOptions, WasmSource } from '@inkoxide/wasm';

import { type EngineHost, type EngineIssue, type EngineVersion, setEngineHost } from './host.ts';
import { nodeProcess, onPage } from './runtime.ts';
import type { Op, Request, Response } from './worker.ts';

export interface BrowserOptions {
  /**
   * The engine's WebAssembly module, as `@inkoxide/wasm`'s `init` takes it: the one-thread module
   * or the module with threads. Default: that package's, the module with threads for a pool.
   */
  wasm?: WasmSource;
  /**
   * The threads renders use in the worker: `'auto'`, `1`, or an integer up to 256. Above 1, the
   * engine starts that many more workers, which share its memory: that needs a page that is
   * cross-origin isolated (`Cross-Origin-Opener-Policy: same-origin` and
   * `Cross-Origin-Embedder-Policy: require-corp`), and a worker (`worker` not `false`). Where
   * either is missing, renders run on one thread. `'auto'` is the number of cores, at most 8, on
   * an isolated page, and 1 otherwise. `engineStatus().threads` tells what the engine got when it
   * started. Default `'auto'`.
   */
  threads?: 'auto' | number;
  /**
   * The folder of the bundled fonts and hyphenation patterns (`fonts/`, `hyphenation/`), to serve
   * them from elsewhere: `fonts/` and `hyphenation/` from this package's `assets/`, and the
   * engine's built-in patterns (`@inkoxide/wasm`'s `hyphenation/*.bin`) in `hyphenation/` too.
   * Default: the files of the packages, which bundlers copy.
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
  if (typeof source === 'string') return onPage(source).href;
  if (source instanceof WebAssembly.Module || ArrayBuffer.isView(source) || source instanceof ArrayBuffer) {
    return source;
  }
  const res = await source;
  return WebAssembly.compile(await res.arrayBuffer());
}

/** The threads to ask the engine for, from the `threads` option: `'auto'` as documented. */
function threadsFor(inWorker: boolean): number {
  const { threads = 'auto' } = options;
  if (!inWorker) return 1;
  if (threads !== 'auto') return threads;
  const isolated = (globalThis as { crossOriginIsolated?: boolean }).crossOriginIsolated === true;
  return isolated ? Math.min(globalThis.navigator?.hardwareConcurrency ?? 1, 8) : 1;
}

/**
 * The script of a worker that runs the script at `url`, of another origin. A classic worker (a
 * bundler's that loads chunks with `importScripts`) imports it so. A module worker imports it
 * dynamically: a static import would be fetched as the worker's own script, which WebKit refuses
 * from another origin on an isolated page. The messages that come meanwhile are kept, and sent on
 * once the module has its handler; an import that fails is an error of the worker's, which the
 * page hears of.
 */
function importing(url: string, module: boolean): string {
  if (!module) return `importScripts(${JSON.stringify(url)});\n`;
  return `const kept = [];
const keep = (e) => {
  e.stopImmediatePropagation();
  kept.push(e);
};
addEventListener('message', keep);
try {
  await import(${JSON.stringify(url)});
  removeEventListener('message', keep);
  for (const e of kept) dispatchEvent(new MessageEvent('message', { data: e.data }));
} catch (e) {
  setTimeout(() => {
    throw e;
  });
}
`;
}

/**
 * The worker `start` makes, as it makes it: `new Worker(new URL(…, import.meta.url))`, which
 * bundlers see and bundle the script of. Meanwhile `Worker` also takes a script of another origin
 * (this package served by a CDN): a page cannot start a worker from it, so the worker is one of
 * the page's origin that imports it, and that needs the script served with CORS.
 */
function startWorker(start: () => Worker): Worker {
  const Native = globalThis.Worker;
  globalThis.Worker = class extends Native {
    constructor(url: string | URL, options?: WorkerOptions) {
      const page = (globalThis as { location?: { href: string; origin: string } }).location;
      const at = new URL(url, page?.href);
      const own = page === undefined || at.origin === page.origin;
      const local = own || at.protocol === 'blob:' || at.protocol === 'data:';
      const blob = local
        ? undefined
        : URL.createObjectURL(
            new Blob([importing(at.href, options?.type === 'module')], { type: 'text/javascript' }),
          );
      super(blob ?? url, options);
      // The worker has it now.
      if (blob) URL.revokeObjectURL(blob);
    }
  };
  try {
    return start();
  } finally {
    globalThis.Worker = Native;
  }
}

interface Pending {
  message: Request;
  resolve(value: unknown): void;
  reject(e: unknown): void;
}

/**
 * The engine in a worker. What the worker was set up with (the module, fonts and patterns) is
 * kept, to set up the next one: a render that is stopped ends its worker, and the calls waiting
 * for it go to a new one. So does a bug in the engine on one of its pool's threads, which the
 * worker, waiting for that thread, cannot report: the thread posts on a channel of each worker's
 * own (`trapChannel`), and the call it was running fails with `INTERNAL`.
 */
class WorkerEngine implements Engine {
  private worker: Worker | undefined;
  private channel: BroadcastChannel | undefined;
  private channels = 0;
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
    this.worker.postMessage(this.withChannel(message));
  }

  /** `message`, with `init` given the trap channel of the current worker. */
  private withChannel(message: Request): Request {
    if (message.op !== 'init' || !this.channel) return message;
    const [source, opts] = message.args as [unknown, InitOptions | undefined];
    return { ...message, args: [source, { ...opts, trapChannel: this.channel.name }] };
  }

  private spawn(): Worker {
    const worker = startWorker(() => new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' }));
    this.channel?.close();
    this.channel = undefined;
    if (typeof BroadcastChannel !== 'undefined') {
      const channel = new BroadcastChannel(
        `inkoxide-trap-${Math.random().toString(36).slice(2)}-${this.channels++}`,
      );
      channel.onmessage = ({ data }: MessageEvent<{ message: string }>) => {
        if (this.worker !== worker) return;
        // The call the worker is running: the first of those waiting, which it answers in order.
        const first = this.pending.entries().next().value;
        if (first) {
          this.pending.delete(first[0]);
          first[1].reject(new EngineError('INTERNAL', '', `the engine stopped: ${data.message}`));
        }
        this.restart();
      };
      this.channel = channel;
    }
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
    for (const { op, args } of this.setup) worker.postMessage(this.withChannel({ id: -1, op, args }));
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
let engineThreads = 1;

function current(): Engine {
  if (!engine) throw new Error('@inkoxide/render/browser: the engine has not started; call init().');
  return engine;
}

/**
 * Empties the engine's caches (decoded images, font subsets, SVG converted with its text), when
 * it has started. Its memory stays with the engine, for the renders after.
 */
export async function clearCaches(): Promise<void> {
  await engine?.call('clearCaches', []);
}

/** The URL of a bundled asset. */
function assetUrl(path: string): URL | undefined {
  if (options.assets !== undefined) return new URL(path, onPage(options.assets));
  return BUNDLED[path]?.();
}

/** The bytes at `url`, or undefined when there are none. A `file:` URL is read in Node (the tests). */
async function readUrl(url: URL): Promise<Uint8Array | undefined> {
  try {
    if (url.protocol === 'file:') {
      type Fs = { readFile(url: URL): Promise<Uint8Array> };
      const fs = nodeProcess()?.getBuiltinModule?.('node:fs/promises') as Fs | undefined;
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
    const init: InitOptions = { threads: threadsFor(inWorker) };
    // The engine's built-in hyphenation patterns are served from `assets` too, when it is set.
    if (options.assets !== undefined) init.hyphenation = assetUrl('hyphenation/')?.href;
    const module = source === undefined ? undefined : inWorker ? await forWorker(source) : source;
    await engine.call('init', [module, init]);
    engineVersion = (await engine.call('version', [])) as EngineVersion;
    engineThreads = (await engine.call('threadCount', [])) as number;
  },
  version() {
    if (!engineVersion) throw new Error('@inkoxide/render/browser: the engine has not started; call init().');
    return engineVersion;
  },
  threads: () => engineThreads,
  registerFont: async (id, bytes) => void (await current().call('registerFont', [id, bytes])),
  registerHyphenationPatterns: async (lang, bytes) =>
    void (await current().call('registerHyphenationPatterns', [lang, bytes])),
  validate: async (doc, images) => (await current().call('validate', [doc, 'json', images])) as EngineIssue[],
  render: async (doc, options, signal, deferredFonts = {}) => {
    // As URLs, which the engine fetches only if the document needs them.
    const urls = Object.fromEntries(
      Object.entries(deferredFonts).flatMap(([id, path]) => {
        const url = assetUrl(path);
        return url ? [[id, url.href]] : [];
      }),
    );
    const withDeferred = Object.keys(urls).length > 0 ? { ...options, deferredFonts: urls } : options;
    return (await current().call('render', [doc, withDeferred], signal)) as Uint8Array;
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
  defersFonts: true,
};

setEngineHost(browserHost);
