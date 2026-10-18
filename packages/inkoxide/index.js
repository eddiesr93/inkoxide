// Loader for the native binding plus the JS-side error type. Hand-written
// and reviewed together with index.d.ts (the public contract).

import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

// Native addons (.node files) load through require, also from an ES module.
const require = createRequire(import.meta.url);

function isMusl() {
  if (process.platform !== 'linux') return false;
  try {
    const report = process.report && process.report.getReport();
    return !(report && report.header && report.header.glibcVersionRuntime);
  } catch {
    return false;
  }
}

// The names of scripts/platforms.mjs, which the build and release scripts use; this copy stays
// here because the published loader cannot import them.
function platformTriple() {
  const { platform, arch } = process;
  if (platform === 'linux') return `linux-${arch}-${isMusl() ? 'musl' : 'gnu'}`;
  if (platform === 'win32') return `win32-${arch}-msvc`;
  return `${platform}-${arch}`;
}

function loadNative() {
  const triple = platformTriple();
  const local = fileURLToPath(new URL(`inkoxide.${triple}.node`, import.meta.url));
  if (existsSync(local)) return require(local);
  try {
    return require(`@inkoxide/${triple}`);
  } catch (e) {
    throw new Error(
      `inkoxide: no native binary for ${triple}. ` +
        `Build one with scripts/build-node.sh or install @inkoxide/${triple}. (${e.message})`,
    );
  }
}

const native = loadNative();

export class InkoxideError extends Error {
  constructor(code, path, message) {
    super(path ? `${code} at ${path}: ${message}` : `${code}: ${message}`);
    this.name = 'InkoxideError';
    this.code = code;
    this.path = path;
  }
}

// napi statuses that mean a JS value had the wrong type.
const ARGUMENT_STATUSES = new Set([
  'InvalidArg',
  'ObjectExpected',
  'StringExpected',
  'NameExpected',
  'FunctionExpected',
  'NumberExpected',
  'BooleanExpected',
  'ArrayExpected',
  'BigintExpected',
  'DateExpected',
  'ArrayBufferExpected',
]);

function engineError(code, path, message, cause) {
  const err = new InkoxideError(code, path, message);
  // Keep `message` the engine's text, as documented.
  Object.defineProperty(err, 'message', { value: message, enumerable: false, writable: true });
  if (cause) err.cause = cause;
  return err;
}

// Every failure surfaces as an InkoxideError: the engine's own codes, or
// INVALID_ARGUMENT for values napi could not convert, or INTERNAL.
function wrapError(e) {
  if (e instanceof InkoxideError) return e;
  const text = e && typeof e.message === 'string' ? e.message : String(e);
  if (text.startsWith('{')) {
    try {
      const { code, path, message } = JSON.parse(text);
      return engineError(code, path, message);
    } catch {
      /* not ours */
    }
  }
  const code = e && ARGUMENT_STATUSES.has(e.code) ? 'INVALID_ARGUMENT' : 'INTERNAL';
  return engineError(code, '', text, e);
}

// The types of the render options, checked before napi converts them: napi's conversion error has
// no path, so a caller's mistake would read as a bug of the engine, and it may show the value,
// which may be a password. Messages name the option and the type, never the value. napi takes
// undefined for an option left out, and refuses null.
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
  const { images, encryption } = options;
  for (const key of ['pdfA', 'pdfUA']) {
    const v = options[key];
    if (v !== undefined && typeof v !== 'boolean' && typeof v !== 'string') {
      throw engineError('INVALID_ARGUMENT', `options.${key}`, 'must be a boolean or a string');
    }
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
  const { entropy } = encryption;
  if (entropy !== undefined && typeof entropy !== 'boolean' && !(entropy instanceof Uint8Array)) {
    throw engineError('INVALID_ARGUMENT', 'options.encryption.entropy', 'must be a boolean or a Uint8Array');
  }
  if (encryption.permissions === undefined) return;
  checkObject(encryption.permissions, 'options.encryption.permissions');
  checkTypes(encryption.permissions, PERMISSION_TYPES, 'options.encryption.permissions');
}

// `encryption.entropy`: `true` is 32 random bytes, drawn once per render, `false` none.
function resolveEntropy(options) {
  const entropy = options?.encryption?.entropy;
  if (typeof entropy !== 'boolean') return options;
  const random = entropy ? crypto.getRandomValues(new Uint8Array(32)) : undefined;
  return { ...options, encryption: { ...options.encryption, entropy: random } };
}

function call(fn, args) {
  try {
    return fn(...args);
  } catch (e) {
    throw wrapError(e);
  }
}

function toUint8Array(b) {
  // Buffer is a Uint8Array subclass; hand back a plain view, no copy.
  return new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
}

export const registerFont = (id, data) => call(native.registerFont, [id, data]);
export const unregisterFont = (id) => call(native.unregisterFont, [id]);
export const registerHyphenationPatterns = (lang, data) =>
  call(native.registerHyphenationPatterns, [lang, data]);
export const unregisterHyphenationPatterns = (lang) => call(native.unregisterHyphenationPatterns, [lang]);
export const validate = (doc, format, images, options) => {
  checkOptions(options);
  const resolved = resolveEntropy(options) ?? undefined;
  return call(native.validate, [doc, format ?? undefined, images ?? undefined, resolved]);
};
export const render = async (doc, options) => {
  try {
    checkOptions(options);
    return toUint8Array(await native.render(doc, resolveEntropy(options) ?? undefined));
  } catch (e) {
    throw wrapError(e);
  }
};
export const renderSync = (doc, options) => {
  checkOptions(options);
  return toUint8Array(call(native.renderSync, [doc, resolveEntropy(options) ?? undefined]));
};
export const version = () => call(native.version, []);
// Test hook, not public API and not in index.d.ts: a panic behind the native boundary. A
// non-enumerable property of InkoxideError, so it is not on the module namespace.
Object.defineProperty(InkoxideError, '__testPanic', { value: () => call(native.__testPanic, []) });
