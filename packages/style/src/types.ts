import type {
  BorderStyles,
  Color as SchemaColor,
  GridPlacement,
  ImageNode,
  ObjectPosition,
  Style,
  TemplateTrack,
  TextRun,
  TrackSize,
  TransformOp,
} from '@inkoxide/schema';

import type { FontFeatures } from './features.ts';

/** RGBA floats in [0, 1], the engine's colour format. */
export type Color = SchemaColor;
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
export type TextDecoration = NonNullable<TextRun['decoration']>;
/** Not in the schema: the tree builder transforms the text itself. */
export type TextTransform = 'none' | 'uppercase' | 'lowercase' | 'capitalize';
export type WhiteSpace = NonNullable<Style['whiteSpace']>;
export type BorderStyle = Extract<BorderStyles, string>;
export const BORDER_STYLES = ['solid', 'dashed', 'dotted'] as const satisfies readonly BorderStyle[];
export type DecorationStyle = NonNullable<TextRun['decorationStyle']>;
export type ObjectFit = NonNullable<ImageNode['fit']>;
export const OBJECT_FITS = ['fill', 'contain', 'cover', 'none'] as const satisfies readonly ObjectFit[];
export type Direction = NonNullable<Style['direction']>;
/** Where a run sits on its line: `verticalAlign` on a `Text` (on a `Cell`, it is `top`, `middle` or `bottom`). */
export type RunAlign = NonNullable<TextRun['verticalAlign']>;
export const RUN_ALIGNS = ['baseline', 'super', 'sub'] as const satisfies readonly RunAlign[];
export type { ObjectPosition, TransformOp };

/**
 * The style of one element, flattened: what `tw()` reads from a class string and the React tree
 * builder from a `style` prop, before it becomes the schema's style. Edges are separate keys so
 * that `px-4 pt-2` merge without allocating objects; the tree builder folds them into the
 * schema's `{ top, right, … }`.
 *
 * Text properties (the `TEXT_INHERITED` keys) cascade from `Page` to `View` to `Text` to nested
 * `Text`. Everything else applies to the element that carries the class only.
 */
export interface FlatStyle {
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
  /** `top`, `middle` or `bottom` on a `Cell`; `baseline`, `super` or `sub` on a `Text` and the runs in it. */
  verticalAlign?: Style['verticalAlign'] | RunAlign;
  /** How an `Image` fills its box: the image node's `fit`. */
  objectFit?: ObjectFit;
  /** Where a fitted `Image` sits in its box: the image node's `objectPosition`. */
  objectPosition?: ObjectPosition;

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
  /** OpenType features (`font-feature-settings`); they win over `fontVariantNumeric`'s. */
  fontFeatureSettings?: FontFeatures;
  /** The features `font-variant-numeric` turns on (`tabular-nums`: `tnum`). */
  fontVariantNumeric?: FontFeatures;

  // Text, not inherited (the Text node that carries the class).
  maxLines?: number;
  ellipsis?: boolean;
}

export type FlatStyleKey = keyof FlatStyle;

/** @deprecated Renamed `FlatStyle`: `style` props build it too, not only Tailwind classes. */
export type TwStyle = FlatStyle;
/** @deprecated Renamed `FlatStyleKey`. */
export type TwKey = FlatStyleKey;

/**
 * Keys that cascade to descendants, like inherited CSS properties. The React tree builder's text
 * context follows this list, so a key added here is inherited without other changes.
 */
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
  'fontFeatureSettings',
  'fontVariantNumeric',
] as const satisfies readonly FlatStyleKey[];

/** Keys that only mean something on a `Text` node. */
export const TEXT_ONLY = ['maxLines', 'ellipsis'] as const satisfies readonly FlatStyleKey[];
