import assert from 'node:assert';
import { readdirSync, readFileSync } from 'node:fs';
import { availableParallelism } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import ink, { pooled, wasm } from './binding.js';

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

test('pdfA writes PDF/A metadata and an output intent', () => {
  const render = (pdfA) =>
    Buffer.from(ink.renderSync(fixture('docs/invoice.json'), { pdfA })).toString('latin1');
  const pdf = render(true);
  assert.match(pdf, /<pdfaid:part>2<\/pdfaid:part>/);
  assert.match(pdf, /\/S \/GTS_PDFA1/);
  assert.strictEqual(render('2b'), pdf);
  assert.match(render('3b'), /<pdfaid:part>3<\/pdfaid:part>/);
  const one = render('1b');
  assert.match(one, /^%PDF-1\.4/);
  assert.match(one, /<pdfaid:part>1<\/pdfaid:part>/);
  assert.match(one, /\/CIDSet/);
  // PDF/A-1b has no transparency; validate knows the level from the render's options.
  const veiled = JSON.stringify({
    version: 1,
    pages: [{ size: { width: 10, height: 10 }, children: [{ type: 'box', style: { opacity: 0.5 } }] }],
  });
  assert.deepStrictEqual(ink.validate(veiled), []);
  assert.deepStrictEqual(
    ink.validate(veiled, undefined, undefined, { pdfA: '1b' }).map((e) => [e.code, e.path]),
    [['PDFA_CONFLICT', 'pages[0].children[0].style.opacity']],
  );
  assert.throws(
    () => ink.renderSync(veiled, { pdfA: '1b' }),
    (e) => e.code === 'PDFA_CONFLICT' && e.path === 'pages[0].children[0].style.opacity',
  );
  assert.ok(ink.renderSync(veiled, { pdfA: '2b' }).length > 0);
  assert.doesNotMatch(render(false), /GTS_PDFA1/);
  assert.match(render('2a'), /<pdfaid:conformance>A<\/pdfaid:conformance>/);
  assert.throws(
    () => render('2u'),
    (e) => e.code === 'INVALID_ARGUMENT' && e.path === 'options.pdfA' && /2u/.test(e.message),
  );
});

test('tagged writes a structure tree, and pdfUA PDF/UA-1 or the first thing it lacks', () => {
  const doc = (meta, field = { name: 'agree', kind: 'checkbox', label: 'Agree' }) =>
    JSON.stringify({
      version: 1,
      meta,
      pages: [
        {
          size: { width: 200, height: 200 },
          children: [{ type: 'field', style: { width: 12, height: 12 }, field }],
        },
      ],
    });
  const render = (input, options) => Buffer.from(ink.renderSync(input, options)).toString('latin1');
  const ok = doc({ title: 'Form', lang: 'en' });
  assert.doesNotMatch(render(ok), /StructTreeRoot/);
  assert.match(render(ok, { tagged: true }), /\/StructTreeRoot/);
  const ua = render(ok, { pdfUA: true });
  assert.match(ua, /\/StructTreeRoot/);
  assert.match(ua, /<pdfuaid:part>1<\/pdfuaid:part>/);
  // PDF/UA's needs are the render's: validate checks them with its options.
  const bare = doc({ title: 'Form', lang: 'en' }, { name: 'agree', kind: 'checkbox' });
  assert.deepStrictEqual(ink.validate(bare), []);
  assert.deepStrictEqual(
    ink.validate(bare, 'json', undefined, { pdfUA: true }).map((e) => e.code),
    ['UA_CONFLICT'],
  );
  assert.ok(ink.renderSync(bare).length > 0);
  assert.throws(
    () => ink.renderSync(bare, { pdfUA: true }),
    (e) => e.code === 'UA_CONFLICT' && e.path === 'pages[0].children[0].field.label',
  );
  assert.throws(
    () => ink.renderSync(doc({ lang: 'en' }), { pdfUA: true }),
    (e) => e.code === 'UA_CONFLICT' && e.path === 'meta.title',
  );
  assert.throws(
    () => ink.renderSync(ok, { tagged: 'yes' }),
    (e) => e.code === 'INVALID_ARGUMENT' && e.path === 'options.tagged',
  );
});

