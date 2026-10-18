// Loader for the WebAssembly build of the engine plus the JS-side error type. Hand-written and
// reviewed together with index.d.ts (the public contract, the one of the `inkoxide` package plus
// `init` and `clearCaches`) and with the module's ABI, described in crates/wasm/src/lib.rs. The
// option checks are those of packages/inkoxide/index.js; keep the two in step.

export class InkoxideError extends Error {
  constructor(code, path, message) {
    super(path ? `${code} at ${path}: ${message}` : `${code}: ${message}`);
    this.name = 'InkoxideError';
    this.code = code;
    this.path = path;
  }
}

function engineError(code, path, message, cause) {
  const err = new InkoxideError(code, path, message);
  // Keep `message` the engine's text, as documented.
  Object.defineProperty(err, 'message', { value: message, enumerable: false, writable: true });
  if (cause) err.cause = cause;
  return err;
}

const argumentError = (message) => engineError('INVALID_ARGUMENT', '', message);

// The types of the render options, checked before they are sent: messages name the option and
// the type, never the value, which may be a password. undefined leaves an option out; null is
// refused, as in the Node package.
const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v);

const OPTION_TYPES = {
  format: 'string',
  compress: 'boolean',
  compressionLevel: 'number',
  creationDate: 'string',
  threads: 'number',
  subsetFonts: 'boolean',
  tagged: 'boolean',
  flattenAlpha: 'boolean',
};

const PERMISSION_TYPES = {
  printing: 'string',
  modifying: 'boolean',
  copying: 'boolean',
  annotating: 'boolean',
  fillingForms: 'boolean',
  contentAccessibility: 'boolean',
  documentAssembly: 'boolean',
};

// A value of the wrong type at `at.key`, for each key of `types`.
function checkTypes(object, types, at) {
  for (const [key, type] of Object.entries(types)) {
    const v = object[key];
    if (v !== undefined && typeof v !== type) {
      throw engineError('INVALID_ARGUMENT', `${at}.${key}`, `must be a ${type}`);
    }
  }
}

function checkObject(v, at) {
  if (!isObject(v)) throw engineError('INVALID_ARGUMENT', at, 'must be an object');
}

function checkOptions(options) {
  if (options === undefined || options === null) return;
  checkObject(options, 'options');
  checkTypes(options, OPTION_TYPES, 'options');
  const { images, attachments, encryption, deferredFonts } = options;
  for (const key of ['pdfA', 'pdfUA']) {
    const v = options[key];
    if (v !== undefined && typeof v !== 'boolean' && typeof v !== 'string') {
      throw engineError('INVALID_ARGUMENT', `options.${key}`, 'must be a boolean or a string');
    }
  }
  if (deferredFonts !== undefined) {
    checkObject(deferredFonts, 'options.deferredFonts');
    for (const [id, url] of Object.entries(deferredFonts)) {
      if (typeof url !== 'string' && !(url instanceof URL)) {
        throw engineError('INVALID_ARGUMENT', `options.deferredFonts.${id}`, 'must be a URL');
      }
    }
  }
  for (const [name, files] of [
    ['images', images],
    ['attachments', attachments],
  ]) {
    if (files === undefined) continue;
    checkObject(files, `options.${name}`);
    for (const [id, bytes] of Object.entries(files)) {
      if (!(bytes instanceof Uint8Array)) {
        throw engineError('INVALID_ARGUMENT', `options.${name}.${id}`, 'must be a Uint8Array');
      }
    }
  }
  if (encryption === undefined) return;
  checkObject(encryption, 'options.encryption');
  checkTypes(encryption, { userPassword: 'string', ownerPassword: 'string' }, 'options.encryption');
  const { entropy } = encryption;
  if (entropy !== undefined && typeof entropy !== 'boolean' && !(entropy instanceof Uint8Array)) {
    throw engineError('INVALID_ARGUMENT', 'options.encryption.entropy', 'must be a boolean or a Uint8Array');
  }
  if (encryption.permissions === undefined) return;
  checkObject(encryption.permissions, 'options.encryption.permissions');
  checkTypes(encryption.permissions, PERMISSION_TYPES, 'options.encryption.permissions');
}

