#!/usr/bin/env node
// Checks that the WebAssembly build of the engine behaves as the native one:
// - every fixtures/docs document, and every fixture image alone on a page, renders to the same
//   bytes. Those listed in fixtures/wasm-parity.json may differ only in numbers, by at most 1e-6
//   relative, and in the file identifier, compared with their streams decompressed. Whether
//   they differ depends on the platform's math library: on Linux they are identical, on macOS
//   they differ in the last digits;
// - the fuzz seeds (the fixture fonts, malformed and limit fonts included, and the fixture
//   images) register and render with the same outcome;
// - documents at the engine's limits (from crates/core/tests/limits.rs) fail with the same code
//   and message, or render the same, in at most LIMIT_SECONDS.
// Needs both builds: scripts/build-node.sh and scripts/build-wasm.sh.
//   node scripts/wasm-parity.mjs
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';

import * as native from '../packages/inkoxide/index.js';
import * as wasm from '../packages/wasm/index.js';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const LIMIT_SECONDS = 20;
const CREATION_DATE = '2026-10-08T00:00:00Z';

await wasm.init();
// renderSync cannot fetch the built-in hyphenation patterns: all of them, first.
await wasm.loadHyphenation(
  readdirSync(join(ROOT, 'crates/core/hyphenation'))
    .filter((f) => f.endsWith('.bin'))
    .map((f) => f.slice(0, -'.bin'.length)),
);
const bindings = { native, wasm };
const failures = [];
const fail = (what) => failures.push(what);

const files = (dir, ext) =>
  readdirSync(join(ROOT, dir))
    .filter((f) => ext.some((e) => f.toLowerCase().endsWith(e)))
    .sort();
const read = (...p) => readFileSync(join(ROOT, ...p));
const stem = (f) => f.replace(/\.[^.]+$/, '');

/** What `f` gives with each binding: `{ ok, bytes }` or `{ code, message }`. */
function both(f) {
  return Object.fromEntries(
    Object.entries(bindings).map(([name, b]) => {
      const t = performance.now();
      try {
        const bytes = f(b);
        return [name, { ok: true, bytes, ms: performance.now() - t }];
      } catch (e) {
        if (!(e instanceof b.InkoxideError)) throw e;
        return [name, { code: e.code, path: e.path, message: e.message, ms: performance.now() - t }];
      }
    }),
  );
}

const outcome = (r) => (r.ok ? 'ok' : `${r.code} at ${r.path}: ${r.message}`);
const sameBytes = (a, b) => a.length === b.length && a.every((v, k) => v === b[k]);

/** Whether the two results agree; renders must give the same bytes. */
function agree(what, r) {
  if (outcome(r.native) !== outcome(r.wasm)) {
    fail(`${what}: native ${outcome(r.native)}; wasm ${outcome(r.wasm)}`);
    return false;
  }
  if (r.native.ok && r.native.bytes && !sameBytes(r.native.bytes, r.wasm.bytes)) {
    fail(`${what}: the bytes differ`);
    return false;
  }
  return true;
}

// PDF text with its streams decompressed, without what moves when a number's digits change: the
// file identifier, stream lengths and the cross-reference table after the last object.
function normalised(pdf) {
  const text = Buffer.from(pdf).toString('latin1');
  const inflate = (_, data) => {
    try {
      return `stream\n${inflateSync(Buffer.from(data, 'latin1')).toString('latin1')}\nendstream`;
    } catch {
      return `stream\n${data}\nendstream`;
    }
  };
  return text
    .slice(0, text.lastIndexOf('\nxref\n'))
    .replace(/stream\n([\s\S]*?)\nendstream/g, inflate)
    .replace(/\/ID \[<[0-9A-Fa-f]*> ?<[0-9A-Fa-f]*>\]/g, '/ID []')
    .replace(/\/Length \d+/g, '/Length _');
}

