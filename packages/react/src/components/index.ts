import type { Break, Color, PageFilter, TransformOp } from '@inkoxide/schema';
import type { TwStyle } from '@inkoxide/tailwind';
import { createElement, type FC, type ReactNode } from 'react';

/**
 * Host element types. Components are host elements, not function components: one fiber per
 * element and no wrapper overhead, which matters for a 100-page report.
 */
export const HOST = {
  Document: 'ink-document',
  Page: 'ink-page',
  Header: 'ink-header',
  Footer: 'ink-footer',
  View: 'ink-view',
  Text: 'ink-text',
  Link: 'ink-link',
  Image: 'ink-image',
  Table: 'ink-table',
  Row: 'ink-row',
  Cell: 'ink-cell',
  PageNumber: 'ink-page-number',
  TotalPages: 'ink-total-pages',
  PageBreak: 'ink-page-break',
  Spacer: 'ink-spacer',
  Field: 'ink-field',
  Canvas: 'ink-canvas',
  Svg: 'ink-svg',
  G: 'ink-svg-g',
  Path: 'ink-svg-path',
  Rect: 'ink-svg-rect',
  Circle: 'ink-svg-circle',
  Ellipse: 'ink-svg-ellipse',
  Line: 'ink-svg-line',
  Polyline: 'ink-svg-polyline',
  Polygon: 'ink-svg-polygon',
  Defs: 'ink-svg-defs',
  ClipPath: 'ink-svg-clip-path',
  LinearGradient: 'ink-svg-linear-gradient',
  RadialGradient: 'ink-svg-radial-gradient',
  Stop: 'ink-svg-stop',
} as const;

export type HostType = (typeof HOST)[keyof typeof HOST];

/** Component name for a host type, for error messages and component stacks. */
export const HOST_NAME: Record<string, string> = Object.fromEntries(
  Object.entries(HOST).map(([name, type]) => [type, name]),
);

type Edges<T> = T | { top?: T; right?: T; bottom?: T; left?: T };

/** A length: points as a number, or a string with a unit (`'20mm'`, `'1.5cm'`, `'1in'`, `'12pt'`, `'16px'`). */
export type Length = number | `${number}${'pt' | 'mm' | 'cm' | 'in' | 'px'}`;
/** A length or a percentage of the containing block. */
export type LengthPercent = Length | `${number}%`;
export type LengthAuto = LengthPercent | 'auto';

type BorderLineStyle = 'solid' | 'dashed' | 'dotted';

/**
 * Inline styles: the same properties as Tailwind produces, applied after `className`. Colours
 * may be CSS strings (`'#1e4a99'`, `'oklch(…)'`) or RGBA float tuples; lengths are points or
 * strings with a unit (`'20mm'`).
 */
export interface StyleProp extends Omit<
  TwStyle,
  | 'fontWeight'
  | 'color'
  | 'backgroundColor'
  | 'lineHeight'
  | 'letterSpacing'
  | `padding${string}`
  | `margin${string}`
  | `inset${string}`
  | `border${'Top' | 'Right' | 'Bottom' | 'Left'}${'Width' | 'Color' | 'Style'}`
  | `border${string}Radius`
  | 'borderStyle'
  | 'fontFamily'
  | 'fontSize'
  | 'width'
  | 'height'
  | 'minWidth'
  | 'minHeight'
  | 'maxWidth'
  | 'maxHeight'
  | 'flexBasis'
  | 'rowGap'
  | 'columnGap'
  | 'borderSpacing'
  | 'textIndent'
  | 'decorationColor'
  | 'decorationThickness'
  | 'translate'
  | 'transform'
  | 'transformOrigin'