// The compiled module, the exports of its current instance, and what was registered in it, to
// register again in a new instance after a trap.
let compiled = null;
let exports = null;
let starting = null;
const fonts = new Map();
const patterns = new Map();
// The built-in hyphenation patterns loaded, by file (`de`), and where they come from.
const builtins = new Map();
let hyphenationBase = null;
// With the module with threads: the pool's size, its workers and the channel they report a trap
// on. `workers` is empty while a pool is starting, or when there is none.
const pool = { size: 1, workers: [], trapChannel: undefined };

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// The memory of the module with threads, which it imports (crates/wasm/build.rs links it so):
// 16 MiB at first, at most 2 GiB, shared with the pool's workers.
const SHARED_MEMORY = { initial: 256, maximum: 32768, shared: true };

// The most threads a render or a pool has: MAX_THREADS in crates/wasm/src/threads.rs.
const MAX_THREADS = 256;

/** A `file:` URL's bytes, in Node, where fetch does not read files; otherwise the URL. */
async function fromFile(url) {
  if (!(url instanceof URL) || url.protocol !== 'file:') return url;
  const fs = globalThis.process?.getBuiltinModule?.('node:fs/promises');
  return fs ? fs.readFile(url) : url;
}

async function compile(source) {
  if (source instanceof WebAssembly.Module) return source;
  source = await source;
  if (source === undefined) source = new URL('./inkoxide.wasm', import.meta.url);
  if (typeof source === 'string') source = new URL(source, import.meta.url);
  source = await fromFile(source);
  if (source instanceof URL) source = await fetch(source);
  if (typeof Response !== 'undefined' && source instanceof Response) {
    if (!source.ok) throw new Error(`@inkoxide/wasm: ${source.url}: HTTP ${source.status}`);
    // Streaming compilation needs the application/wasm content type.
    if (WebAssembly.compileStreaming && source.headers.get('content-type') === 'application/wasm') {
      return WebAssembly.compileStreaming(source);
    }
    source = await source.arrayBuffer();
  }
  return WebAssembly.compile(source);
}

// Whether this thread may share memory with workers and wait for them: a page that is
// cross-origin isolated, in a worker (a page's main thread cannot wait), or Node.
const canShare = () =>
  typeof SharedArrayBuffer === 'function' &&
  globalThis.crossOriginIsolated !== false &&
  !(typeof window === 'object' && typeof document === 'object');

// Shared memory for the module with threads, of at most `maximum` pages, or null where it cannot
// be had. The module takes any maximum up to its own.
function sharedMemory(maximum = SHARED_MEMORY.maximum) {
  if (typeof SharedArrayBuffer !== 'function' || globalThis.crossOriginIsolated === false) return null;
  try {
    return new WebAssembly.Memory({ ...SHARED_MEMORY, maximum });
  } catch (e) {
    // A device that cannot reserve the maximum.
    if (e instanceof RangeError) return null;
    throw e;
  }
}

const importsMemory = (module) => WebAssembly.Module.imports(module).some((i) => i.kind === 'memory');

// The memory of an instance of `module`: none for a module with its own; else `memory`, or new
// shared memory of the largest maximum the device reserves (after a trap, while the old memory is
// not freed, it may reserve less).
function memoryFor(module, memory = null) {
  if (!importsMemory(module)) return null;
  for (let max = SHARED_MEMORY.maximum; !memory && max >= SHARED_MEMORY.initial; max /= 2) {
    memory = sharedMemory(max);
  }
  if (!memory) {
    throw engineError(
      'INTERNAL',
      '',
      'no shared memory for the module with threads: it needs a cross-origin isolated page, and memory the device can reserve',
    );
  }
  return memory;
}

// The imports of an instance on `memory`, which the module with threads imports.
const importsOf = (memory) => (memory ? { env: { memory } } : {});

// The exports of an instance, with its memory, which the module with threads imports.
const exportsOf = (instance, memory) => (memory ? { ...instance.exports, memory } : instance.exports);

function poolSize(threads) {
  if (threads === undefined) return 1;
  if (!Number.isInteger(threads) || threads < 1 || threads > MAX_THREADS) {
    throw engineError('INVALID_ARGUMENT', 'options.threads', `must be an integer from 1 to ${MAX_THREADS}`);
  }
  return threads;
}

