// Bounds of the document schema that the readers check, so that a value they accept validates.
// `test/bounds.test.ts` checks them against `@inkoxide/schema`'s JSON Schema.

/** Largest grid line, either way (`GridPlacement.start`, `end`). */
export const MAX_GRID_LINE = 32_767;
/** Most tracks a grid item spans (`GridPlacement.span`). */
export const MAX_GRID_SPAN = 65_535;
/** Most OpenType features a run sets (`TextRun.fontFeatures`). */
export const MAX_FEATURES = 32;
/** Largest value of one OpenType feature. */
export const MAX_FEATURE_VALUE = 65_535;
/** Most axis values a font declaration takes (`FontDecl.variations`). */
export const MAX_VARIATIONS = 16;
/** Largest axis value, either way. */
export const MAX_VARIATION_VALUE = 32_767;
