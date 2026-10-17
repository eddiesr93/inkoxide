import { beforeAll, describe, expect, it } from 'vitest';

import {
  initTailwind,
  parseColor,
  parseObjectPosition,
  setTailwindStrict,
  tw,
  UnsupportedUtilityError,
} from '../src/index.ts';
import { A4_UNITS, SUPPORTED, UNSUPPORTED } from '../src/supported.ts';

beforeAll(async () => {
  await initTailwind();
  setTailwindStrict(true);
});

describe('supported utilities (every SUPPORTED example)', () => {
  for (const family of SUPPORTED) {
    describe(`${family.group}: ${family.utilities}`, () => {
      for (const [cls, expected] of Object.entries(family.examples)) {
        it(cls, () => {
          expect(tw(cls, family.units)).toEqual(expected);
        });
      }
    });
  }
});

describe('unsupported utilities throw in strict mode', () => {
  for (const { example } of UNSUPPORTED) {
    it(example, () => {
      expect(() => tw(`p-2 ${example}`)).toThrow(UnsupportedUtilityError);
      // Not only for want of the units a render has.
      expect(() => tw(`p-2 ${example}`, A4_UNITS)).toThrow(UnsupportedUtilityError);
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

  it('ignores a span at the end when the start is a span, as in CSS', () => {
    expect(tw('col-start-[span_2] col-end-[span_3]')).toEqual({ gridColumn: { span: 2 } });
    expect(tw('row-start-[span_2] row-end-[span_4]')).toEqual({ gridRow: { span: 2 } });
    expect(tw('col-span-2 col-end-[span_3]')).toEqual({ gridColumn: { span: 2 } });
    expect(tw('col-start-[span_2] col-end-5')).toEqual({ gridColumn: { span: 2, end: 5 } });
    expect(tw('col-start-3 col-end-[span_2]')).toEqual({ gridColumn: { start: 3, span: 2 } });
    expect(tw('col-end-[span_3]')).toEqual({ gridColumn: { span: 3 } });
    expect(errorOf('col-start-[1/2]').reason).toBe('grid-column-start: 1/2');
  });
});

describe('flex-grow and flex-shrink', () => {
  it('take a number that is not negative, as in CSS', () => {
    expect(tw('grow-[2] shrink-[0.5]')).toEqual({ flexGrow: 2, flexShrink: 0.5 });
    for (const bad of ['grow-[30%]', 'shrink-[30%]']) {
      expect(errorOf(bad).reason, bad).toMatch(/^flex-(grow|shrink): 30% \(expected a number\)$/);
    }
    for (const bad of ['grow-[-1]', 'shrink-[-1]']) {
      expect(errorOf(bad).reason, bad).toMatch(/^flex-(grow|shrink): -1 \(cannot be negative\)$/);
    }
  });
});

describe('the flex shorthand', () => {
  it('reads a lone length as the basis, as CSS and the style prop do', () => {
    expect(tw('flex-[30%]')).toEqual({ flexGrow: 1, flexShrink: 1, flexBasis: '30%' });
    expect(tw('flex-[2_2_10pt]')).toEqual({ flexGrow: 2, flexShrink: 2, flexBasis: 10 });
    expect(tw('flex-[1_1_0]')).toEqual({ flexGrow: 1, flexShrink: 1, flexBasis: 0 });
  });

  it('rejects grow and shrink with units', () => {
    expect(() => tw('flex-[50%_50%]')).toThrow(/grow and shrink are numbers/);
  });
});

/** The error `tw(cls)` throws. */
function errorOf(cls: string): UnsupportedUtilityError {
  try {
    tw(cls);
  } catch (e) {
    expect(e).toBeInstanceOf(UnsupportedUtilityError);
    return e as UnsupportedUtilityError;
  }
  return expect.unreachable(`${cls} did not throw`);
}

describe('errors name the CSS property', () => {
  it('for values that do not evaluate, edges and var()', () => {
    expect(errorOf('leading-[abc]').property).toBe('line-height');
    expect(errorOf('tracking-[abc]').property).toBe('letter-spacing');
    expect(errorOf('rotate-[abc]').property).toBe('rotate');
    expect(errorOf('translate-[1pt_2pt_abc]').property).toBe('translate');
    expect(errorOf('p-[1pt_2pt_3pt_4pt_5pt]').property).toBe('padding');
    expect(errorOf('m-[var(--nope)]').property).toBe('margin');
    expect(errorOf('leading-[abc]').message).toContain('line-height: abc');
  });
});

describe('arbitrary lengths', () => {
  it('are not clamped', () => {
    expect(tw('w-[20000pt]')).toEqual({ width: 20000 });
    expect(tw('h-[12000pt]')).toEqual({ height: 12000 });
    expect(tw('-mt-[20000pt]')).toEqual({ marginTop: -20000 });
  });

  it('cap an infinite radius', () => {
    expect(tw('rounded-full')).toEqual({ borderRadius: 1e4 });
  });

  it('take infinity only inside calc(), as CSS does', () => {
    expect(tw('w-[calc(infinity*1pt)]')).toEqual({ width: 1e4 });
    expect(errorOf('w-[infinity]').reason).toBe('width: infinity (infinity is only allowed inside calc())');
    expect(errorOf('rounded-[infinity]').property).toBe('border-radius');
  });

  it('take units in any case, exponents and a space before the unit, as the style prop does', () => {
    expect(tw('w-[5PX]')).toEqual({ width: 3.75 });
    expect(tw('w-[1e1pt]')).toEqual({ width: 10 });
    expect(tw('w-[5_pt]')).toEqual({ width: 5 });
    expect(errorOf('w-[3ex]').reason).toBe('width: 3ex (unsupported unit "ex")');
  });
});

describe('em, vw and vh', () => {
  const page = { width: 600, height: 800 };

  it('resolve against the units given, and fail without them', () => {
    const units = { parentFontSize: 10, page };
    expect(tw('w-[2em] h-[10vh]', units)).toEqual({ width: 20, height: 80 });
    expect(tw('w-[calc(1em+2pt)] translate-x-[10vw]', units)).toEqual({ width: 12, translate: [60, 0] });
    expect(errorOf('w-[2em]').reason).toBe(
      'width: 2em (depends on the font or page size, which is not known here)',
    );
    expect(errorOf('h-[10vh]').property).toBe('height');
  });

  it('take the element’s font size, the inherited one in text-[…], and a style’s over the classes’', () => {
    expect(tw('text-[2em] w-[1em]', { parentFontSize: 10, page })).toEqual({ fontSize: 20, width: 20 });
    expect(tw('w-[2em] text-[15pt]', { parentFontSize: 10, page })).toEqual({ fontSize: 15, width: 30 });
    expect(tw('w-[2em] text-[15pt]', { parentFontSize: 10, fontSize: 12, page })).toEqual({
      fontSize: 15,
      width: 24,
    });
  });

  it('are cached by the units, and other classes by the class string alone', () => {
    expect(tw('w-[1em]', { parentFontSize: 10, page })).toEqual({ width: 10 });
    expect(tw('w-[1em]', { parentFontSize: 11, page })).toEqual({ width: 11 });
    expect(tw('w-[1em]', { parentFontSize: 10, page })).toEqual({ width: 10 });
    const fixed = tw('w-[7pt]', { parentFontSize: 10, page });
    expect(tw('w-[7pt]')).toBe(fixed);
    expect(tw('w-[7pt]', { parentFontSize: 11, page })).toBe(fixed);
  });

  it('keep line height and letter spacing relative to the text', () => {
    expect(tw('leading-[2em] tracking-[0.1em]', { parentFontSize: 10, page })).toEqual({
      lineHeight: { mult: 2 },
      letterSpacing: { em: 0.1 },
    });
  });
});

describe('values out of the range the engine takes', () => {
  it('fail where the class is read, naming the property', () => {
    expect(errorOf('line-clamp-0').property).toBe('-webkit-line-clamp');
    expect(errorOf('line-clamp-[2.5]').property).toBe('-webkit-line-clamp');
    expect(errorOf('opacity-[1.5]').property).toBe('opacity');
    expect(errorOf('col-span-0').property).toBe('grid-column');
    expect(errorOf('aspect-[0]').property).toBe('aspect-ratio');
    expect(errorOf('skew-x-[90deg]').property).toBe('transform');
  });
});

describe('unsupported-class reasons', () => {
  it('suggest gap only for classes that style children', () => {
    expect(errorOf('space-x-2').reason).toContain('use gap');
    expect(errorOf('container').reason).toMatch(
      /^it sizes the element by the screen width .*; use max-w-\* or w-full$/,
    );
    expect(errorOf('placeholder-red-500').reason).not.toContain('gap');
  });
});

describe('strict mode', () => {
  it('checks class strings cached while it was off', () => {
    setTailwindStrict(false);
    const warn = console.warn;
    console.warn = () => {};
    try {
      expect(tw('p-1 shadow-xl')).toEqual({
        paddingTop: 3,
        paddingRight: 3,
        paddingBottom: 3,
        paddingLeft: 3,
      });
    } finally {
      console.warn = warn;
      setTailwindStrict(true);
    }
    expect(() => tw('p-1 shadow-xl')).toThrow(UnsupportedUtilityError);
  });
});

describe('page variants', () => {
  const A5 = { parentFontSize: 12, page: { width: 419.53, height: 595.28 } };
  const LANDSCAPE = { parentFontSize: 12, page: { width: 841.89, height: 595.28 } };
  const AT_MD = { parentFontSize: 12, page: { width: 576, height: 800 } };

  it('apply on the pages they match, and are cached per page size', () => {
    expect(tw('p-2 md:p-4', A4_UNITS).paddingTop).toBe(12);
    expect(tw('p-2 md:p-4', A5).paddingTop).toBe(6);
    expect(tw('p-2 md:p-4', A4_UNITS).paddingTop).toBe(12);
    expect(tw('landscape:flex-row sm:landscape:gap-2', LANDSCAPE)).toEqual({
      flexDirection: 'row',
      rowGap: 6,
      columnGap: 6,
    });
    expect(tw('landscape:flex-row sm:landscape:gap-2', A4_UNITS)).toEqual({});
  });

  it('meet at the breakpoint without overlapping', () => {
    expect(tw('max-md:p-1 md:p-3', AT_MD).paddingTop).toBe(9);
    expect(tw('max-md:p-1 md:p-3', { ...AT_MD, page: { width: 575.99, height: 800 } }).paddingTop).toBe(3);
  });

  it('leave out the custom properties of a variant that does not match', () => {
    expect(tw('translate-y-2 md:translate-x-4', A5)).toEqual({ translate: [0, 6] });
    expect(tw('translate-y-2 md:translate-x-4', A4_UNITS)).toEqual({ translate: [12, 6] });
  });

  it('check a variant on every page size, and need the page', () => {
    expect(() => tw('md:w-[3ex]', A5)).toThrow(UnsupportedUtilityError);
    expect(() => tw('md:w-[3ex]', A4_UNITS)).toThrow(UnsupportedUtilityError);
    expect(errorOf('md:p-4').reason).toBe(
      'its page variant depends on the font or page size, which is not known here',
    );
  });

  it('name what a variant for screens lacks', () => {
    const hover = (() => {
      try {
        tw('hover:p-4', A4_UNITS);
      } catch (e) {
        return (e as UnsupportedUtilityError).reason;
      }
      return '';
    })();
    expect(hover).toBe(
      'the variant `hover:` has no meaning in a PDF (@media (hover: hover): "hover" has no meaning in a PDF); page variants work: sm: to 2xl:, max-*:, min-[…]:, max-[…]:, portrait:, landscape: and print:',
    );
    expect(errorOf('focus:p-4').reason).toMatch(
      /^the variant `focus:` has no meaning in a PDF; page variants work/,
    );
    expect(errorOf('@sm:p-4').reason).toMatch(
      /^the variant `@sm:` has no meaning in a PDF \(at-rules have no meaning/,
    );
    expect(errorOf('md:not-a-class').reason).toMatch(/^not a Tailwind utility/);
  });
});
