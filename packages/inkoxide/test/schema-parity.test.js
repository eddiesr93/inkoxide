// The JSON Schema is the contract: the engine accepts exactly the values
// ajv accepts, for the value forms the schema states as patterns or
// integer bounds.
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

/** Whether ajv and the engine agree on `child`; returns ajv's verdict. */
function agree(child) {
  const d = doc(child);
  const schemaOk = ajv(JSON.parse(d));
  const engine = ink.validate(d);
  assert.strictEqual(engine.length === 0, schemaOk, `${JSON.stringify(child)}: ${JSON.stringify(engine)}`);
  return schemaOk;
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