test('encryption encrypts with passwords and permissions, and never echoes them', async () => {
  const doc = fixture('docs/invoice.json');
  const secret = 's3cret-marker';
  const encryption = {
    userPassword: secret,
    ownerPassword: 'owner',
    permissions: { printing: 'highResolution', copying: true },
  };
  const pdf = Buffer.from(ink.renderSync(doc, { encryption })).toString('latin1');
  assert.match(pdf, /\/Filter \/Standard/);
  assert.match(pdf, /\/V 5/);
  assert.match(pdf, /\/R 6/);
  assert.match(pdf, /\/CFM \/AESV3/);
  assert.match(pdf, /\/P -1324\b/);
  assert.match(pdf, /\/Encrypt \d+ 0 R/);
  assert.ok(!pdf.includes(secret));
  // Deterministic.
  assert.strictEqual(Buffer.from(await ink.render(doc, { encryption })).toString('latin1'), pdf);
  const rejected = (path) => (e) => {
    assert.ok(e instanceof ink.InkoxideError, `${e}`);
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, path);
    assert.ok(!e.message.includes(secret) && !JSON.stringify(e).includes(secret), e.message);
    assert.ok(!e.message.includes('1234'), e.message);
    return true;
  };
  for (const [options, path] of [
    [{ encryption: {} }, 'options.encryption'],
    [{ encryption: { userPassword: `${secret}é` } }, 'options.encryption.userPassword'],
    [
      { encryption: { userPassword: secret, ownerPassword: 'x'.repeat(128) } },
      'options.encryption.ownerPassword',
    ],
    [{ encryption: { userPassword: secret, permissions: {} } }, 'options.encryption.permissions'],
    [
      { encryption: { userPassword: secret, ownerPassword: secret, permissions: {} } },
      'options.encryption.permissions',
    ],
    [
      { encryption: { userPassword: secret, ownerPassword: 'o', permissions: { printing: 'yes' } } },
      'options.encryption.permissions.printing',
    ],
    [{ encryption: { userPassword: secret }, pdfA: true }, 'options.encryption'],
    // Wrong types are refused before napi, whose message would show the value.
    [{ encryption: secret }, 'options.encryption'],
    [{ encryption: [secret] }, 'options.encryption'],
    [{ encryption: { userPassword: 1234, ownerPassword: secret } }, 'options.encryption.userPassword'],
    [{ encryption: { userPassword: 'u', ownerPassword: { secret } } }, 'options.encryption.ownerPassword'],
    [{ encryption: { userPassword: [secret] } }, 'options.encryption.userPassword'],
    [
      { encryption: { userPassword: 'u', ownerPassword: secret, permissions: secret } },
      'options.encryption.permissions',
    ],
  ]) {
    assert.throws(() => ink.renderSync(doc, options), rejected(path));
    await assert.rejects(ink.render(doc, options), rejected(path));
  }
});

test('images are passed per render', async () => {
  const pdf = await ink.render(fixture('docs/images-jpeg-png.json'), { images });
  assert.ok(pdf.length > 10000);
});

test('attachments are passed per render, and a missing one is named', async () => {
  const doc = JSON.stringify({
    version: 1,
    attachments: [{ name: 'data.xml', src: 'xml', mimeType: 'text/xml' }],
    pages: [{ size: { width: 100, height: 100 } }],
  });
  const bytes = new TextEncoder().encode('<Invoice/>');
  const pdf = Buffer.from(await ink.render(doc, { attachments: { xml: bytes }, compress: false }));
  assert.ok(pdf.includes('/EmbeddedFiles'));
  assert.ok(pdf.includes('<Invoice/>'));
  assert.throws(() => ink.renderSync(doc), { code: 'ATTACHMENT_NOT_REGISTERED', path: 'attachments[0].src' });
});

// In WebAssembly, render runs on the calling thread, which a page keeps free with a worker.
test('render does not block the event loop', { skip: wasm }, async () => {
  const doc = fixture('docs/table-split-header-repeat.json');
  let ticks = 0;
  const timer = setInterval(() => ticks++, 0);
  await Promise.all([1, 2, 3, 4].map(() => ink.render(doc)));
  clearInterval(timer);
  assert.ok(ticks > 0, 'the event loop was blocked');
});

