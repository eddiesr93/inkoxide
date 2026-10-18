// `@inkoxide/style/internal`: what @inkoxide/tailwind and @inkoxide/react use beyond the public
// entry. Each release of those packages pins the exact version of this one, so this entry is not
// covered by semver and may change in any release.
export { VERTICAL_ALIGNS, type VerticalAlignKeyword, verticalAlignKeyword } from './align.ts';
export {
  MAX_ASPECT_RATIO,
  MAX_FONT_SIZE,
  MAX_LINE_HEIGHT,
  MAX_LINES,
  MAX_WIDOW_LINES,
  MAX_Z_INDEX,
  MIN_Z_INDEX,
} from './bounds.ts';
export { ENUMS, type EnumKey, Reader } from './reader.ts';
export { DEFAULT_SANS, type Dim, evalDim, parseFontFamily, substituteVars, UNITS_UNKNOWN } from './values.ts';
export { type MediaQuery, parseMedia } from './media.ts';
