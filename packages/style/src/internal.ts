// `@inkoxide/style/internal`: what @inkoxide/tailwind and @inkoxide/react use beyond the public
// entry. Each release of those packages pins the exact version of this one, so this entry is not
// covered by semver and may change in any release.
export { VERTICAL_ALIGNS, type VerticalAlignKeyword, verticalAlignKeyword } from './align.ts';
export { ENUMS, type EnumKey, Reader } from './reader.ts';
export { DEFAULT_SANS, type Dim, evalDim, parseFontFamily, substituteVars, UNITS_UNKNOWN } from './values.ts';
