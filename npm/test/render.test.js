'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { readFileSync, readdirSync } = require('node:fs');
const { join } = require('node:path');

const ink = require('..');

const root = join(__dirname, '..', '..');
const fixture = (p) => readFileSync(join(root, 'fixtures', p));

for (const f of readdirSync(join(root, 'fixtures/fonts')).filter((f) => f.endsWith('.ttf'))) {
  ink.registerFont(f.replace(/\.ttf$/, ''), fixture(`fonts/${f}`));
}
const images = {};
for (const f of readdirSync(join(root, 'fixtures/images'))) {
  images[f.replace(/\.(png|jpe?g)$/, '')] = fixture(`images/${f}`);
}

test('version', () => {
  const v = ink.version();
  assert.strictEqual(v.schema, 1);
  assert.match(v.engine, /^\d+\.\d+\.\d+/);
});

test('render resolves to a deterministic PDF', async () => {
  const doc = fixture('docs/invoice.json');
  const opts = { creationDate: '2026-10-08T00:00:00Z' };
  const a = await ink.render(doc, opts);
  const b = ink.renderSync(doc.toString('utf8'), opts);
  assert.ok(a instanceof Uint8Array);
  assert.strictEqual(Buffer.from(a.subarray(0, 5)).toString(), '%PDF-');
  assert.deepStrictEqual(a, b);
});

test('images are passed per render', async () => {
  const pdf = await ink.render(fixture('docs/images-jpeg-png.json'), { images });
  assert.ok(pdf.length > 10000);
});

test('render does not block the event loop', async () => {
  const doc = fixture('docs/table-split-header-repeat.json');
  let ticks = 0;
  const timer = setInterval(() => ticks++, 0);
  await Promise.all([1, 2, 3, 4].map(() => ink.render(doc)));
  clearInterval(timer);
  assert.ok(ticks > 0, 'the event loop was blocked');
});

test('validation errors are structured', async () => {
  const doc = JSON.stringify({
    version: 1,
    fonts: [],
    pages: [{ size: { width: 100, height: 100 }, children: [{ type: 'text', runs: [{ text: 'x', font: 'Nope', size: 10, color: [0, 0, 0, 1] }] }] }],
  });
  const errs = ink.validate(doc);
  assert.deepStrictEqual(
    errs.map((e) => [e.code, e.path]),
    [['UNKNOWN_FONT', 'pages[0].children[0].runs[0].font']],
  );
  await assert.rejects(ink.render(doc), (e) => {
    assert.ok(e instanceof ink.InkoxideError);
    assert.strictEqual(e.code, 'UNKNOWN_FONT');
    assert.strictEqual(e.path, 'pages[0].children[0].runs[0].font');
    assert.match(e.message, /Nope/);
    return true;
  });
  assert.throws(() => ink.renderSync('{'), (e) => e.code === 'PARSE_ERROR');
  assert.throws(() => ink.renderSync('{}', { format: 'yaml' }), (e) => e.code === 'INVALID_ARGUMENT');
});

test('fonts can be unregistered', () => {
  ink.registerFont('Tmp', fixture('fonts/Inter-Regular.ttf'));
  assert.strictEqual(ink.unregisterFont('Tmp'), true);
  assert.strictEqual(ink.unregisterFont('Tmp'), false);
  assert.throws(() => ink.registerFont('Bad', new Uint8Array([1, 2, 3])), (e) => e.code === 'FONT_ERROR');
});
