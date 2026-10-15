// The JSON Schema is the contract: the engine accepts exactly the values
// ajv accepts, for the value forms the schema states as patterns,
// integers or signs. (Other numbers are read as 32-bit floats: one past a
// bound by less than a float's precision there is read as the bound.)
import assert from 'node:assert';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { Ajv2020 } from 'ajv/dist/2020.js';

import * as ink from '../index.js';

const schema = JSON.parse(readFileSync(new URL('../../schema/document.v1.json', import.meta.url), 'utf8'));
const ajv = new Ajv2020({ discriminator: true, strict: false, validateFormats: false }).compile(schema);

ink.registerFont(
  'Inter-Regular',
  readFileSync(new URL('../../../fixtures/fonts/Inter-Regular.ttf', import.meta.url)),
);

const doc = (child) =>
  JSON.stringify({
    version: 1,
    fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
    pages: [{ size: { width: 500, height: 500 }, children: [child] }],
  });

/** Whether ajv and the engine agree on document text `d`; returns ajv's verdict. */
function agreeText(d) {
  const schemaOk = ajv(JSON.parse(d));
  const engine = ink.validate(d);
  assert.strictEqual(engine.length === 0, schemaOk, `${d}: ${JSON.stringify(engine)}`);
  return schemaOk;
}

/** Whether ajv and the engine agree on `child`; returns ajv's verdict. */
function agree(child) {
  return agreeText(doc(child));
}

const percents = [
  '50%',
  '-12.5%',
  '0%',
  '-0%',
  '007%',
  '10000%',
  '.5%',
  '5.%',
  '+5%',
  '1e2%',
  '5 %',
  '%',
  '-%',
];
const frs = ['1fr', '0fr', '2.5fr', '1000000fr', '-0fr', '-1fr', '.5fr', '5.fr', '1e1fr', '+1fr', 'fr'];

test('percentages are read as the Percent pattern states', () => {
  const accepted = percents.filter((p) => agree({ type: 'box', style: { margin: { top: p } } }));
  assert.deepStrictEqual(accepted, ['50%', '-12.5%', '0%', '-0%', '007%', '10000%']);
});

test('flexible sizes are read as the Fr pattern states', () => {
  const tracks = frs.filter((fr) =>
    agree({ type: 'box', style: { display: 'grid', gridTemplateColumns: [fr] } }),
  );
  assert.deepStrictEqual(tracks, ['1fr', '0fr', '2.5fr', '1000000fr']);
  // A table column is positive: 0fr is out, like a width of 0 points.
  const table = (width) => ({ type: 'table', columns: [{ width }], rows: [{ cells: [{}] }] });
  const columns = frs.filter((fr) => agree(table(fr)));
  assert.deepStrictEqual(columns, ['1fr', '2.5fr', '1000000fr']);
  assert.strictEqual(agree(table('0.00fr')), false);
});

test('integers are bounded as the schema states', () => {
  const text = (style) => ({ type: 'text', style, runs: [{ text: 'x', font: 'Inter-Regular', size: 9 }] });
  const cell = (c) => ({ type: 'table', columns: [{ width: 10 }], rows: [{ cells: [c] }] });
  for (const [child, ok] of [
    [text({ maxLines: 4294967295 }), true],
    [text({ maxLines: 4294967296 }), false],
    [text({ orphans: -1 }), false],
    [{ type: 'box', style: { zIndex: 2147483648 } }, false],
    [{ type: 'box', pages: [-2147483648, 2147483647] }, true],
    [{ type: 'box', pages: [2147483648] }, false],
    [cell({ colSpan: 4294967296 }), false],
    [cell({ rowSpan: -1 }), false],
    [
      { type: 'field', field: { name: 'n', kind: 'text', font: 'Inter-Regular', maxLength: 4294967296 } },
      false,
    ],
  ]) {
    assert.strictEqual(agree(child), ok, JSON.stringify(child));
  }
});

test('lengths that cannot be negative are bounded as the schema states', () => {
  const values = [-5, -0.5, '-10%', '-0.001%', '-0%', '-0.00%', 0, '0%', 5, '10%'];
  const nonNegative = ['-0%', '-0.00%', 0, '0%', 5, '10%'];
  const accepted = (style) => values.filter((v) => agree({ type: 'box', style: style(v) }));
  for (const p of [
    'width',
    'height',
    'minWidth',
    'minHeight',
    'maxWidth',
    'maxHeight',
    'flexBasis',
    'gap',
    'rowGap',
    'columnGap',
  ]) {
    assert.deepStrictEqual(
      accepted((v) => ({ [p]: v })),
      nonNegative,
      p,
    );
  }
  assert.deepStrictEqual(
    accepted((v) => ({ padding: { left: v } })),
    nonNegative,
  );
  // Margins may be negative.
  assert.deepStrictEqual(
    accepted((v) => ({ margin: { left: v } })),
    values,
  );
  const grid = (track) => ({ type: 'box', style: { display: 'grid', gridTemplateColumns: [track] } });
  assert.deepStrictEqual(
    values.filter((v) => agree(grid(v))),
    ['-0%', '-0.00%', 0, '0%', 5, '10%'],
  );
  for (const [track, ok] of [
    [{ min: '-1%', max: '1fr' }, false],
    [{ min: 0, max: '-0.5%' }, false],
    [{ min: '0%', max: '1fr' }, true],
    [{ fitContent: -5 }, false],
    [{ fitContent: '-5%' }, false],
    [{ fitContent: 5 }, true],
    [{ fitContent: '5%' }, true],
  ]) {
    assert.strictEqual(agree(grid(track)), ok, JSON.stringify(track));
  }
});

