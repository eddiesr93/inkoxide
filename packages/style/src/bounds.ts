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
/** Narrowest and widest width a font declaration takes (`FontDecl.stretch`), percentages. */
export const MIN_STRETCH = 1;
export const MAX_STRETCH = 1000;
/** Largest run size, in points (`TextRun.size`). */
export const MAX_FONT_SIZE = 1000;
/** Smallest and largest `Style.zIndex`. */
export const MIN_Z_INDEX = -2_147_483_648;
export const MAX_Z_INDEX = 2_147_483_647;
/** Most lines `Style.orphans` and `Style.widows` keep together. */
export const MAX_WIDOW_LINES = 100;
/** Most lines `Style.maxLines` clamps to. */
export const MAX_LINES = 4_294_967_295;
/** Largest `Style.lineHeight` multiplier. */
export const MAX_LINE_HEIGHT = 100;
/** Largest `Style.aspectRatio`. */
export const MAX_ASPECT_RATIO = 1_000_000;
