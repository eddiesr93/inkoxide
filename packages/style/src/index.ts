// Style values for Inkoxide: the flat style object that @inkoxide/react builds from `style` props
// and @inkoxide/tailwind from classes, the CSS units, and the readers of the values both take
// alike: colours, `flex`, transforms, transform origins and object positions.
import * as internal from './internal.ts';

export { parseObjectPosition, parseOrigin, parseTransform, UnsupportedValueError } from './reader.ts';
export * from './types.ts';
export {
  PT_PER_PX,
  PT_PER_REM,
  PT_PER_UNIT,
  parseColor,
  parseFlex,
  round,
  splitTopLevel,
  ValueError,
} from './values.ts';

// Moved to `@inkoxide/style/internal`: only @inkoxide/tailwind uses them.

/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export const ENUMS = internal.ENUMS;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export type EnumKey = internal.EnumKey;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export const Reader = internal.Reader;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export type Reader = internal.Reader;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export const evalDim = internal.evalDim;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export type Dim = internal.Dim;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export const substituteVars = internal.substituteVars;
/** @deprecated Internal to Inkoxide's packages, and will be removed from this entry. */
export const parseFontFamily = internal.parseFontFamily;
