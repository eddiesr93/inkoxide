// `init` and the calls before it, on the one-thread module.
import assert from 'node:assert/strict';
import test from 'node:test';

// An instance of the package of its own: `init` takes effect once per instance.
let instances = 0;
const engine = () => import(`../index.js?init-${instances++}`);

test('before init, calls fail with NOT_LOADED', async () => {
  const ink = await engine();
  assert.throws(
    () => ink.renderSync('{}'),
    (e) => e instanceof ink.InkoxideError && e.code === 'NOT_LOADED',
  );
  await assert.rejects(
    ink.loadHyphenation(['de']),
    (e) => e instanceof ink.InkoxideError && e.code === 'NOT_LOADED',
  );
});

test('a module that cannot be loaded rejects init with NOT_LOADED, and init can be called again', async () => {
  const ink = await engine();
  await assert.rejects(ink.init(new URL('file:///nonexistent/inkoxide.wasm')), (e) => {
    assert.ok(e instanceof ink.InkoxideError, `${e}`);
    assert.equal(e.code, 'NOT_LOADED');
    assert.match(e.message, /nonexistent\/inkoxide\.wasm/);
    return true;
  });
  await assert.rejects(ink.init(new Uint8Array([0, 1, 2, 3])), { name: 'InkoxideError', code: 'NOT_LOADED' });
  await ink.init();
  assert.equal(ink.version().schema, 1);
});

test('later init calls ignore their options, hyphenation too', async () => {
  const ink = await engine();
  await ink.init();
  await ink.init(undefined, { hyphenation: 'file:///nonexistent/hyphenation/' });
  await ink.loadHyphenation(['de']);
  // The first call's folder is the one used.
  const other = await engine();
  await other.init(undefined, { hyphenation: 'file:///nonexistent/hyphenation/' });
  await assert.rejects(other.loadHyphenation(['de']), (e) => {
    assert.equal(e.code, 'NOT_LOADED');
    assert.match(e.message, /nonexistent\/hyphenation\/de\.bin/);
    return true;
  });
});
