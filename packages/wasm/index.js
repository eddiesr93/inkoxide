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
  const { pdfA, images, encryption } = options;
  if (pdfA !== undefined && typeof pdfA !== 'boolean' && typeof pdfA !== 'string') {
    throw engineError('INVALID_ARGUMENT', 'options.pdfA', 'must be a boolean or a string');
  }
  if (images !== undefined) {
    checkObject(images, 'options.images');
    for (const [id, bytes] of Object.entries(images)) {
      if (!(bytes instanceof Uint8Array)) {
        throw engineError('INVALID_ARGUMENT', `options.images.${id}`, 'must be a Uint8Array');
      }
    }
  }
  if (encryption === undefined) return;
  checkObject(encryption, 'options.encryption');
  checkTypes(encryption, { userPassword: 'string', ownerPassword: 'string' }, 'options.encryption');
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

const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function compile(source) {
  if (source instanceof WebAssembly.Module) return source;
  source = await source;
  if (source === undefined) source = new URL('./inkoxide.wasm', import.meta.url);
  if (typeof source === 'string') source = new URL(source, import.meta.url);
  if (source instanceof URL && source.protocol === 'file:') {
    // Node and Deno: fetch does not read files.
    const fs = globalThis.process?.getBuiltinModule?.('node:fs/promises');
    if (fs) source = await fs.readFile(source);
    else if (globalThis.Deno) source = await globalThis.Deno.readFile(source);
  }
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

/**
 * Compiles and instantiates the engine once; later calls wait for the first and ignore their
 * `source`.
 */
export function init(source) {
  starting ??= (async () => {
    const module = await compile(source);
    const instance = await WebAssembly.instantiate(module, {});
    compiled = module;
    exports = instance.exports;
  })().catch((e) => {
    starting = null;
    throw e;
  });
  return starting;
}

function engine() {
  if (exports) return exports;
  if (!compiled) throw new Error('@inkoxide/wasm: call init() and wait for it first');
  // After a trap: a new instance, with what the old one had registered.
  const e = new WebAssembly.Instance(compiled, {}).exports;
  for (const [id, data] of fonts) callIn(e, () => registerIn(e, 'ink_register_font', id, data));
  for (const [lang, data] of patterns) {
    callIn(e, () => registerIn(e, 'ink_register_hyphenation_patterns', lang, data));
  }
  exports = e;
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
      if (exports === e) exports = null;
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

// Images as one buffer: their count, then for each the id's length, the id, the bytes' length
// and the bytes, lengths as little-endian u32s (crates/wasm/src/images.rs).
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

export function validate(doc, format, images) {
  const bytes = bytesOf(doc);
  const fmt = format === undefined || format === null ? new Uint8Array() : encoder.encode(stringOf(format));
  if (images !== undefined && images !== null) {
    if (!isObject(images)) throw argumentError('expected an object of images');
    for (const v of Object.values(images)) {
      if (!(v instanceof Uint8Array)) throw argumentError('expected Uint8Array images');
    }
  }
  const has = images !== undefined && images !== null;
  const buf = has ? imagesBuffer(images) : new Uint8Array();
  return call((e) => {
    const status = e.ink_validate(...put(e, bytes), ...put(e, fmt), ...put(e, buf), has ? 1 : 0);
    return JSON.parse(decoder.decode(outcome(e, status)));
  });
}

export function renderSync(doc, options) {
  const bytes = bytesOf(doc);
  checkOptions(options);
  const { images, ...rest } = options ?? {};
  const json = encoder.encode(JSON.stringify(rest, finiteOrString));
  const buf = images === undefined ? new Uint8Array() : imagesBuffer(images);
  return call((e) => outcome(e, e.ink_render(...put(e, bytes), ...put(e, json), ...put(e, buf))));
}

/** Renders on the calling thread: in a page, call it from a worker. */
export const render = async (doc, options) => renderSync(doc, options);

export const version = () => call((e) => JSON.parse(decoder.decode(outcome(e, e.ink_version()))));

/** Empties the caches renders share; the memory stays with the instance, for reuse. */
export const clearCaches = () => call((e) => e.ink_clear_caches());

// Test hook, not public API and not in index.d.ts: a panic in the engine, which traps. A
// non-enumerable property of InkoxideError, so it is not on the module namespace.
Object.defineProperty(InkoxideError, '__testPanic', { value: () => call((e) => e.ink_test_panic()) });