test('threads: 1 renders run side by side', { skip: wasm || availableParallelism() < 2 }, async () => {
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
  assert.throws(() => ink.registerFont(1, 2), isArgError);
  assert.throws(() => ink.validate({}), isArgError);
  await assert.rejects(ink.render(42), isArgError);
  await assert.rejects(ink.render('{}', { compressionLevel: 12 }), (e) => {
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, 'options.compressionLevel');
    return true;
  });
});

test('render options of the wrong type are INVALID_ARGUMENT at their path', async () => {
  const doc = fixture('docs/invoice.json');
  for (const [options, path] of [
    ['x', 'options'],
    [{ threads: '4' }, 'options.threads'],
    [{ compressionLevel: '6' }, 'options.compressionLevel'],
    [{ compress: 'no' }, 'options.compress'],
    [{ subsetFonts: 0 }, 'options.subsetFonts'],
    [{ creationDate: 20260101 }, 'options.creationDate'],
    [{ format: 1 }, 'options.format'],
    [{ pdfA: 5 }, 'options.pdfA'],
    [{ images: [] }, 'options.images'],
    [{ images: { logo: 'logo.png' } }, 'options.images.logo'],
    [
      { encryption: { userPassword: 'u', permissions: { copying: 'yes' } } },
      'options.encryption.permissions.copying',
    ],
    [
      { encryption: { userPassword: 'u', permissions: { printing: true } } },
      'options.encryption.permissions.printing',
    ],
    // napi refuses null where it takes undefined.
    [{ threads: null }, 'options.threads'],
    [{ images: null }, 'options.images'],
    [{ encryption: null }, 'options.encryption'],
    [{ encryption: { userPassword: 'u', permissions: null } }, 'options.encryption.permissions'],
  ]) {
    const rejected = (e) => {
      assert.ok(e instanceof ink.InkoxideError, `${e}`);
      assert.strictEqual(e.code, 'INVALID_ARGUMENT');
      assert.strictEqual(e.path, path);
      return true;
    };
    assert.throws(() => ink.renderSync(doc, options), rejected, JSON.stringify(options));
    await assert.rejects(ink.render(doc, options), rejected, JSON.stringify(options));
  }
});

