import test from 'node:test';
import assert from 'node:assert';
import { render, version, InkoxideError } from '../index.js';

test('named ESM imports', async () => {
  assert.strictEqual(typeof render, 'function');
  assert.strictEqual(version().schema, 1);
  await assert.rejects(render('{"version":2,"fonts":[],"pages":[]}'), InkoxideError);
});