test('integers may be written with a zero fraction', () => {
  // JSON.stringify writes 400.0 as 400: these documents are written as text.
  const page = '"size":{"width":500,"height":500}';
  const text = '{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":9}]';
  const docWith = (weight, child) =>
    `{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter","weight":${weight}}],` +
    `"pages":[{${page},"children":[${child}]}]}`;
  const one = (child) => docWith(400, child);
  const table = (rows, rest = '') =>
    `{"type":"table","columns":[{"width":10},{"width":10}],${rest}"rows":[${rows}]}`;
  const field = (props) =>
    `{"type":"field","field":{"name":"n","kind":"text","font":"Inter-Regular",${props}}}`;
  for (const [d, ok] of [
    [one(`${text}}`).replace('"version":1', '"version":1.0'), true],
    [one(`${text}}`).replace('"version":1', '"version":1.5'), false],
    [docWith('400.0', `${text}}`), true],
    [docWith('400.5', `${text}}`), false],
    [docWith('1e3', `${text}}`), true],
    [docWith('1e300', `${text}}`), false],
    [one(table('{"cells":[{"colSpan":2.0}]}')), true],
    [one(table('{"cells":[{"colSpan":1.5},{}]}')), false],
    [one(table('{"cells":[{"rowSpan":2.0},{}]},{"cells":[{}]}')), true],
    [one(table('{"cells":[{},{}]}', '"headerRows":1.0,')), true],
    [one(`${text},"style":{"maxLines":3.0,"orphans":2.0,"widows":2.0}}`), true],
    [one(`${text},"style":{"zIndex":-1.0}}`), true],
    [one('{"type":"box","pages":[1.0,-1.0]}'), true],
    [one('{"type":"box","pages":[1.5]}'), false],
    [one('{"type":"box","style":{"gridRow":{"start":1.0,"span":2.0}}}'), true],
    [one('{"type":"box","style":{"gridColumn":{"end":-1.0}}}'), true],
    [one('{"type":"box","style":{"gridColumn":{"end":-1.5}}}'), false],
    [one('{"type":"box","bookmark":{"title":"t","level":2.0}}'), true],
    [one(field('"maxLength":10.0')), true],
    [one(field('"format":{"type":"number","decimals":2.0}')), true],
    [
      one(
        '{"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":9,"fontFeatures":{"liga":0.0}}]}',
      ),
      true,
    ],
  ]) {
    assert.strictEqual(agreeText(d), ok, d);
  }
});

test('form field options are a non-empty list', () => {
  const field = (kind, options) => ({
    type: 'field',
    field: { name: 'f', kind, font: 'Inter-Regular', ...(options && { options }) },
  });
  for (const [child, ok] of [
    [field('text', []), false],
    [field('checkbox', []), false],
    [field('select', []), false],
    [field('list', []), false],
    [field('select', ['a']), true],
    [field('text'), true],
  ]) {
    assert.strictEqual(agree(child), ok, JSON.stringify(child));
  }
});

test('percentages and flexible sizes are bounded as written', () => {
  const tiny = `0.${'0'.repeat(60)}1`;
  for (const [w, ok] of [
    ['10000%', true],
    ['10000.0001%', false],
    ['10000.00000000000000000001%', false],
    ['9999.99999999999999999999%', true],
    [`${tiny}%`, true],
    [`-${tiny}%`, false],
  ]) {
    assert.strictEqual(agree({ type: 'box', style: { width: w } }), ok, w);
  }
  for (const [m, ok] of [
    ['-10000%', true],
    ['-10000.0001%', false],
    [`-${tiny}%`, true],
  ]) {
    assert.strictEqual(agree({ type: 'box', style: { margin: { top: m } } }), ok, m);
  }
  const grid = (fr) => ({ type: 'box', style: { display: 'grid', gridTemplateColumns: [fr] } });
  for (const [fr, ok] of [
    ['1000000fr', true],
    ['1000000.01fr', false],
    ['1000000.0000000000000001fr', false],
  ]) {
    assert.strictEqual(agree(grid(fr)), ok, fr);
  }
  const table = (width) => ({ type: 'table', columns: [{ width }], rows: [{ cells: [{}] }] });
  assert.strictEqual(agree(table(`${tiny}fr`)), true);
});