> {
  fontFamily?: string | string[];
  /** 100–900, or a CSS name (`'bold'`, `'light'`, `'semibold'`). */
  fontWeight?: number | FontWeightName;
  fontSize?: Length;
  color?: string | Color;
  backgroundColor?: string | Color;
  /** Multiplier of the text size. */
  lineHeight?: number;
  letterSpacing?: Length;
  width?: LengthAuto;
  height?: LengthAuto;
  minWidth?: LengthAuto;
  minHeight?: LengthAuto;
  maxWidth?: LengthAuto;
  maxHeight?: LengthAuto;
  flexBasis?: LengthAuto;
  gap?: LengthPercent;
  rowGap?: LengthPercent;
  columnGap?: LengthPercent;
  padding?: Edges<LengthPercent>;
  /** Per side, and `paddingHorizontal` / `paddingVertical`; these win over `padding`. */
  paddingTop?: LengthPercent;
  paddingRight?: LengthPercent;
  paddingBottom?: LengthPercent;
  paddingLeft?: LengthPercent;
  paddingHorizontal?: LengthPercent;
  paddingVertical?: LengthPercent;
  margin?: Edges<LengthAuto>;
  marginTop?: LengthAuto;
  marginRight?: LengthAuto;
  marginBottom?: LengthAuto;
  marginLeft?: LengthAuto;
  marginHorizontal?: LengthAuto;
  marginVertical?: LengthAuto;
  inset?: Edges<LengthAuto>;
  /** Offsets of a `position: 'absolute'` or `'relative'` box. */
  top?: LengthAuto;
  right?: LengthAuto;
  bottom?: LengthAuto;
  left?: LengthAuto;
  /** `flex: 1` grows, shrinks and starts from 0; strings are CSS's `'1 0 auto'`, `'none'`, `'auto'`. */
  flex?: number | string;
  borderWidth?: Edges<Length>;
  borderColor?: Edges<string | Color>;
  /** One side at a time (`borderBottomWidth: 0.5, borderBottomColor: '#ddd'`). */
  borderTopWidth?: Length;
  borderRightWidth?: Length;
  borderBottomWidth?: Length;
  borderLeftWidth?: Length;
  borderTopColor?: string | Color;
  borderRightColor?: string | Color;
  borderBottomColor?: string | Color;
  borderLeftColor?: string | Color;
  borderTopStyle?: BorderLineStyle;
  borderRightStyle?: BorderLineStyle;
  borderBottomStyle?: BorderLineStyle;
  borderLeftStyle?: BorderLineStyle;
  /** One style, or one per side. */
  borderStyle?: Edges<BorderLineStyle>;
  /** One radius, or one per corner. */
  borderRadius?: Length | { topLeft?: Length; topRight?: Length; bottomRight?: Length; bottomLeft?: Length };
  borderSpacing?: Length;
  /** First-line indent; negative for a hanging indent. */
  textIndent?: Length;
  decorationColor?: string | Color;
  decorationThickness?: Length;
  translate?: [LengthPercent, LengthPercent];
  /** Transform functions (`[{ rotate: 10 }, { scale: [1.2, 1.2] }]`) or a CSS transform string. */
  transform?: TransformOp[] | string;
  /** `[x, y]` or a CSS value (`'top left'`, `'50% 0'`). */
  transformOrigin?: [LengthPercent, LengthPercent] | string;
}

/** One style object, or a list of them merged left to right (falsy entries are skipped). */
export type Style = StyleProp | ReadonlyArray<StyleProp | false | null | undefined>;

/** Named styles, typed once (`StyleSheet.create({ title: { fontSize: 18 } })`). */
export const StyleSheet = {
  create<T extends Record<string, StyleProp>>(styles: T): T {
    return styles;
  },
};

/** `break="avoid"` keeps the element on one page; `"page-before"` / `"page-after"` force a break. */
export type FontWeightName =
  | 'thin'
  | 'hairline'
  | 'ultralight'
  | 'extralight'
  | 'light'
  | 'normal'
  | 'regular'
  | 'medium'
  | 'semibold'
  | 'demibold'
  | 'bold'
  | 'ultrabold'
  | 'extrabold'
  | 'heavy'
  | 'black';

export type BreakProp = 'avoid' | 'page-before' | 'page-after' | Break;

export type BookmarkProp = string | { title: string; level?: number };

/**
 * Splits a word into the parts a line may break between: `'hyphenation'` →
 * `['hy', 'phen', 'ation']`. `[word]` keeps the word whole.
 */
export type HyphenationCallback = (word: string) => string[];

interface Common {
  className?: string;
  style?: Style;
  /** Node id: the target of `href="#id"` links. */
  id?: string;
  children?: ReactNode;
}

