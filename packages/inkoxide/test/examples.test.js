'use strict';
// Every JSON document in packages/schema/EXAMPLES.md validates and renders.
const test = require('node:test');
const assert = require('node:assert');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const ink = require('..');

const root = join(__dirname, '..', '..', '..');
for (const id of ['Inter-Regular', 'Inter-Bold']) {
  ink.registerFont(id, readFileSync(join(root, 'fixtures', 'fonts', `${id}.ttf`)));
}

const examples = readFileSync(join(root, 'packages', 'schema', 'EXAMPLES.md'), 'utf8')
  .split('```json\n')
  .slice(1)
  .map((block) => block.slice(0, block.indexOf('```')));

test('EXAMPLES.md has documents', () => {
  assert.ok(examples.length >= 3);
});

examples.forEach((json, i) => {
  test(`example ${i + 1} validates and renders`, async () => {
    assert.deepStrictEqual(ink.validate(json), []);
    const pdf = await ink.render(json);
    assert.strictEqual(Buffer.from(pdf.subarray(0, 5)).toString(), '%PDF-');
  });
});
