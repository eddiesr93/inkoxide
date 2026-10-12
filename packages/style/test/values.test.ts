import { describe, expect, it } from 'vitest';

import {
  parseColor,
  parseFlex,
  parseOrigin,
  parseTransform,
  Reader,
  UnsupportedValueError,
} from '../src/index.ts';

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

describe('parseTransform errors', () => {
  it('are UnsupportedValueError, naming the property, for a bad angle', () => {
    for (const bad of ['rotate(abc)', 'rotate(1grad)', 'skewX(1grad)', 'rotate(calc(1deg)']) {
      expect(() => parseTransform('transform', bad), bad).toThrow(UnsupportedValueError);
      expect(() => parseTransform('transform', bad), bad).toThrow(/^transform: /);
    }
  });
});

describe('lengths', () => {
  it('are kept above 10000pt, and only infinite ones are capped', () => {
    const r = new Reader('width');
    expect(r.len('20000pt')).toBe(20000);
    expect(r.len('-20000pt')).toBe(-20000);
    expect(r.len('calc(infinity * 1px)')).toBe(1e4);
  });

  it('are kept above 10000pt in a transform', () => {
    expect(parseTransform('transform', 'translate(20000pt, 0)')).toEqual([{ translate: [20000, 0] }]);
  });
});

describe('skew', () => {
  it('takes angles strictly between -90deg and 90deg', () => {
    expect(parseTransform('transform', 'skewX(89deg)')).toEqual([{ skew: [89, 0] }]);
    for (const bad of ['skewX(90deg)', 'skewY(-90deg)', 'skew(10deg, 135deg)']) {
      expect(() => parseTransform('transform', bad), bad).toThrow(/between -90deg and 90deg/);
    }
  });
});

describe('parseColor', () => {
  it('reads keywords in any case', () => {
    expect(parseColor('Transparent')).toEqual([0, 0, 0, 0]);
    expect(parseColor('TRANSPARENT')).toEqual([0, 0, 0, 0]);
    expect(parseColor('Red')).toEqual([1, 0, 0, 1]);
    expect(parseColor('CURRENTCOLOR')).toBe('currentColor');
  });
});

describe('parseOrigin', () => {
  const origin = (v: string) => parseOrigin('transformOrigin', v);

  it('takes keywords in either order', () => {
    expect(origin('center left')).toEqual([0, '50%']);
    expect(origin('center right')).toEqual(['100%', '50%']);
    expect(origin('left center')).toEqual([0, '50%']);
    expect(origin('top left')).toEqual([0, 0]);
    expect(origin('bottom')).toEqual(['50%', '100%']);
    expect(origin('right 10%')).toEqual(['100%', '10%']);
  });

  it('rejects names that are not keywords', () => {
    for (const bad of ['left toString', 'constructor', 'valueOf top', 'center __proto__']) {
      expect(() => origin(bad), bad).toThrow(UnsupportedValueError);
    }
  });
});