/** A worker of the pool, in a page or in Node, running thread.js. */
function spawnThread() {
  const nodeThreads = globalThis.process?.getBuiltinModule?.('node:worker_threads');
  if (nodeThreads) {
    const w = new nodeThreads.Worker(new URL('./thread.js', import.meta.url));
    return {
      send: (m) => w.postMessage(m),
      onReply: (f) => {
        w.once('message', f);
        // Also after it is ready: a thread that traps throws.
        w.on('error', (e) => f({ error: e.message }));
      },
      // Once it runs, it does not keep Node from exiting.
      detach: () => w.unref(),
      terminate: () => w.terminate(),
    };
  }
  const w = new Worker(new URL('./thread.js', import.meta.url), { type: 'module' });
  return {
    send: (m) => w.postMessage(m),
    onReply: (f) => {
      w.onmessage = ({ data }) => f(data);
      w.onerror = (e) => {
        e.preventDefault();
        f({ error: e.message });
      };
    },
    detach: () => {},
    terminate: () => w.terminate(),
  };
}

// Starts the pool of the instance `e` on `memory`: its workers, once each is ready, then the
// threads in them, unless `live()` says `e` was dropped meanwhile. A worker cannot start while
// this thread waits, so a render waits for this. A trap here is a RuntimeError, as in any call.
async function startPool(module, memory, e, live) {
  const workers = [];
  try {
    await Promise.all(
      Array.from({ length: pool.size }, () => {
        const at = e.ink_thread_alloc(e.__tls_size.value, e.__tls_align.value);
        if (at === 0) throw new Error('@inkoxide/wasm: no block for a thread');
        const w = spawnThread();
        workers.push(w);
        return new Promise((resolve, reject) => {
          w.onReply((m) =>
            m.ready ? resolve() : reject(new Error(`@inkoxide/wasm: a thread failed: ${m.error}`)),
          );
          w.send({ module, memory, at, trapChannel: pool.trapChannel });
        });
      }),
    );
    if (!live()) throw new Error('@inkoxide/wasm: the engine stopped');
    outcome(e, e.ink_threads_start(pool.size));
  } catch (err) {
    for (const w of workers) w.terminate();
    throw err;
  }
  for (const w of workers) w.detach();
  pool.workers = workers;
}

function stopPool() {
  for (const w of pool.workers) w.terminate();
  pool.workers = [];
}

/**
 * Compiles and instantiates the engine once; later calls wait for the first and ignore their
 * arguments. `options.hyphenation` is the folder of the built-in hyphenation patterns
 * (`<file>.bin`), by default the package's `hyphenation/`. `options.threads` above 1 starts a
 * pool of that many threads with the module with threads, where memory can be shared.
 */
export function init(source, options) {
  if (options?.hyphenation !== undefined) {
    const base = String(options.hyphenation);
    hyphenationBase = new URL(
      base.endsWith('/') ? base : `${base}/`,
      globalThis.location?.href ?? import.meta.url,
    );
  }
  starting ??= (async () => {
    const threads = poolSize(options?.threads);
    // The memory first: where there is none, the module with threads is not downloaded.
    const shared = threads > 1 && canShare() ? sharedMemory() : null;
    const module = await compile(
      source ?? (shared ? new URL('./inkoxide-threads.wasm', import.meta.url) : undefined),
    );
    const memory = memoryFor(module, shared);
    const e = exportsOf(await WebAssembly.instantiate(module, importsOf(memory)), memory);
    pool.trapChannel = options?.trapChannel;
    if (memory && threads > 1 && canShare()) {
      pool.size = threads;
      try {
        await startPool(module, memory, e, () => true);
      } catch (err) {
        if (err instanceof WebAssembly.RuntimeError) throw err;
        // Workers that cannot start (a policy, or a bundler that leaves out thread.js): renders
        // run on one thread.
        pool.size = 1;
        console.warn(`${err.message}; rendering on one thread`);
      }
    }
    // Calls wait until here: "call init() and wait for it first".
    compiled = module;
    exports = e;
  })().catch((e) => {
    starting = null;
    throw e;
  });
  return starting;
}

/** The threads renders use: the pool's, or 1. */
export const threadCount = () => (pool.workers.length > 0 ? pool.size : 1);