/** Whether two PDFs differ only in numbers, by at most 1e-6 relative, past `normalised`. */
function closeEnough(a, b) {
  const number = /[-+]?(?:\d+\.?\d*|\.\d+)/g;
  const [ta, tb] = [normalised(a), normalised(b)];
  const [sa, sb] = [ta.split(number), tb.split(number)];
  if (sa.length !== sb.length || sa.some((s, k) => s !== sb[k])) return false;
  const [na, nb] = [ta.match(number) ?? [], tb.match(number) ?? []];
  return na.every((x, k) => {
    const [p, q] = [Number(x), Number(nb[k])];
    return Math.abs(p - q) <= 1e-6 * Math.max(1, Math.abs(p), Math.abs(q));
  });
}

// Fonts: every fixture font, as the fixture tests register them, and the fuzz seeds.
for (const f of files('fixtures/fonts', ['.ttf', '.otf'])) {
  for (const b of Object.values(bindings)) b.registerFont(stem(f), read('fixtures/fonts', f));
}
for (const dir of ['fixtures/fonts/woff', 'fixtures/fonts/malformed', 'fixtures/fonts/limits']) {
  for (const f of files(dir, ['.ttf', '.otf', '.woff', '.woff2'])) {
    const bytes = read(dir, f);
    agree(
      `register ${dir}/${f}`,
      both((b) => b.registerFont(`seed-${f}`, bytes)),
    );
  }
}

const allowed = JSON.parse(read('fixtures/wasm-parity.json'));
let identical = 0;

/** Whether renders `r` agree: the same bytes, or for an `allowed` name the same but numbers. */
function sameRender(what, name, allow, r) {
  if (outcome(r.native) !== outcome(r.wasm)) {
    fail(`${what}: native ${outcome(r.native)}; wasm ${outcome(r.wasm)}`);
  } else if (!r.native.ok || sameBytes(r.native.bytes, r.wasm.bytes)) {
    identical++;
  } else if (!allow.includes(name)) {
    fail(`${what}: the bytes differ`);
  } else if (!closeEnough(r.native.bytes, r.wasm.bytes)) {
    fail(`${what}: differs beyond numbers`);
  }
}

// Images: each fixture image alone in a document.
const images = Object.fromEntries(
  files('fixtures/images', ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg']).map((f) => [
    stem(f),
    read('fixtures/images', f),
  ]),
);
for (const [id, bytes] of Object.entries(images)) {
  const doc = JSON.stringify({
    version: 1,
    fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
    pages: [
      { size: { width: 200, height: 200 }, children: [{ type: 'image', src: id, style: { width: 100 } }] },
    ],
  });
  const opts = { creationDate: CREATION_DATE, images: { [id]: bytes } };
  sameRender(
    `image ${id}`,
    id,
    allowed.images,
    both((b) => b.renderSync(doc, opts)),
  );
}

// Fixture documents, with their own hyphenation patterns.
const hyphenation = files('fixtures/hyphenation', ['.tex']);
for (const f of files('fixtures/docs', ['.json'])) {
  const name = stem(f);
  const doc = read('fixtures/docs', f);
  const patterns = hyphenation
    .filter((h) => h.startsWith(`${name}.`))
    .map((h) => [h.slice(name.length + 1, -'.tex'.length), read('fixtures/hyphenation', h)]);
  for (const b of Object.values(bindings)) {
    for (const [lang, data] of patterns) b.registerHyphenationPatterns(lang, data);
  }
  try {
    const opts = { creationDate: CREATION_DATE, images };
    sameRender(
      name,
      name,
      allowed.docs,
      both((b) => b.renderSync(doc, opts)),
    );
  } finally {
    for (const b of Object.values(bindings)) {
      for (const [lang] of patterns) b.unregisterHyphenationPatterns(lang);
    }
  }
}

// Documents at the engine's limits (crates/core/tests/limits.rs).
const page = (children, fonts = [{ id: 'Inter-Regular', family: 'Inter' }], extra = {}) =>
  JSON.stringify({
    version: 1,
    fonts,
    ...extra,
    pages: [{ size: { width: 595, height: 842 }, children }],
  });
