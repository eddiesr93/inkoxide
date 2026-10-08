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

function wrapError(e) {
  if (e && typeof e.message === 'string' && e.message.startsWith('{')) {
    try {
      const { code, path, message } = JSON.parse(e.message);
      const err = new InkoxideError(code, path, message);
      // Keep `message` the engine's text, as documented.
      Object.defineProperty(err, 'message', { value: message, enumerable: false, writable: true });
      return err;
    } catch {
      /* not ours */
    }
  }
  return e;
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
exports.version = () => native.version();
