import { describe, expect, it } from 'vitest';

import { parseFlex, Reader } from '../src/index.ts';

describe('Reader.placement', () => {
  it('reads both halves of a grid placement', () => {
    const r = new Reader('grid-column');
    expect(r.placement('2 / span 3')).toEqual({ start: 2, span: 3 });
    expect(r.placement('auto / span 2')).toEqual({ span: 2 });
    expect(r.placement('span 2 / 5')).toEqual({ span: 2, end: 5 });
    expect(r.placement('span 3 / span 3')).toEqual({ span: 3 });
    expect(r.placement('1 / -1')).toEqual({ start: 1, end: -1 });
    for (const bad of ['span 0', '0', '1 / 0', '1 / 2 / 3', 'span 70000']) {
      expect(() => r.placement(bad), bad).toThrow(/^grid-column: /);
    }
  });
});

describe('parseFlex', () => {
  const flex = (v: string) =>
    parseFlex(
      v,
      (x) => Number(x),
      (x) => x,
    );

  it('reads it as CSS does', () => {
    expect(flex('2')).toEqual([2, 1, '0%']);
    expect(flex('30%')).toEqual([1, 1, '30%']);
    expect(flex('10pt')).toEqual([1, 1, '10pt']);
    expect(flex('1 auto')).toEqual([1, 1, 'auto']);
    expect(flex('2 3')).toEqual([2, 3, '0%']);
    expect(flex('2 3 0')).toEqual([2, 3, '0']);
    expect(flex('30% 2')).toEqual([2, 1, '30%']);
    expect(flex('30% 2 3')).toEqual([2, 3, '30%']);
    expect(flex('none')).toEqual([0, 0, 'auto']);
  });

  it('rejects grow and shrink that are not plain numbers', () => {
    for (const bad of ['50% 50%', '1 2% 3', 'auto auto', '1 2 3 4', '', '-1']) {
      expect(() => flex(bad), bad).toThrow(/flex/);
    }
  });
});