interface Flow extends Common {
  /** `true` is the same as `'page-before'`. */
  break?: BreakProp | boolean;
  /** `wrap={false}` is the same as `break="avoid"`. */
  wrap?: boolean;
  /** The same as `break={{ minPresenceAhead }}`, in points. */
  minPresenceAhead?: number;
  /** Not supported: put content that repeats on every page in `<Header>` or `<Footer>`. */
  fixed?: never;
  /**
   * Pages that paint this element (it keeps its layout space on the others): `'first'`,
   * `'last'`, `'notFirst'`, `'notLast'`, `'odd'`, `'even'`, or page numbers (negative from the
   * end, `[-1]` is the last page).
   */
  pages?: PageFilter;
  /** Makes the element a link: `https://…`, `mailto:…` or `#id`. */
  href?: string;
  /** Adds an entry to the PDF outline. */
  bookmark?: BookmarkProp;
}

export interface DocumentProps {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string;
  creator?: string;
  /** BCP-47 language of the document (`"en-US"`); also selects hyphenation patterns. */
  lang?: string;
  /** RFC 3339 timestamp written to the PDF. Omit it for deterministic output. */
  creationDate?: string;
  /** Font ids tried for characters missing from a run's font. Default: `['NotoSans-Regular']`. */
  fallbackFonts?: string[];
  children?: ReactNode;
}

type Series<P extends string> = `${P}${0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10}`;

export type PagePreset =
  | '4A0'
  | '2A0'
  | Series<'A' | 'B' | 'C'>
  | `${'RA' | 'SRA'}${0 | 1 | 2 | 3 | 4}`
  | 'Letter'
  | 'Legal'
  | 'Tabloid'
  | 'Executive'
  | 'Folio'
  | 'ID1';

/** A preset name in any case (`'A4'`, `'letter'`, `'LEGAL'`). */
type PresetName = PagePreset | Lowercase<PagePreset> | Uppercase<PagePreset>;

export type PageSize =
  | PresetName
  | `${PresetName}-landscape`
  | Length
  | [width: Length, height: Length]
  | [side: Length]
  | {
      width: Length;
      height: Length;
    };

export interface PageProps {
  /**
   * A preset (`'A4'`, `'letter'`, `'A5-landscape'`), `[width, height]`, `{ width, height }`, or
   * `[width]` or a single length for a square page. Lengths are points or strings with a unit
   * (`'210mm'`). Default `A4`.
   */
  size?: PageSize;
  /** Turns the size sideways (`landscape`) or upright (`portrait`). */
  orientation?: 'portrait' | 'landscape';
  /** One length for all sides or per side (points, or `'15mm'`). */
  margin?: Edges<Length>;
  /** Text utilities set the defaults inherited by the page's content; `bg-*` sets the page background. */
  className?: string;
  style?: Style;
  children?: ReactNode;
}

export type HeaderProps = Common;
export type FooterProps = Common;

/** What a `render` prop receives. */
export interface PageInfo {
  /** Physical page number, from 1. */
  pageNumber: number;
  /** Pages in the document. */
  totalPages: number;
}

/**
 * Content computed per page: called once for every page the document has (after a first pass
 * that counts them), and painted only on that page. Pages with identical output share one node.
 */
export type RenderProp = (page: PageInfo) => ReactNode;

export interface ViewProps extends Flow {
  render?: RenderProp;
}

export interface TextProps extends Flow {
  /** Hyphenate this text (same as `hyphens-auto`). Needs patterns for its language. */
  hyphenate?: boolean;
  /** Language of this text, when it differs from the document's. */
  lang?: string;
  /**
   * Breaks words where this function says, instead of the built-in patterns (`hyphenate`).
   * Overrides the registered callback (`Font.registerHyphenationCallback`); `null` turns it off
   * for this text.
   */
  hyphenationCallback?: HyphenationCallback | null;
  /** Base direction: `rtl` for Arabic or Hebrew paragraphs, `auto` from the first strong letter. */
  dir?: 'ltr' | 'rtl' | 'auto';
  /** Minimum lines left at the bottom of a page before a break (default 1). */
  orphans?: number;
  /** Minimum lines carried to the top of the next page (default 1). */
  widows?: number;
  render?: RenderProp;
}

