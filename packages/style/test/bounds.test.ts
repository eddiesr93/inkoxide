// The bounds the readers check are the schema's.
import schema from '@inkoxide/schema/document.v1.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';

import {
  MAX_FEATURE_VALUE,
  MAX_FEATURES,
  MAX_GRID_LINE,
  MAX_GRID_SPAN,
  MAX_STRETCH,
  MAX_VARIATION_VALUE,
  MAX_VARIATIONS,
  MIN_STRETCH,
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
});
