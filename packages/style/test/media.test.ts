import { describe, expect, it } from 'vitest';

import { MAX_MEDIA_QUERY, parseMedia } from '../src/media.ts';

const A4 = { width: 595.28, height: 841.89 };
const A4_LANDSCAPE = { width: 841.89, height: 595.28 };
const A5 = { width: 419.53, height: 595.28 };
const LETTER = { width: 612, height: 792 };
const SQUARE = { width: 500, height: 500 };

const on = (query: string) => {
  const q = parseMedia(query);
  return [A4, A4_LANDSCAPE, A5, LETTER, SQUARE].map((p) => q.matches(p));
};

describe('parseMedia', () => {
  it('reads widths and heights in every form', () => {
    expect(on('(min-width: 48rem)')).toEqual([true, true, false, true, false]);
    expect(on('(width >= 48rem)')).toEqual([true, true, false, true, false]);
    expect(on('(48rem <= width)')).toEqual([true, true, false, true, false]);
    expect(on('(max-width: 210mm)')).toEqual([true, false, true, false, true]);
    expect(on('(width < 48rem)')).toEqual([false, false, true, false, true]);
    expect(on('(400pt <= width < 600pt)')).toEqual([true, false, true, false, true]);
    expect(on('(600pt > width > 400pt)')).toEqual([true, false, true, false, true]);
    expect(on('(height > 11in)')).toEqual([true, false, false, false, false]);
    expect(on('(min-height: 1056px)')).toEqual([true, false, false, true, false]);
    expect(on('(width: 612pt)')).toEqual([false, false, false, true, false]);
    expect(on('(width = 21cm)')).toEqual([true, false, false, false, false]);
    expect(on('(width)')).toEqual([true, true, true, true, true]);
    // 1pc is 12pt.
    expect(on('(width > 50pc)')).toEqual([false, true, false, true, false]);
  });

  it('meets the presets at their boundaries, to 1/100 pt', () => {
    // 210mm is 595.2756pt; A4 is 595.28pt.
    expect(parseMedia('(max-width: 210mm)').matches(A4)).toBe(true);
    expect(parseMedia('(min-width: 210mm)').matches(A4)).toBe(true);
    // 48rem is 576pt: `md:` and `max-md:` never both match.
    const at = { width: 576, height: 800 };
    expect(parseMedia('(width >= 48rem)').matches(at)).toBe(true);
    expect(parseMedia('(width < 48rem)').matches(at)).toBe(false);
    // Orientation rounds the sides too: a page square to 1/100 pt is portrait.
    const near = { width: 500.004, height: 500 };
    expect(parseMedia('(orientation: portrait)').matches(near)).toBe(true);
    expect(parseMedia('(orientation: landscape)').matches(near)).toBe(false);
  });

  it('reads orientation and aspect ratios; a square page is portrait', () => {
    expect(on('(orientation: portrait)')).toEqual([true, false, true, true, true]);
    expect(on('(orientation: landscape)')).toEqual([false, true, false, false, false]);
    expect(on('(min-aspect-ratio: 1/1)')).toEqual([false, true, false, false, true]);
    expect(on('(aspect-ratio > 1)')).toEqual([false, true, false, false, false]);
    expect(on('(max-aspect-ratio: 3/4)')).toEqual([true, false, true, false, false]);
    expect(on('(aspect-ratio: 1/1)')).toEqual([false, false, false, false, true]);
  });

  it('combines with and, or, not, media types and lists', () => {
    expect(on('print')).toEqual([true, true, true, true, true]);
    expect(on('screen')).toEqual([false, false, false, false, false]);
    expect(on('only print and (orientation: landscape)')).toEqual([false, true, false, false, false]);
    expect(on('not print')).toEqual([false, false, false, false, false]);
    expect(on('not screen and (width > 1pt)')).toEqual([true, true, true, true, true]);
    expect(on('all and (min-width: 600pt) and (orientation: portrait)')).toEqual([
      false,
      false,
      false,
      true,
      false,
    ]);
    expect(on('(orientation: landscape) or (width < 450pt)')).toEqual([false, true, true, false, false]);
    expect(on('not (orientation: landscape)')).toEqual([true, false, true, true, true]);
    expect(on('((width > 600pt) and (height > 600pt)) or (width < 450pt)')).toEqual([
      false,
      false,
      true,
      true,
      false,
    ]);
    expect(on('screen, (orientation: landscape)')).toEqual([false, true, false, false, false]);
  });

  it('refuses what a page does not have, and what does not parse', () => {
    const error = (q: string) => {
      try {
        parseMedia(q);
      } catch (e) {
        return (e as Error).message;
      }
      throw new Error(`${q} parsed`);
    };
    expect(error('(hover: hover)')).toBe('@media (hover: hover): "hover" has no meaning in a PDF');
    expect(error('(prefers-color-scheme: dark)')).toContain('"prefers-color-scheme" has no meaning in a PDF');
    expect(error('(min-resolution: 2dppx)')).toContain('"min-resolution" has no meaning in a PDF');
    expect(error('(colour: 1)')).toContain('unknown media feature "colour"');
    expect(error('(min-width: 30em)')).toContain('"30em" has no fixed size in a media query');
    expect(error('(width > 50vw)')).toContain('has no fixed size');
    expect(error('(width > 50%)')).toContain('has no fixed size');
    expect(error('(width > 5)')).toContain('width takes a length');
    expect(error('(width > 1pt) and (width < 2pt) or (height > 1pt)')).toContain('cannot be mixed');
    expect(error('print and (width > 1pt) or (height > 1pt)')).toContain('"or" cannot follow a media type');
    expect(error('print or (width > 1pt)')).toContain('unexpected "or"');
    expect(error('(width > 1pt')).toContain('the query ends too early');
    expect(error('(width > 1pt))')).toContain('unexpected ")"');
    expect(error('(1pt < width > 2pt)')).toContain('two "<" or two ">"');
    expect(error('tv')).toContain('unknown media type "tv"');
    expect(error('(orientation: sideways)')).toContain('portrait or landscape');
    expect(error('(aspect-ratio: 0/1)')).toContain('two positive numbers');
    expect(error('   ')).toContain('the query is empty');
    expect(error('(min-width: 30em)')).toContain('use pt, mm, cm, in, px, pc or rem');
    expect(error('(orientation > 1)')).toContain('orientation takes portrait or landscape, not a range');
    expect(error('(min-orientation: portrait)')).toContain('unknown media feature "min-orientation"');
    expect(error('(constructor)')).toContain('unknown media feature "constructor"');
  });

  it('reads a query of at most 1000 characters', () => {
    // `(width > 1…1pt)`: 12 characters besides the digits.
    const query = (length: number) => `(width > ${'1'.repeat(length - 12)}pt)`;
    expect(query(MAX_MEDIA_QUERY)).toHaveLength(1000);
    expect(parseMedia(query(MAX_MEDIA_QUERY)).matches(A4)).toBe(false);
    expect(() => parseMedia(query(MAX_MEDIA_QUERY + 1))).toThrow('a query is at most 1000 characters');
  });
});
