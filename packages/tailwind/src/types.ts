import type { GridPlacement, Style, TemplateTrack, TrackSize, TransformOp } from '@inkoxide/schema';

/** RGBA floats in [0, 1], the engine's colour format. */
export type Color = [number, number, number, number];
/** A colour, or CSS `currentColor` (resolved against the inherited text colour by the tree builder). */
export type ColorValue = Color | 'currentColor';
/** Points, a percentage string (`"50%"`) or `"auto"`. */
export type Len = number | `${number}%` | 'auto';
/** Points or a percentage string. */
export type LenPercent = number | `${number}%`;

/**
 * Line height before inheritance is resolved. A length (`leading-6`) only becomes the engine's
 * multiplier once the text size is known, so the tree builder converts `{ pt }` to `pt / fontSize`.
 */
export type LineHeight = { mult: number } | { pt: number };
/** Letter spacing before inheritance is resolved: `em` needs the run's font size. */
export type LetterSpacing = { pt: number } | { em: number };

export type TextAlign = NonNullable<Style['textAlign']>;
export type TextDecoration = 'none' | 'underline' | 'lineThrough';
export type TextTransform = 'none' | 'uppercase' | 'lowercase' | 'capitalize';
export type WhiteSpace = NonNullable<Style['whiteSpace']>;
export type BorderStyle = 'solid' | 'dashed' | 'dotted';
export type DecorationStyle = 'solid' | 'double' | 'dotted' | 'dashed' | 'wavy';
export type Direction = 'ltr' | 'rtl' | 'auto';
export type { TransformOp };

/**
 * The style of one class string, flattened. Edges are separate keys so that `px-4 pt-2` merge
 * without allocating objects; the tree builder folds them into the schema's `{ top, right, … }`.
 *
 * Text properties (the `TEXT_INHERITED` keys) cascade from `Page` to `View` to `Text` to nested
 * `Text`. Everything else applies to the element that carries the class only.
 */
export interface TwStyle {
  display?: Style['display'];
  flexDirection?: Style['flexDirection'];
  flexWrap?: Style['flexWrap'];
  justifyContent?: Style['justifyContent'];
  alignItems?: Style['alignItems'];
  alignSelf?: Style['alignSelf'];
  alignContent?: Style['alignContent'];
  justifyItems?: Style['justifyItems'];
  justifySelf?: Style['justifySelf'];
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: Len;
  rowGap?: LenPercent;
  columnGap?: LenPercent;
  width?: Len;
  height?: Len;
  minWidth?: Len;
  minHeight?: Len;
  maxWidth?: Len;
  maxHeight?: Len;
  aspectRatio?: number;
  paddingTop?: LenPercent;
  paddingRight?: LenPercent;
  paddingBottom?: LenPercent;
  paddingLeft?: LenPercent;
  marginTop?: Len;
  marginRight?: Len;
  marginBottom?: Len;
  marginLeft?: Len;
  position?: Style['position'];
  insetTop?: Len;
  insetRight?: Len;
  insetBottom?: Len;
  insetLeft?: Len;
  gridTemplateColumns?: TemplateTrack[];
  gridTemplateRows?: TemplateTrack[];
  gridAutoColumns?: TrackSize[];
  gridAutoRows?: TrackSize[];
  gridAutoFlow?: Style['gridAutoFlow'];
  gridColumn?: GridPlacement;
  gridRow?: GridPlacement;
  overflow?: Style['overflow'];
  backgroundColor?: ColorValue;
  opacity?: number;
  borderTopWidth?: number;
  borderRightWidth?: number;
  borderBottomWidth?: number;
  borderLeftWidth?: number;
  borderTopColor?: ColorValue;
  borderRightColor?: ColorValue;
  borderBottomColor?: ColorValue;
  borderLeftColor?: ColorValue;
  /** One radius for every corner; the corner keys below override it. */
  borderRadius?: number;
  borderTopLeftRadius?: number;
  borderTopRightRadius?: number;
  borderBottomRightRadius?: number;
  borderBottomLeftRadius?: number;
  /** One style for every side; the side keys below override it. */
  borderStyle?: BorderStyle;
  borderTopStyle?: BorderStyle;
  borderRightStyle?: BorderStyle;
  borderBottomStyle?: BorderStyle;
  borderLeftStyle?: BorderStyle;
  /**
   * CSS's individual transform properties and `transform`. The tree builder composes them in
   * CSS order: `translate`, `rotate`, `scale`, then the `transform` list.
   */
  rotate?: number;
  translate?: [LenPercent, LenPercent];
  scale?: [number, number];
  transform?: TransformOp[];
  transformOrigin?: [LenPercent, LenPercent];
  zIndex?: number;
  borderCollapse?: Style['borderCollapse'];
  borderSpacing?: number;
  verticalAlign?: Style['verticalAlign'];

  // Text, inherited.
  fontFamily?: string[];
  fontWeight?: number;
  fontStyle?: 'normal' | 'italic';
  fontSize?: number;
  color?: Color;
  lineHeight?: LineHeight;
  letterSpacing?: LetterSpacing;
  textAlign?: TextAlign;
  textDecoration?: TextDecoration;
  decorationColor?: Color;
  /** Points. */
  decorationThickness?: number;
  decorationStyle?: DecorationStyle;
  textTransform?: TextTransform;
  whiteSpace?: WhiteSpace;
  hyphenate?: boolean;
  /** Points; negative for a hanging indent. */
  textIndent?: number;
  direction?: Direction;
  orphans?: number;
  widows?: number;

  // Text, not inherited (the Text node that carries the class).
  maxLines?: number;
  ellipsis?: boolean;
}

export type TwKey = keyof TwStyle;

/** Keys that cascade to descendants, like inherited CSS properties. */
export const TEXT_INHERITED = [
  'fontFamily',
  'fontWeight',
  'fontStyle',
  'fontSize',
  'color',
  'lineHeight',
  'letterSpacing',
  'textAlign',
  'textDecoration',
  'decorationColor',
  'decorationThickness',
  'decorationStyle',
  'textTransform',
  'whiteSpace',
  'hyphenate',
  'textIndent',
  'direction',
  'orphans',
  'widows',
] as const satisfies readonly TwKey[];

/** Keys that only mean something on a `Text` node. */
export const TEXT_ONLY = ['maxLines', 'ellipsis'] as const satisfies readonly TwKey[];
