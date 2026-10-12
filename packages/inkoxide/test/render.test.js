import assert from 'node:assert';
import { readdirSync, readFileSync } from 'node:fs';
import { availableParallelism } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import * as ink from '../index.js';

const root = fileURLToPath(new URL('../../../', import.meta.url));
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
  assert.match(v.gitSha, /^([0-9a-f]{12}|unknown)$/);
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

test('fonts are subset unless subsetFonts is false', () => {
  const doc = fixture('docs/invoice.json');
  const sub = ink.renderSync(doc);
  const full = ink.renderSync(doc, { subsetFonts: false });
  assert.ok(sub.length * 5 < full.length, `${sub.length} vs ${full.length}`);
  assert.match(Buffer.from(sub).toString('latin1'), /\/BaseFont \/[A-Z]{6}\+Inter-Regular/);
});

test('pdfA writes PDF/A-2b metadata and an output intent', () => {
  const pdf = Buffer.from(ink.renderSync(fixture('docs/invoice.json'), { pdfA: true })).toString('latin1');
  assert.match(pdf, /<pdfaid:part>2<\/pdfaid:part>/);
  assert.match(pdf, /\/S \/GTS_PDFA1/);
  const plain = Buffer.from(ink.renderSync(fixture('docs/invoice.json'))).toString('latin1');
  assert.doesNotMatch(plain, /GTS_PDFA1/);
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

test('threads: 1 renders run side by side', { skip: availableParallelism() < 2 }, async () => {
  const doc = fixture('docs/table-split-header-repeat.json');
  const opts = { threads: 1 };
  await ink.render(doc, opts);
  const time = async (f) => {
    const t = performance.now();
    await f();
    return performance.now() - t;
  };
  const one = await time(() => ink.render(doc, opts));
  const four = await time(() => Promise.all([1, 2, 3, 4].map(() => ink.render(doc, opts))));
  // On one shared thread, four renders would take four times as long as one.
  assert.ok(four < 3 * one, `4 concurrent renders: ${four.toFixed(0)} ms, 1: ${one.toFixed(0)} ms`);
});

test('validation errors are structured', async () => {
  const doc = JSON.stringify({
    version: 1,
    fonts: [],
    pages: [
      {
        size: { width: 100, height: 100 },
        children: [{ type: 'text', runs: [{ text: 'x', font: 'Nope', size: 10, color: [0, 0, 0, 1] }] }],
      },
    ],
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
  assert.throws(
    () => ink.renderSync('{'),
    (e) => e.code === 'PARSE_ERROR',
  );
  assert.throws(
    () => ink.renderSync('{}', { format: 'yaml' }),
    (e) => e.code === 'INVALID_ARGUMENT',
  );
});

test('fonts can be unregistered', () => {
  ink.registerFont('Tmp', fixture('fonts/Inter-Regular.ttf'));
  assert.strictEqual(ink.unregisterFont('Tmp'), true);
  assert.strictEqual(ink.unregisterFont('Tmp'), false);
  assert.throws(
    () => ink.registerFont('Bad', new Uint8Array([1, 2, 3])),
    (e) => e.code === 'FONT_ERROR',
  );
});

test('broken font files are FONT_ERROR and leave the registry usable', () => {
  for (const f of [
    'woff/RobotoMono-Regular.woff',
    'woff/RobotoMono-Regular.woff2',
    'RobotoMono-Regular.ttf',
  ]) {
    const bytes = fixture(`fonts/${f}`);
    const broken = [
      bytes.subarray(0, 64),
      bytes.subarray(0, bytes.length >> 1),
      Buffer.from(bytes).fill(0xff, 12, 400),
    ];
    for (const b of broken) {
      assert.throws(
        () => ink.registerFont('Broken', b),
        (e) => e instanceof ink.InkoxideError && e.code === 'FONT_ERROR',
        f,
      );
    }
  }
  assert.strictEqual(ink.unregisterFont('Broken'), false);
  ink.registerFont('Tmp', fixture('fonts/woff/RobotoMono-Regular.woff2'));
  assert.strictEqual(ink.unregisterFont('Tmp'), true);
});

test('wrong argument types are INVALID_ARGUMENT errors', async () => {
  const isArgError = (e) => {
    assert.ok(e instanceof ink.InkoxideError, `${e}`);
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, '');
    return true;
  };
  assert.throws(() => ink.renderSync(42), isArgError);
  assert.throws(() => ink.renderSync('{}', { threads: 'x' }), isArgError);
  assert.throws(() => ink.registerFont(1, 2), isArgError);
  assert.throws(() => ink.validate({}), isArgError);
  await assert.rejects(ink.render(42), isArgError);
  await assert.rejects(ink.render('{}', { compressionLevel: 12 }), (e) => {
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, 'options.compressionLevel');
    return true;
  });
});

test('out-of-range numeric options are INVALID_ARGUMENT, not converted', async () => {
  const doc = fixture('docs/invoice.json');
  const rejected = (option) => (e) => {
    assert.ok(e instanceof ink.InkoxideError, `${e}`);
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, `options.${option}`);
    return true;
  };
  for (const threads of [0, -1, 1.5, 257, 100000, NaN, Infinity, 2 ** 32 + 1]) {
    assert.throws(() => ink.renderSync(doc, { threads }), rejected('threads'), `threads: ${threads}`);
    await assert.rejects(ink.render(doc, { threads }), rejected('threads'), `threads: ${threads}`);
  }
  for (const compressionLevel of [-1, 10, 9.9, NaN, 2 ** 32 + 3]) {
    assert.throws(
      () => ink.renderSync(doc, { compressionLevel }),
      rejected('compressionLevel'),
      `compressionLevel: ${compressionLevel}`,
    );
  }
  assert.ok(ink.renderSync(doc, { threads: 256, compressionLevel: 0 }).length > 0);
  // A bad creation date is the caller's argument too, not a document problem.
  assert.throws(() => ink.renderSync(doc, { creationDate: 'yesterday' }), rejected('creationDate'));
});

test('validate without images still finds the problems after many image references', () => {
  const children = Array.from({ length: 100 }, () => ({ type: 'image', src: 'logo' }));
  children.push({ type: 'text', runs: [{ text: 'x', font: 'nope', size: 10 }] });
  const doc = JSON.stringify({ version: 1, pages: [{ size: { width: 100, height: 100 }, children }] });
  const font = [['UNKNOWN_FONT', 'pages[0].children[100].runs[0].font']];
  assert.deepStrictEqual(
    ink.validate(doc).map((e) => [e.code, e.path]),
    font,
  );
  assert.deepStrictEqual(
    ink.validate(doc, 'json', { logo: images.logo }).map((e) => [e.code, e.path]),
    font,
  );
});

test('validate reports images that do not decode', () => {
  const doc = JSON.stringify({
    version: 1,
    pages: [{ size: { width: 100, height: 100 }, children: [{ type: 'image', src: 'logo' }] }],
  });
  const cut = { logo: images.logo.subarray(0, 60) };
  assert.deepStrictEqual(
    ink.validate(doc, 'json', cut).map((e) => [e.code, e.path]),
    [['INVALID_IMAGE', 'pages[0].children[0].src']],
  );
  assert.throws(
    () => ink.renderSync(doc, { images: cut }),
    (e) => e.code === 'INVALID_IMAGE' && e.path === 'pages[0].children[0].src',
  );
});

test('an error has only the documented own enumerable properties', () => {
  try {
    ink.renderSync('{');
    assert.fail('expected an error');
  } catch (e) {
    assert.deepStrictEqual(Object.keys(e), ['name', 'code', 'path']);
    assert.ok(typeof e.message === 'string' && e.message.length > 0);
  }
});

test('the module exports only the public API', () => {
  assert.deepStrictEqual(Object.keys(ink).sort(), [
    'InkoxideError',
    'registerFont',
    'registerHyphenationPatterns',
    'render',
    'renderSync',
    'unregisterFont',
    'unregisterHyphenationPatterns',
    'validate',
    'version',
  ]);
  assert.ok(!Object.keys(ink.InkoxideError).includes('__testPanic'));
});

test('engine failures are coded errors, never a crash', async () => {
  // A panic behind the boundary becomes INTERNAL.
  assert.throws(
    () => ink.InkoxideError.__testPanic(),
    (e) => e instanceof ink.InkoxideError && e.code === 'INTERNAL',
  );
  // Hostile input is a structured error.
  const deep = '{"type":"box","children":['.repeat(5000) + ']}'.repeat(5000);
  await assert.rejects(
    ink.render(`{"version":1,"fonts":[],"pages":[{"size":{"width":100,"height":100},"children":[${deep}]}]}`),
    ink.InkoxideError,
  );
  await assert.rejects(ink.render(new Uint8Array([0xff, 0x00, 0x13])), (e) => e.code === 'PARSE_ERROR');
});

test('compressionLevel changes the output size', () => {
  const doc = fixture('docs/invoice.json');
  const fast = ink.renderSync(doc, { compressionLevel: 0 });
  const best = ink.renderSync(doc, { compressionLevel: 9 });
  assert.ok(best.length < fast.length, `${best.length} vs ${fast.length}`);
});

test('registered hyphenation patterns win over the built-in ones', () => {
  const doc = fixture('docs/text-hyphenate-en-patterns.json');
  const patterns = fixture('hyphenation/text-hyphenate-en-patterns.en-us.tex');
  const without = ink.renderSync(doc, { compress: false });
  ink.registerHyphenationPatterns('en-US', patterns);
  try {
    const withPatterns = ink.renderSync(doc, { compress: false });
    assert.notDeepStrictEqual(withPatterns, without);
    assert.deepStrictEqual(ink.renderSync(doc, { compress: false }), withPatterns);
  } finally {
    assert.strictEqual(ink.unregisterHyphenationPatterns('en-US'), true);
  }
  assert.deepStrictEqual(ink.renderSync(doc, { compress: false }), without);
  // The document is en-US: patterns for en-GB leave it alone.
  ink.registerHyphenationPatterns('en_gb', patterns);
  try {
    assert.deepStrictEqual(ink.renderSync(doc, { compress: false }), without);
  } finally {
    assert.strictEqual(ink.unregisterHyphenationPatterns('EN-GB'), true);
  }
  assert.throws(() => ink.registerHyphenationPatterns('en-US', '1ba 1ca'), {
    name: 'InkoxideError',
    code: 'HYPHENATION_ERROR',
  });
});