const tall = [1, 2].map(() => ({ type: 'box', style: { height: 1000000 } }));
const doubling = [{ id: 'glyph-doubling', family: 'Doubling' }];
const run = (text, font = 'Inter-Regular') => ({ type: 'text', runs: [{ text, font, size: 10 }] });
let deep = run('deep');
for (let i = 0; i < 60; i++) deep = { type: 'box', style: { padding: { left: 0.1 } }, children: [deep] };
const svgImage = (svg) => ({
  doc: [{ type: 'image', src: 'svg', style: { width: 100, height: 100 } }],
  images: { svg: new TextEncoder().encode(svg) },
});
let copies = `<text id="l0">${'x'.repeat(1000)}</text>`;
for (let i = 1; i <= 10; i++) copies += `<g id="l${i}">${`<use href="#l${i - 1}"/>`.repeat(10)}</g>`;
const limits = {
  'page count': [page(Array.from({ length: 100 }, () => ({ type: 'box', style: { height: 1000000 } })))],
  'repeated content': [
    page([
      {
        type: 'text',
        break: { fixed: true },
        runs: [{ text: 'x'.repeat(1000), font: 'Inter-Regular', size: 10, link: 'https://example.com' }],
      },
      ...tall,
    ]),
  ],
  'repeated notes': [
    page([{ type: 'note', break: { fixed: true }, note: { contents: `${'x'.repeat(7999)}é` } }, ...tall]),
  ],
  'shaped glyphs': [
    page(
      Array.from({ length: 200 }, () => run('A A', 'glyph-doubling')),
      doubling,
    ),
  ],
  'shaped glyphs on every page': [
    JSON.stringify({
      version: 1,
      fonts: doubling,
      pages: [{ size: { width: 595, height: 842 }, header: run('A', 'glyph-doubling'), children: tall }],
    }),
  ],
  'SVG text glyphs': (() => {
    const { doc, images } = svgImage(
      `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text font-family="Doubling">${Array(300).fill('A').join(' ')}</text></svg>`,
    );
    return [page(doc, doubling), { images }];
  })(),
  'SVG text copies': (() => {
    const { doc, images } = svgImage(
      `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs>${copies}</defs><use href="#l10"/></svg>`,
    );
    return [page(doc), { images }];
  })(),
  'colour glyphs that fan out': [
    page(
      [run('😀 a')],
      [
        { id: 'Inter-Regular', family: 'Inter' },
        { id: 'colr-fanout', family: 'F' },
      ],
      { fallbackFonts: ['colr-fanout'] },
    ),
  ],
  'colour glyphs that fan out without painting': [
    page(
      [run('😀 a')],
      [
        { id: 'Inter-Regular', family: 'Inter' },
        { id: 'colr-fanout-empty', family: 'F' },
      ],
      { fallbackFonts: ['colr-fanout-empty'] },
    ),
  ],
  'deepest nesting': [page([deep])],
};
for (const f of ['glyph-doubling', 'colr-fanout', 'colr-fanout-empty']) {
  for (const b of Object.values(bindings)) b.registerFont(f, read('fixtures/fonts/limits', `${f}.ttf`));
}
for (const [what, [doc, opts = {}]] of Object.entries(limits)) {
  const r = both((b) => b.renderSync(doc, { creationDate: CREATION_DATE, ...opts }));
  agree(`limit: ${what}`, r);
  if (r.wasm.ms > LIMIT_SECONDS * 1000) fail(`limit: ${what}: took ${(r.wasm.ms / 1000).toFixed(1)} s`);
}

const renders = files('fixtures/docs', ['.json']).length + Object.keys(images).length;
const differ = allowed.docs.length + allowed.images.length;
console.log(`${identical} of ${renders} fixture renders identical, ${differ} allowed to differ in numbers`);
if (failures.length) {
  console.error(failures.map((f) => `wasm-parity: ${f}`).join('\n'));
  process.exit(1);
}
console.log('wasm-parity: the WebAssembly build behaves as the native one');
