import { describe, expect, it } from 'vitest';

import {
  parseColor,
  parseFlex,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  PT_PER_UNIT,
  UnsupportedValueError,
  ValueError,
} from '../src/index.ts';
import { evalDim, parseFontFamily, Reader, substituteVars } from '../src/internal.ts';

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

  it('take units in any case, exponents, a space before the unit and calc()', () => {
    const r = new Reader('width');
    expect(['5PX', '1e1pt', '5 pt', 'calc(10pt + 2pt)', '50%', '0', 'auto'].map((v) => r.len(v))).toEqual([
      3.75,
      10,
      5,
      12,
      '50%',
      0,
      'auto',
    ]);
    expect(() => r.len('3ex')).toThrow('width: 3ex (unsupported unit "ex")');
    expect(() => r.len('1e999pt')).toThrow('width: 1e999pt (too large a number)');
  });

  it('take em, vw and vh in the units given, and fail without them', () => {
    const units = { em: 10, page: { width: 600, height: 800 } };
    const r = new Reader('width', units);
    expect(['2em', '10vw', '10vh', 'calc(1em + 2pt)'].map((v) => r.len(v))).toEqual([20, 60, 80, 12]);
    expect(parseTransform('transform', 'translate(1em, 10vw)', units)).toEqual([{ translate: [10, 60] }]);
    expect(parseOrigin('transform-origin', '1em 10vh', units)).toEqual([10, 80]);
    for (const v of ['2em', '10vw']) {
      expect(() => new Reader('width').len(v)).toThrow(
        `width: ${v} (depends on the font or page size, which is not known here)`,
      );
    }
    // A line height or letter spacing in `em` stays relative to its text.
    expect(r.dim('1.5em')).toEqual({ v: 1.5, u: 'em' });
  });

  it('are absolute where the property takes no percentage or auto', () => {
    const r = new Reader('fontSize');
    expect(() => r.points('50%')).toThrow('fontSize: 50% (a percentage is not allowed here)');
    expect(() => r.points('auto')).toThrow('fontSize: auto is not allowed here');
  });
});

describe('Reader.decorationLine', () => {
  it('reads one line or both, which draw the line-through', () => {
    const r = new Reader('text-decoration-line');
    expect(
      ['none', 'underline', 'line-through', 'underline line-through'].map((v) => r.decorationLine(v)),
    ).toEqual(['none', 'underline', 'lineThrough', 'lineThrough']);
    for (const bad of ['overline', 'none underline', 'lineThrough']) {
      expect(() => r.decorationLine(bad), bad).toThrow(/^text-decoration-line: /);
    }
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

describe('parseObjectPosition', () => {
  const position = (v: string) => parseObjectPosition('object-position', v);

  it('takes two keywords in either order, and otherwise x first', () => {
    expect(position('top left')).toEqual({ x: 'left', y: 'top' });
    expect(position('center right')).toEqual({ x: 'right', y: 'center' });
    expect(position('top')).toEqual({ x: 'center', y: 'top' });
    expect(position('25% 75%')).toEqual({ x: '25%', y: '75%' });
    expect(position('left 10pt')).toEqual({ x: 'left', y: 10 });
  });

  it('rejects y before x when a value is not a keyword, unlike transform-origin', () => {
    expect(parseOrigin('transform-origin', 'bottom 10%')).toEqual(['10%', '100%']);
    for (const bad of ['bottom 10%', '10% left', 'left right', 'top bottom', 'right 10px bottom 5px']) {
      expect(() => position(bad), bad).toThrow(UnsupportedValueError);
    }
  });
});

describe('evalDim', () => {
  it('converts absolute units to points and evaluates calc()', () => {
    expect(evalDim('1in')).toEqual({ v: 72, u: 'pt' });
    expect(evalDim('4px')).toEqual({ v: 3, u: 'pt' });
    expect(evalDim('calc(100% - 10%)')).toEqual({ v: 90, u: '%' });
    expect(evalDim('calc(2 * (1rem + 1pt))')).toEqual({ v: 26, u: 'pt' });
    expect(evalDim('0.5turn')).toEqual({ v: 180, u: 'deg' });
    expect(evalDim('1.5em')).toEqual({ v: 1.5, u: 'em' });
    expect(evalDim('1.5em', { em: 10, page: { width: 0, height: 0 } })).toEqual({ v: 15, u: 'pt' });
  });

  it('throws ValueError for what it cannot evaluate', () => {
    for (const bad of ['1pt + 1%', '2pt * 3pt', '1vw', 'calc(1pt', 'abc']) {
      expect(() => evalDim(bad), bad).toThrow(ValueError);
    }
  });
});

describe('substituteVars', () => {
  const vars: Record<string, string> = { '--a': '1pt', '--b': 'var(--a) solid', '--loop': 'var(--loop)' };
  const lookup = (name: string) => vars[name];

  it('resolves var() recursively, with fallbacks', () => {
    expect(substituteVars('var(--b)', lookup)).toBe('1pt solid');
    expect(substituteVars('calc(var(--a) * 2)', lookup)).toBe('calc(1pt * 2)');
    expect(substituteVars('var(--none, var(--a))', lookup)).toBe('1pt');
    expect(substituteVars('no vars', lookup)).toBe('no vars');
  });

  it('throws ValueError for a var() it cannot resolve, or one that refers to itself', () => {
    expect(() => substituteVars('var(--none)', lookup)).toThrow(ValueError);
    expect(() => substituteVars('var(--loop)', lookup)).toThrow(/too deep/);
  });
});

describe('errors', () => {
  it('are ValueError from the value readers, and UnsupportedValueError from the property readers', () => {
    expect(() => parseColor('nonsense')).toThrow(ValueError);
    expect(() => parseFlex('1 2 3 4', Number, String)).toThrow(ValueError);
    const r = new Reader('color');
    expect(() => r.color('nonsense')).toThrow(UnsupportedValueError);
    expect(() => r.color('nonsense')).toThrow(/^color: nonsense/);
    expect(() => new Reader('display').oneOf('display', 'block')).toThrow(
      'display: block (supported: flex, grid, none)',
    );
  });
});

describe('units and fonts', () => {
  it('converts units to points', () => {
    expect(PT_PER_UNIT.mm).toBeCloseTo(2.834646, 6);
    expect(PT_PER_UNIT.px).toBe(0.75);
    expect(PT_PER_UNIT.rem).toBe(12);
  });

  it('unquotes a font-family stack', () => {
    expect(parseFontFamily('Inter, "Noto Sans", \'Roboto Mono\'')).toEqual([
      'Inter',
      'Noto Sans',
      'Roboto Mono',
    ]);
  });
});
