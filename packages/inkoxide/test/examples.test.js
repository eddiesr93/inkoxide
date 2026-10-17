// Every JSON document in packages/schema/EXAMPLES.md validates and renders, with the fonts of
// fixtures/fonts and the images of fixtures/images under their file stems, as
// `inkoxide render --fonts fixtures/fonts --images fixtures/images` gives them.
import assert from 'node:assert';
import { readdirSync, readFileSync } from 'node:fs';
import { extname, join, parse } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import ink from './binding.js';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const files = (dir) =>
  readdirSync(join(root, 'fixtures', dir), { withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => [parse(e.name).name, join(root, 'fixtures', dir, e.name)]);

for (const [id, path] of files('fonts')) {
  if (['.ttf', '.otf'].includes(extname(path))) ink.registerFont(id, readFileSync(path));
}
const images = Object.fromEntries(files('images').map(([id, path]) => [id, readFileSync(path)]));

const examples = readFileSync(join(root, 'packages', 'schema', 'EXAMPLES.md'), 'utf8')
  .split('```json\n')
  .slice(1)
  .map((block) => block.slice(0, block.indexOf('```')));

test('EXAMPLES.md has documents', () => {
  assert.ok(examples.length >= 3);
});

examples.forEach((json, i) => {
  test(`example ${i + 1} validates and renders`, async () => {
    assert.deepStrictEqual(ink.validate(json, 'json', images), []);
    const pdf = await ink.render(json, { images });
    assert.strictEqual(Buffer.from(pdf.subarray(0, 5)).toString(), '%PDF-');
  });
});
