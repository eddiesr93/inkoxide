/**
 * The public API: every value `@inkoxide/style` exports. A name added to the main entry is API
 * under semver; `@inkoxide/style/internal` is for Inkoxide's own packages, which pin this one's
 * exact version.
 */
import { describe, expect, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
import * as main from '../src/index.ts';
import * as internal from '../src/internal.ts';

const MAIN_VALUES = [
  'BORDER_STYLES',
  'FONT_STRETCHES',
  'OBJECT_FITS',
  'RUN_ALIGNS',
  'PT_PER_PX',
  'PT_PER_REM',
  'PT_PER_UNIT',
  'SVG_ONLY',
  'TEXT_INHERITED',
  'TEXT_ONLY',
  'UnsupportedValueError',
  'ValueError',
  'parseColor',
  'parseFlex',
  'parseFontFeatureSettings',
  'parseFontStretch',
  'parseFontVariantNumeric',
  'parseFontVariationSettings',
  'parseObjectPosition',
  'parseOrigin',
  'parseShape',
  'parseTransform',
  'round',
  'splitTopLevel',
];

/** Deprecated in the main entry: they moved to `@inkoxide/style/internal`. */
const DEPRECATED = ['ENUMS', 'Reader', 'evalDim', 'parseFontFamily', 'substituteVars'];

const INTERNAL_VALUES = [
  'DEFAULT_SANS',
  'ENUMS',
  'Reader',
  'UNITS_UNKNOWN',
  'VERTICAL_ALIGNS',
  'evalDim',
  'parseFontFamily',
  'parseMedia',
  'substituteVars',
  'verticalAlignKeyword',
];

const sorted = (names: string[]) => [...names].sort();

describe('exports', () => {
  it('@inkoxide/style exports exactly its public values, and the deprecated ones', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted([...MAIN_VALUES, ...DEPRECATED]));
  });

  it('keeps each deprecated value the same as the internal one', () => {
    const before: Record<string, unknown> = { ...main };
    const moved: Record<string, unknown> = { ...internal };
    for (const name of DEPRECATED) expect(before[name], name).toBe(moved[name]);
  });

  it('@inkoxide/style/internal exports what the other packages use', () => {
    expect(sorted(Object.keys(internal))).toEqual(sorted(INTERNAL_VALUES));
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({ '.': './src/index.ts', './internal': './src/internal.ts' });
  });
});