test('out-of-range numeric options are INVALID_ARGUMENT, not converted', async () => {
  const doc = fixture('docs/invoice.json');
  const rejected = (option) => (e) => {
    assert.ok(e instanceof ink.InkoxideError, `${e}`);
    assert.strictEqual(e.code, 'INVALID_ARGUMENT');
    assert.strictEqual(e.path, `options.${option}`);
    return true;
  };
  // WebAssembly has one thread, unless it has a pool.
  const most = wasm && !pooled ? 1 : 256;
  for (const threads of [0, -1, 1.5, most + 1, 100000, NaN, Infinity, 2 ** 32 + 1]) {
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
  assert.ok(ink.renderSync(doc, { threads: most, compressionLevel: 0 }).length > 0);
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

test('PDF/A-1b masks binary alpha, and flattenAlpha draws partial alpha on white', () => {
  const doc = (src) =>
    JSON.stringify({
      version: 1,
      pages: [
        { size: { width: 100, height: 100 }, children: [{ type: 'image', src, style: { width: 50 } }] },
      ],
    });
  const options = { pdfA: '1b', images: { gif: images['anim.gif'], logo: images.logo } };
  const render = (src, extra) =>
    Buffer.from(ink.renderSync(doc(src), { ...options, ...extra })).toString('latin1');
  assert.deepStrictEqual(ink.validate(doc('gif'), undefined, undefined, options), []);
  assert.match(render('gif'), /\/ImageMask true/);
  assert.doesNotMatch(render('gif'), /\/SMask/);
  const partial = ink.validate(doc('logo'), undefined, undefined, options);
  assert.deepStrictEqual(
    partial.map((e) => [e.code, e.path]),
    [['PDFA_CONFLICT', 'pages[0].children[0].src']],
  );
  assert.match(partial[0].message, /flattenAlpha/);
  assert.throws(
    () => render('logo'),
    (e) => e.code === 'PDFA_CONFLICT',
  );
  assert.deepStrictEqual(
    ink.validate(doc('logo'), undefined, undefined, { ...options, flattenAlpha: true }),
    [],
  );
  assert.doesNotMatch(render('logo', { flattenAlpha: true }), /\/SMask|\/ImageMask/);
  // The options' own problems: invalid values are reported, wrong types thrown.
  assert.deepStrictEqual(
    ink.validate(doc('gif'), undefined, undefined, { ...options, creationDate: 'soon' }).map((e) => e.path),
    ['options.creationDate'],
  );
  assert.throws(
    () => ink.validate(doc('gif'), undefined, undefined, { flattenAlpha: 'yes' }),
    (e) => e.code === 'INVALID_ARGUMENT' && e.path === 'options.flattenAlpha',
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
  const extra = wasm ? ['clearCaches', 'init', 'loadHyphenation', 'threadCount'] : [];
  assert.deepStrictEqual(
    Object.keys(ink).sort(),
    [
      'InkoxideError',
      ...extra,
      'registerFont',
      'registerHyphenationPatterns',
      'render',
      'renderSync',
      'unregisterFont',
      'unregisterHyphenationPatterns',
      'validate',
      'version',
    ].sort(),
  );
  assert.ok(!Object.keys(ink.InkoxideError).includes('__testPanic'));
});

test('engine failures are coded errors, never a crash', async () => {
  // A panic behind the boundary becomes INTERNAL.
  assert.throws(
    () => ink.InkoxideError.__testPanic(),
    (e) => e instanceof ink.InkoxideError && e.code === 'INTERNAL',
  );
  // In WebAssembly it ends the instance; the next call has a new one, with the same fonts.
  const invoice = fixture('docs/invoice.json');
  assert.ok(ink.renderSync(invoice).length > 0);
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

test(
  'the WebAssembly engine fetches the hyphenation patterns a document needs',
  { skip: !wasm },
  async () => {
    const doc = JSON.stringify({
      version: 1,
      meta: { lang: 'de' },
      fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
      pages: [
        {
          size: { width: 200, height: 400 },
          children: [
            {
              type: 'text',
              style: { width: 80, hyphenate: true },
              runs: [{ text: 'Donaudampfschifffahrtsgesellschaft', font: 'Inter-Regular', size: 10 }],
            },
          ],
        },
      ],
    });
    assert.throws(
      () => ink.renderSync(doc),
      (e) => e instanceof ink.InkoxideError && e.code === 'NOT_LOADED' && /\bde\b/.test(e.message),
    );
    const pdf = await ink.render(doc);
    assert.deepStrictEqual(ink.renderSync(doc), pdf);
    // After a trap, a new instance has them too.
    assert.throws(() => ink.InkoxideError.__testPanic());
    assert.deepStrictEqual(ink.renderSync(doc), pdf);
  },
);

test('the WebAssembly engine fetches the deferred fonts a document needs', { skip: !wasm }, async () => {
  const noto = new URL('../../../fixtures/fonts/NotoSans-Regular.ttf', import.meta.url);
  const doc = (text) =>
    JSON.stringify({
      version: 1,
      fonts: [
        { id: 'Inter-Regular', family: 'Inter' },
        { id: 'Deferred-Noto', family: 'Noto Sans' },
      ],
      fallbackFonts: ['Deferred-Noto'],
      pages: [
        {
          size: { width: 200, height: 200 },
          children: [{ type: 'text', runs: [{ text, font: 'Inter-Regular', size: 10 }] }],
        },
      ],
    });
  const deferredFonts = { 'Deferred-Noto': noto };
  // Not needed: renders without it.
  assert.ok(ink.renderSync(doc('Hello'), { deferredFonts }).length > 0);
  // Inter has no U+01C4: the fallback is needed.
  assert.throws(
    () => ink.renderSync(doc('\u01C4'), { deferredFonts }),
    (e) => e.code === 'NOT_LOADED' && /Deferred-Noto/.test(e.message),
  );
  const pdf = await ink.render(doc('\u01C4'), { deferredFonts });
  assert.deepStrictEqual(ink.renderSync(doc('\u01C4'), { deferredFonts }), pdf);
  assert.strictEqual(ink.unregisterFont('Deferred-Noto'), true);
});
