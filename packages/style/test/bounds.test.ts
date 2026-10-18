// The bounds the readers check are the schema's.
import schema from '@inkoxide/schema/document.v1.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';

import {
  MAX_ASPECT_RATIO,
  MAX_FEATURE_VALUE,
  MAX_FEATURES,
  MAX_FONT_SIZE,
  MAX_GRID_LINE,
  MAX_GRID_SPAN,
  MAX_LINE_HEIGHT,
  MAX_LINES,
  MAX_STRETCH,
  MAX_VARIATION_VALUE,
  MAX_VARIATIONS,
  MAX_WIDOW_LINES,
  MAX_Z_INDEX,
  MIN_STRETCH,
  MIN_Z_INDEX,
} from '../src/bounds.ts';

describe('schema bounds', () => {
  it('match the JSON Schema', () => {
    const { FontDecl, GridPlacement, TextRun } = schema.$defs;
    const variations = FontDecl.properties.variations;
    expect(variations.maxProperties).toBe(MAX_VARIATIONS);
    expect([variations.additionalProperties.minimum, variations.additionalProperties.maximum]).toEqual([
      -MAX_VARIATION_VALUE,
      MAX_VARIATION_VALUE,
    ]);
    for (const side of [GridPlacement.properties.start, GridPlacement.properties.end]) {
      expect([side.minimum, side.maximum]).toEqual([-MAX_GRID_LINE, MAX_GRID_LINE]);
    }
    expect(GridPlacement.properties.span.maximum).toBe(MAX_GRID_SPAN);
    const features = TextRun.properties.fontFeatures;
    expect(features.maxProperties).toBe(MAX_FEATURES);
    expect(features.additionalProperties.maximum).toBe(MAX_FEATURE_VALUE);
    for (const stretch of [FontDecl.properties.stretch, FontDecl.properties.stretchRange.items]) {
      expect([stretch.minimum, stretch.maximum]).toEqual([MIN_STRETCH, MAX_STRETCH]);
    }
  });

  it('match the JSON Schema for the integers, sizes and ratios classes set', () => {
    const { Style, TextRun } = schema.$defs;
    expect([Style.properties.zIndex.minimum, Style.properties.zIndex.maximum]).toEqual([
      MIN_Z_INDEX,
      MAX_Z_INDEX,
    ]);
    for (const lines of [Style.properties.orphans, Style.properties.widows]) {
      expect([lines.minimum, lines.maximum]).toEqual([1, MAX_WIDOW_LINES]);
    }
    expect([Style.properties.maxLines.minimum, Style.properties.maxLines.maximum]).toEqual([1, MAX_LINES]);
    expect(Style.properties.lineHeight.maximum).toBe(MAX_LINE_HEIGHT);
    expect(Style.properties.aspectRatio.maximum).toBe(MAX_ASPECT_RATIO);
    expect(TextRun.properties.size.maximum).toBe(MAX_FONT_SIZE);
  });
});