function engine() {
  if (exports) return exports;
  if (!compiled) throw new Error('@inkoxide/wasm: call init() and wait for it first');
  // After a trap: a new instance, with what the old one had registered. The module with threads
  // gets new memory, and a new pool once its workers have started; renders until then run on
  // one thread.
  const memory = memoryFor(compiled);
  const e = exportsOf(new WebAssembly.Instance(compiled, importsOf(memory)), memory);
  for (const [id, data] of fonts) callIn(e, () => registerIn(e, 'ink_register_font', id, data));
  for (const [lang, data] of patterns) {
    callIn(e, () => registerIn(e, 'ink_register_hyphenation_patterns', lang, data));
  }
  for (const [file, data] of builtins) {
    callIn(e, () => registerIn(e, 'ink_register_builtin_hyphenation', file, data));
  }
  exports = e;
  if (memory && pool.size > 1) {
    startPool(compiled, memory, e, () => exports === e).catch((err) => {
      // A trap while it started ends the instance, as in a call; other failures leave it on one
      // thread.
      if (err instanceof WebAssembly.RuntimeError && exports === e) exports = null;
    });
  }
  return e;
}

// Copies `bytes` into a buffer of the instance; the call it is passed to frees it.
function put(e, bytes) {
  const ptr = e.ink_alloc(bytes.length);
  new Uint8Array(e.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

function result(e) {
  const out = new Uint8Array(e.memory.buffer, e.ink_result_ptr(), e.ink_result_len()).slice();
  e.ink_result_free();
  return out;
}

// The result of a call that returned `status`: its value, or the error it is.
function outcome(e, status) {
  const bytes = result(e);
  if (status === 0) return bytes;
  const { code, path, message } = JSON.parse(decoder.decode(bytes));
  throw engineError(code, path, message);
}

// Runs `f` on the instance `e`. A trap (a panic in the engine, or memory running out) ends the
// instance: the next call starts a new one.
function callIn(e, f) {
  try {
    return f();
  } catch (err) {
    if (err instanceof InkoxideError) throw err;
    if (err instanceof WebAssembly.RuntimeError || err instanceof RangeError) {
      if (exports === e) {
        exports = null;
        // Its threads share memory that may be inconsistent.
        stopPool();
      }
      throw engineError('INTERNAL', '', `the engine stopped: ${err.message}`, err);
    }
    throw engineError('INTERNAL', '', String(err?.message ?? err), err);
  }
}

const call = (f) => {
  const e = engine();
  return callIn(e, () => f(e));
};

function registerIn(e, name, key, data) {
  const [k, kl] = put(e, encoder.encode(key));
  const [d, dl] = put(e, data);
  outcome(e, e[name](k, kl, d, dl));
}

const bytesOf = (v) => {
  if (typeof v === 'string') return encoder.encode(v);
  if (v instanceof Uint8Array) return v;
  throw argumentError('expected a Uint8Array or a string');
};

const stringOf = (v) => {
  if (typeof v !== 'string') throw argumentError('expected a string');
  return v;
};

// Images, or attachments, as one buffer: their count, then for each the id's length, the id, the
// bytes' length and the bytes, lengths as little-endian u32s (crates/wasm/src/images.rs).
function imagesBuffer(images) {
  const entries = Object.entries(images).map(([id, bytes]) => [encoder.encode(id), bytes]);
  const size = entries.reduce((n, [id, bytes]) => n + 8 + id.length + bytes.length, 4);
  const buf = new Uint8Array(size);
  const view = new DataView(buf.buffer);
  view.setUint32(0, entries.length, true);
  let at = 4;
  for (const [id, bytes] of entries) {
    for (const part of [id, bytes]) {
      view.setUint32(at, part.length, true);
      buf.set(part, at + 4);
      at += 4 + part.length;
    }
  }
  return buf;
}

// JSON has no NaN or infinities: they go as strings, which the binding reads back, so that the
// range checks report them (crates/wasm/src/options.rs).
const finiteOrString = (_key, v) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v);

// `encryption.entropy`: `true` is 32 random bytes, drawn once per render, `false` none.
function resolveEntropy(options) {
  const entropy = options?.encryption?.entropy;
  if (typeof entropy !== 'boolean') return options;
  const random = entropy ? crypto.getRandomValues(new Uint8Array(32)) : undefined;
  return { ...options, encryption: { ...options.encryption, entropy: random } };
}

// The options the engine reads as JSON; the entropy bytes go as an array of numbers.
function optionsJson(rest) {
  const entropy = rest.encryption?.entropy;
  if (entropy !== undefined) rest.encryption = { ...rest.encryption, entropy: Array.from(entropy) };
  return encoder.encode(JSON.stringify(rest, finiteOrString));
}

export function registerFont(id, data) {
  stringOf(id);
  if (!(data instanceof Uint8Array)) throw argumentError('expected a Uint8Array');
  call((e) => registerIn(e, 'ink_register_font', id, data));
  fonts.set(id, data.slice());
}

export function unregisterFont(id) {
  const key = encoder.encode(stringOf(id));
  fonts.delete(id);
  return call((e) => e.ink_unregister_font(...put(e, key)) === 1);
}

export function registerHyphenationPatterns(lang, data) {
  stringOf(lang);
  const bytes = bytesOf(data);
  call((e) => registerIn(e, 'ink_register_hyphenation_patterns', lang, bytes));
  patterns.set(lang, bytes.slice());
}

export function unregisterHyphenationPatterns(lang) {
  const key = encoder.encode(stringOf(lang));
  patterns.delete(lang);
  return call((e) => e.ink_unregister_hyphenation_patterns(...put(e, key)) === 1);
}

export function validate(doc, format, images, options) {
  const bytes = bytesOf(doc);
  const fmt = format === undefined || format === null ? new Uint8Array() : encoder.encode(stringOf(format));
  if (images !== undefined && images !== null) {
    if (!isObject(images)) throw argumentError('expected an object of images');
    for (const v of Object.values(images)) {
      if (!(v instanceof Uint8Array)) throw argumentError('expected Uint8Array images');
    }
  }
  checkOptions(options);
  const withOptions = options !== undefined && options !== null;
  const { images: optionImages, attachments, deferredFonts, ...rest } = resolveEntropy(options) ?? {};
  images ??= optionImages;
  const has = images !== undefined && images !== null;
  const buf = has ? imagesBuffer(images) : new Uint8Array();
  if (deferredFonts !== undefined) rest.deferredFonts = Object.keys(deferredFonts);
  const json = withOptions ? optionsJson(rest) : new Uint8Array();
  const files = attachments === undefined ? new Uint8Array() : imagesBuffer(attachments);
  return call((e) => {
    const status = e.ink_validate(
      ...put(e, bytes),
      ...put(e, fmt),
      ...put(e, buf),
      has ? 1 : 0,
      ...put(e, json),
      ...put(e, files),
      withOptions ? 1 : 0,
    );
    return JSON.parse(decoder.decode(outcome(e, status)));
  });
}

// The URL of the built-in hyphenation patterns in `file`. Written as a template next to
// import.meta.url, so that bundlers copy the folder.
function hyphenationUrl(file) {
  if (hyphenationBase) return new URL(`${file}.bin`, hyphenationBase);
  return new URL(`./hyphenation/${file}.bin`, import.meta.url);
}

// The bytes at `url`; a failure is a NOT_LOADED error saying `what` could not be fetched.
async function fetchBytes(url, what) {
  try {
    const res = await fromFile(url);
    if (!(res instanceof URL)) return new Uint8Array(res);
    const r = await fetch(res);
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return new Uint8Array(await r.arrayBuffer());
  } catch (e) {
    throw engineError('NOT_LOADED', '', `${what} could not be fetched from ${url.href}: ${e.message}`, e);
  }
}

// Fetches and registers the built-in hyphenation patterns of `files` not loaded yet.
async function loadFiles(files) {
  await Promise.all(
    files
      .filter((file) => !builtins.has(file))
      .map(async (file) => {
        const bytes = await fetchBytes(hyphenationUrl(file), `the built-in hyphenation patterns ${file}`);
        call((e) => registerIn(e, 'ink_register_builtin_hyphenation', file, bytes));
        builtins.set(file, bytes);
      }),
  );
}

// Fetches and registers the fonts of `ids` from `deferredFonts` (id → URL).
async function loadFonts(ids, deferredFonts) {
  await Promise.all(
    ids.map(async (id) => {
      const url = new URL(String(deferredFonts?.[id] ?? ''), globalThis.location?.href ?? import.meta.url);
      const bytes = await fetchBytes(url, `the font ${id}`);
      registerFont(id, bytes);
    }),
  );
}

/**
 * Fetches the built-in hyphenation patterns of `langs` (BCP-47 tags) that are not loaded, so
 * that `renderSync` can hyphenate text in them; `render` loads what a document needs itself.
 */
export async function loadHyphenation(langs) {
  if (!Array.isArray(langs) || langs.some((l) => typeof l !== 'string')) {
    throw argumentError('expected an array of language tags');
  }
  const files = langs
    .map((lang) =>
      call((e) => decoder.decode(outcome(e, e.ink_hyphenation_file(...put(e, encoder.encode(lang)))))),
    )
    .filter(Boolean);
  await loadFiles([...new Set(files)]);
}

// One render: `{ pdf }`, or `{ needs }`: `{ fonts, hyphenation }` it needs and has not been given.
function renderOnce(doc, options) {
  const bytes = bytesOf(doc);
  checkOptions(options);
  const { images, attachments, deferredFonts, ...rest } = options ?? {};
  // A pool that is starting again after a trap, or could not: one thread, the same bytes.
  if (pool.size > 1 && pool.workers.length === 0 && rest.threads > 1) rest.threads = 1;
  // The engine renders without them until a render needs them: it takes their ids.
  if (deferredFonts !== undefined) rest.deferredFonts = Object.keys(deferredFonts);
  const json = optionsJson(rest);
  const buf = images === undefined ? new Uint8Array() : imagesBuffer(images);
  const files = attachments === undefined ? new Uint8Array() : imagesBuffer(attachments);
  return call((e) => {
    const status = e.ink_render(...put(e, bytes), ...put(e, json), ...put(e, buf), ...put(e, files));
    if (status === 2) return { needs: JSON.parse(decoder.decode(result(e))) };
    return { pdf: outcome(e, status) };
  });
}

function notLoaded({ fonts, hyphenation }) {
  const parts = [];
  if (fonts.length > 0) parts.push(`the fonts ${fonts.join(', ')}`);
  if (hyphenation.length > 0) parts.push(`the built-in hyphenation patterns ${hyphenation.join(', ')}`);
  const message = `not loaded: ${parts.join(' and ')}; render with render(), which loads them`;
  return engineError('NOT_LOADED', '', message);
}

/**
 * Renders on the calling thread, with what was loaded: a document that needs one of its
 * `deferredFonts`, or built-in hyphenation patterns not loaded (`loadHyphenation`), fails with
 * `NOT_LOADED`.
 */
export function renderSync(doc, options) {
  checkOptions(options);
  const { pdf, needs } = renderOnce(doc, resolveEntropy(options));
  if (needs) throw notLoaded(needs);
  return pdf;
}

/**
 * Renders on the calling thread (in a page, call it from a worker), after fetching what the
 * document needs: those of its `deferredFonts` it draws with, and built-in hyphenation patterns.
 */
export async function render(doc, options) {
  checkOptions(options);
  // Drawn once: the render after loading what the first needed uses the same bytes.
  options = resolveEntropy(options);
  const first = renderOnce(doc, options);
  if (!first.needs) return first.pdf;
  await Promise.all([
    loadFonts(first.needs.fonts, options?.deferredFonts),
    loadFiles(first.needs.hyphenation),
  ]);
  const { pdf, needs } = renderOnce(doc, options);
  if (needs) throw notLoaded(needs);
  return pdf;
}

export const version = () => call((e) => JSON.parse(decoder.decode(outcome(e, e.ink_version()))));

/** Empties the caches renders share; the memory stays with the instance, for reuse. */
export const clearCaches = () => call((e) => e.ink_clear_caches());

// Test hook, not public API and not in index.d.ts: a panic in the engine, which traps. A
// non-enumerable property of InkoxideError, so it is not on the module namespace.
Object.defineProperty(InkoxideError, '__testPanic', { value: () => call((e) => e.ink_test_panic()) });
// The same on a thread of the pool, which ends the engine: the call never returns.
Object.defineProperty(InkoxideError, '__testPoolPanic', {
  value: () => call((e) => e.ink_test_pool_panic()),
});