export interface LinkProps extends Flow {
  /** The same as `href`. */
  src?: string;
}

export interface ImageProps extends Omit<Flow, 'children'> {
  /**
   * Bytes (JPEG, PNG, GIF, WebP, SVG), a `data:` URL, the id of an image passed in the render
   * options, or an `https:` URL allowed by the render option `remoteImages`.
   */
  src: Uint8Array | string;
  fit?: 'fill' | 'contain' | 'cover' | 'none';
}

export interface TableProps extends Flow {
  /** Column widths: points, `'1fr'`-style fractions, or `'auto'`. */
  columns: Array<number | string>;
  /** Rows repeated at the top of every page the table spans. */
  headerRows?: number;
}

export interface RowProps {
  /** `bg-*`, `min-h-*` and text utilities (inherited by the cells). */
  className?: string;
  style?: Style;
  id?: string;
  minHeight?: number;
  children?: ReactNode;
}

export interface CellProps extends Common {
  colSpan?: number;
  rowSpan?: number;
  verticalAlign?: 'top' | 'middle' | 'bottom';
}

export interface SpacerProps {
  /** Height in points. */
  h?: number;
  /** Width in points. */
  w?: number;
  className?: string;
  style?: Style;
  id?: string;
}

export interface PageBreakProps {
  id?: string;
}

interface FieldCommon extends Omit<Flow, 'children' | 'href' | 'bookmark'> {
  /** Field name, unique in the document (radio buttons of one group share it). No `.`. */
  name: string;
  readOnly?: boolean;
  required?: boolean;
}

export interface TextFieldProps extends FieldCommon {
  value?: string;
  multiline?: boolean;
  /** The value is neither shown nor stored. */
  password?: boolean;
  maxLength?: number;
}

export interface CheckboxProps extends FieldCommon {
  checked?: boolean;
}

export interface RadioProps extends FieldCommon {
  /** The value the group exports when this button is selected. */
  value: string;
  checked?: boolean;
}

export interface SelectProps extends FieldCommon {
  options: string[];
  value?: string;
}

/** Any field; `kind` picks the type. The font, size and colour come from the text utilities. */
export type FieldProps =
  | ({ kind: 'text' } & TextFieldProps)
  | ({ kind: 'checkbox' } & CheckboxProps)
  | ({ kind: 'radio' } & RadioProps)
  | ({ kind: 'select' } & SelectProps);

type SvgPaint = string;
/** SVG presentation attributes, camelCase (`strokeWidth`), as in React DOM. */
export interface SvgPresentation {
  fill?: SvgPaint;
  fillOpacity?: number | string;
  fillRule?: 'nonzero' | 'evenodd';
  stroke?: SvgPaint;
  strokeWidth?: number | string;
  strokeOpacity?: number | string;
  strokeLinecap?: 'butt' | 'round' | 'square';
  strokeLinejoin?: 'miter' | 'round' | 'bevel';
  strokeDasharray?: string | number[];
  strokeDashoffset?: number | string;
  strokeMiterlimit?: number | string;
  opacity?: number | string;
  transform?: string;
  clipPath?: string;
  clipRule?: 'nonzero' | 'evenodd';
  id?: string;
  children?: ReactNode;
}

export interface SvgProps extends SvgPresentation {
  /** Width and height of the drawing in the layout (points or units); also the SVG viewport. */
  width: Length;
  height: Length;
  viewBox?: string;
  preserveAspectRatio?: string;
  /** Box utilities for the image the drawing becomes (margins, position, `pages`…). */
  className?: string;
  style?: Style;
  pages?: PageFilter;
  break?: BreakProp;
  href?: string;
}

