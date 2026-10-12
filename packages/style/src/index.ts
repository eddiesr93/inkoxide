// Style values for Inkoxide: the style object that @inkoxide/react builds from `style` props and
// @inkoxide/tailwind from classes, and the readers of CSS lengths, colours and transforms into it.
export {
  ENUMS,
  type EnumKey,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  Reader,
  UnsupportedValueError,
} from './reader.ts';
export * from './types.ts';
export {
  type Dim,
  evalDim,
  PT_PER_PX,
  PT_PER_REM,
  PT_PER_UNIT,
  parseColor,
  parseFlex,
  parseFontFamily,
  round,
  splitTopLevel,
  substituteVars,
  ValueError,
} from './values.ts';
