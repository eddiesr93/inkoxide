'use strict';
// Loader for the native binding plus the JS-side error type. Hand-written
// and reviewed together with index.d.ts (the public contract).

const { existsSync } = require('node:fs');
const { join } = require('node:path');

function isMusl() {
  if (process.platform !== 'linux') return false;
  try {
    const report = process.report && process.report.getReport();
    return !(report && report.header && report.header.glibcVersionRuntime);
  } catch {
    return false;
  }
}

function platformTriple() {
  const { platform, arch } = process;
  if (platform === 'linux') return `linux-${arch}-${isMusl() ? 'musl' : 'gnu'}`;
  if (platform === 'win32') return `win32-${arch}-msvc`;
  return `${platform}-${arch}`;
}

function loadNative() {
  const triple = platformTriple();
  const local = join(__dirname, `inkoxide.${triple}.node`);
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

class InkoxideError extends Error {
  constructor(code, path, message) {
    super(path ? `${code} at ${path}: ${message}` : `${code}: ${message}`);
    this.name = 'InkoxideError';
    this.code = code;
    this.path = path;
    this.detail = message;
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

// Plain `exports.x =` assignments so ESM `import { render } from 'inkoxide'`
// sees named exports.
exports.InkoxideError = InkoxideError;
exports.registerFont = (id, data) => call(native.registerFont, [id, data]);
exports.unregisterFont = (id) => call(native.unregisterFont, [id]);
exports.registerHyphenationPatterns = (lang, data) =>
  call(native.registerHyphenationPatterns, [lang, data]);
exports.unregisterHyphenationPatterns = (lang) =>
  call(native.unregisterHyphenationPatterns, [lang]);
exports.validate = (doc, format, images) =>
  call(native.validate, [doc, format ?? undefined, images ?? undefined]);
exports.render = async (doc, options) => {
  try {
    return toUint8Array(await native.render(doc, options ?? undefined));
  } catch (e) {
    throw wrapError(e);
  }
};
exports.renderSync = (doc, options) =>
  toUint8Array(call(native.renderSync, [doc, options ?? undefined]));
exports.version = () => call(native.version, []);
// Test hook (not public API): a panic behind the native boundary.
Object.defineProperty(exports, '__testPanic', { value: () => call(native.__testPanic, []) });