export interface PathProps extends SvgPresentation {
  d: string;
}
export interface RectProps extends SvgPresentation {
  x?: number | string;
  y?: number | string;
  width: number | string;
  height: number | string;
  rx?: number | string;
  ry?: number | string;
}
export interface CircleProps extends SvgPresentation {
  cx?: number | string;
  cy?: number | string;
  r: number | string;
}
export interface EllipseProps extends SvgPresentation {
  cx?: number | string;
  cy?: number | string;
  rx: number | string;
  ry: number | string;
}
export interface LineProps extends SvgPresentation {
  x1: number | string;
  y1: number | string;
  x2: number | string;
  y2: number | string;
}
export interface PolyProps extends SvgPresentation {
  points: string | number[];
}
export interface GradientProps {
  id: string;
  x1?: number | string;
  y1?: number | string;
  x2?: number | string;
  y2?: number | string;
  cx?: number | string;
  cy?: number | string;
  r?: number | string;
  fx?: number | string;
  fy?: number | string;
  gradientUnits?: 'userSpaceOnUse' | 'objectBoundingBox';
  gradientTransform?: string;
  children?: ReactNode;
}
export interface StopProps {
  offset: number | string;
  stopColor: string;
  stopOpacity?: number | string;
}

export interface CanvasProps extends Omit<
  SvgProps,
  keyof SvgPresentation | 'viewBox' | 'preserveAspectRatio'
> {
  /** Draws with a pdfkit-style painter; `width` and `height` are the canvas size in points. */
  paint: (painter: import('../canvas.ts').Painter, width: number, height: number) => void;
}

const host = <P>(type: HostType): FC<P> => type as unknown as FC<P>;

export const Document = host<DocumentProps>(HOST.Document);
export const Page = host<PageProps>(HOST.Page);
export const Header = host<HeaderProps>(HOST.Header);
export const Footer = host<FooterProps>(HOST.Footer);
export const View = host<ViewProps>(HOST.View);
export const Text = host<TextProps>(HOST.Text);
/**
 * A link to `href` (`https://…`, `mailto:…` or `#id`; `src` is the same). Inside a `Text` it is
 * an inline run; elsewhere a box, like a `View` with `href`. It adds no colour or underline.
 */
export const Link = host<LinkProps>(HOST.Link);
export const Image = host<ImageProps>(HOST.Image);
export const Table = host<TableProps>(HOST.Table);
export const Row = host<RowProps>(HOST.Row);
export const Cell = host<CellProps>(HOST.Cell);
/** The current page number. Only inside `Text`. */
export const PageNumber = host<Record<string, never>>(HOST.PageNumber);
/** The total page count. Only inside `Text`. */
export const TotalPages = host<Record<string, never>>(HOST.TotalPages);
export const PageBreak = host<PageBreakProps>(HOST.PageBreak);
export const Spacer = host<SpacerProps>(HOST.Spacer);
/** An interactive form field (AcroForm). */
export const Field = host<FieldProps>(HOST.Field);
/** A vector drawing; becomes an SVG image. Use the SVG primitives (`Path`, `Rect`, …) inside. */
export const Svg = host<SvgProps>(HOST.Svg);
export const G = host<SvgPresentation>(HOST.G);
export const Path = host<PathProps>(HOST.Path);
export const Rect = host<RectProps>(HOST.Rect);
export const Circle = host<CircleProps>(HOST.Circle);
export const Ellipse = host<EllipseProps>(HOST.Ellipse);
export const Line = host<LineProps>(HOST.Line);
export const Polyline = host<PolyProps>(HOST.Polyline);
export const Polygon = host<PolyProps>(HOST.Polygon);
export const Defs = host<{ children?: ReactNode }>(HOST.Defs);
export const ClipPath = host<{ id: string; children?: ReactNode }>(HOST.ClipPath);
export const LinearGradient = host<GradientProps>(HOST.LinearGradient);
export const RadialGradient = host<GradientProps>(HOST.RadialGradient);
export const Stop = host<StopProps>(HOST.Stop);
/** Imperative drawing with a painter (`paint={(p, w, h) => p.circle(w / 2, h / 2, 20).fill('red')}`). */
export const Canvas = host<CanvasProps>(HOST.Canvas);

function field<P extends object>(kind: FieldProps['kind'], displayName: string): FC<P> {
  const f: FC<P> = (props) => createElement(HOST.Field, { ...props, kind });
  f.displayName = displayName;
  return f;
}
/** A text field. */
export const TextField: FC<TextFieldProps> = field('text', 'TextField');
/** A checkbox. */
export const Checkbox: FC<CheckboxProps> = field('checkbox', 'Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio: FC<RadioProps> = field('radio', 'Radio');
/** A drop-down list. */
export const Select: FC<SelectProps> = field('select', 'Select');
