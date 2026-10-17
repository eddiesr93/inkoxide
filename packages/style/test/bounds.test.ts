// The bounds the readers check are the schema's.
import schema from '@inkoxide/schema/document.v1.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';

import { MAX_FEATURE_VALUE, MAX_FEATURES, MAX_GRID_LINE, MAX_GRID_SPAN } from '../src/bounds.ts';

describe('schema bounds', () => {
  it('match the JSON Schema', () => {
    const { GridPlacement, TextRun } = schema.$defs;
    for (const side of [GridPlacement.properties.start, GridPlacement.properties.end]) {
      expect([side.minimum, side.maximum]).toEqual([-MAX_GRID_LINE, MAX_GRID_LINE]);
    }
    expect(GridPlacement.properties.span.maximum).toBe(MAX_GRID_SPAN);
    const features = TextRun.properties.fontFeatures;
    expect(features.maxProperties).toBe(MAX_FEATURES);
    expect(features.additionalProperties.maximum).toBe(MAX_FEATURE_VALUE);
  });
});
