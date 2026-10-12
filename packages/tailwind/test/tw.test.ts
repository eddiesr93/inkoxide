import { beforeAll, describe, expect, it } from 'vitest';

import {
  initTailwind,
  parseColor,
  parseObjectPosition,
  SUPPORTED,
  setTailwindStrict,
  tw,
  UNSUPPORTED,
  UnsupportedUtilityError,
} from '../src/index.ts';

beforeAll(async () => {
  await initTailwind();
  setTailwindStrict(true);
});

describe('supported utilities (every SUPPORTED example)', () => {
  for (const family of SUPPORTED) {
    describe(`${family.group}: ${family.utilities}`, () => {
      for (const [cls, expected] of Object.entries(family.examples)) {
        it(cls, () => {
          expect(tw(cls)).toEqual(expected);
        });
      }
    });
  }
});

describe('unsupported utilities throw in strict mode', () => {
  for (const { example } of UNSUPPORTED) {
    it(example, () => {
      expect(() => tw(`p-2 ${example}`)).toThrow(UnsupportedUtilityError);
    });
  }

  it('names the class and the CSS property', () => {
    try {
      tw('p-2 shadow-md');
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(UnsupportedUtilityError);
      const err = e as UnsupportedUtilityError;
      expect(err.className).toBe('shadow-md');
      expect(err.property).toBe('box-shadow');
    }
  });

  it('ignores the class and warns once when not strict', () => {
    setTailwindStrict(false);
    const warn = console.warn;
    const messages: string[] = [];
    console.warn = (m: string) => messages.push(m);
    try {
      expect(tw('p-1 shadow-lg')).toEqual({
        paddingTop: 3,
        paddingRight: 3,
        paddingBottom: 3,
        paddingLeft: 3,
      });
      tw('p-2 shadow-lg');
      expect(messages).toHaveLength(1);
      expect(messages[0]).toContain('shadow-lg');
    } finally {
      console.warn = warn;
      setTailwindStrict(true);
    }
  });
});

describe('S1 acceptance', () => {
  it("tw('p-4 bg-neutral-100 text-[11pt] font-bold rounded border')", () => {
    expect(tw('p-4 bg-neutral-100 text-[11pt] font-bold rounded border')).toEqual({
      paddingTop: 12,
      paddingRight: 12,
      paddingBottom: 12,
      paddingLeft: 12,
      backgroundColor: [0.960587, 0.960587, 0.960587, 1],
      fontSize: 11,
      fontWeight: 700,
      borderRadius: 3,
      borderStyle: 'solid',
      borderTopWidth: 0.75,
      borderRightWidth: 0.75,
      borderBottomWidth: 0.75,
      borderLeftWidth: 0.75,
    });
  });
});

describe('resolution rules', () => {
  it('lets a later Tailwind utility win regardless of class order', () => {
    expect(tw('pt-2 p-4')).toEqual(tw('p-4 pt-2'));
  });

  it('resolves --tw-* variables set by other classes on the same element', () => {
    expect(tw('border-dashed border').borderStyle).toBe('dashed');
    expect(tw('border border-dashed').borderStyle).toBe('dashed');
  });

  it('caches by normalised class string and returns the same object', () => {
    const a = tw('  p-2   m-1 ');
    expect(tw('p-2 m-1')).toBe(a);
  });

  it('returns an empty style for empty input', () => {
    expect(tw('')).toEqual({});
    expect(tw(undefined)).toEqual({});
  });

  it('converts oklch, hex, rgb and named colours to clamped sRGB floats', () => {
    expect(parseColor('#ff0000')).toEqual([1, 0, 0, 1]);
    expect(parseColor('rgb(0 0 255 / 50%)')).toEqual([0, 0, 1, 0.5]);
    expect(parseColor('white')).toEqual([1, 1, 1, 1]);
    expect((parseColor('oklch(100% 0.4 30)') as number[]).every((x) => x >= 0 && x <= 1)).toBe(true);
    expect(parseColor('currentColor')).toBe('currentColor');
  });
});

describe('parseObjectPosition', () => {
  const p = (v: string) => parseObjectPosition('objectPosition', v);

  it('reads one value as x, or as y for top and bottom; the other axis is centred', () => {
    expect(p('left')).toEqual({ x: 'left', y: 'center' });
    expect(p('right')).toEqual({ x: 'right', y: 'center' });
    expect(p('top')).toEqual({ x: 'center', y: 'top' });
    expect(p('bottom')).toEqual({ x: 'center', y: 'bottom' });
    expect(p('center')).toEqual({ x: 'center', y: 'center' });
    expect(p('30%')).toEqual({ x: '30%', y: 'center' });
    expect(p('6pt')).toEqual({ x: 6, y: 'center' });
  });

  it('takes two keywords in either order', () => {
    expect(p('left top')).toEqual({ x: 'left', y: 'top' });
    expect(p('top left')).toEqual({ x: 'left', y: 'top' });
    expect(p('bottom right')).toEqual({ x: 'right', y: 'bottom' });
    expect(p('center left')).toEqual({ x: 'left', y: 'center' });
    expect(p('top center')).toEqual({ x: 'center', y: 'top' });
    expect(p('center bottom')).toEqual({ x: 'center', y: 'bottom' });
  });

  it('reads lengths and percentages as x then y, mixed with keywords', () => {
    expect(p('25% 75%')).toEqual({ x: '25%', y: '75%' });
    expect(p('10pt 20px')).toEqual({ x: 10, y: 15 });
    expect(p('0 -2pt')).toEqual({ x: 0, y: -2 });
    expect(p('left 40%')).toEqual({ x: 'left', y: '40%' });
    expect(p('1rem bottom')).toEqual({ x: 12, y: 'bottom' });
    expect(p('-10% center')).toEqual({ x: '-10%', y: 'center' });
  });

  it('rejects swapped axes, edge offsets and non-lengths', () => {
    for (const bad of [
      'top 25%',
      '25% left',
      'left right',
      'top bottom',
      'right 10px top',
      'left top 0 0',
      '',
      'middle',
      'auto',
      '10',
      '2em',
    ]) {
      expect(() => p(bad), bad).toThrow(/objectPosition/);
    }
  });
});

describe('grid placement', () => {
  it('keeps a span after the slash', () => {
    expect(tw('col-[2/span_3]')).toEqual({ gridColumn: { start: 2, span: 3 } });
    expect(tw('col-[auto/span_2]')).toEqual({ gridColumn: { span: 2 } });
    expect(tw('row-[span_2/4]')).toEqual({ gridRow: { span: 2, end: 4 } });
  });

  it('drops a span once both lines are set, as in CSS', () => {
    expect(tw('col-span-3 col-start-1 col-end-3')).toEqual({ gridColumn: { start: 1, end: 3 } });
    expect(tw('row-span-2 row-end-4')).toEqual({ gridRow: { span: 2, end: 4 } });
  });
});
