import type { Break, Color, Meta, PageFilter, TransformOp } from '@inkoxide/schema';
import type { FlatStyle, FontFeatureSettings } from '@inkoxide/style';
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
  ImageBackground: 'ink-image-background',
  Table: 'ink-table',
  Row: 'ink-row',
  Cell: 'ink-cell',
  PageNumber: 'ink-page-number',
  TotalPages: 'ink-total-pages',
  PageBreak: 'ink-page-break',
  Spacer: 'ink-spacer',
  Field: 'ink-field',
  FieldSet: 'ink-field-set',
  Note: 'ink-note',
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
  Marker: 'ink-svg-marker',
  Tspan: 'ink-svg-tspan',
  LinearGradient: 'ink-svg-linear-gradient',
  RadialGradient: 'ink-svg-radial-gradient',
  Stop: 'ink-svg-stop',
} as const;

export type HostType = (typeof HOST)[keyof typeof HOST];

/** Component name for a host type, for error messages and component stacks. */
export const HOST_NAME = Object.fromEntries(
  Object.entries(HOST).map(([name, type]) => [type, name]),
) as Record<HostType, string>;

/** A value for every side, or one per side (`{ top: 8, bottom: 4 }`). */
export type Edges<T> = T | { top?: T; right?: T; bottom?: T; left?: T };

/**
 * A length that needs no context: points as a number, or a string with a unit (`'20mm'`,
 * `'1.5cm'`, `'1in'`, `'12pt'`, `'16px'`, `'1pc'` = 12pt, `'1rem'` = 12pt as in Tailwind).
 */
export type AbsoluteLength = number | `${number}${'pt' | 'mm' | 'cm' | 'in' | 'px' | 'pc' | 'rem'}`;
/**
 * A length, or one relative to the font size (`'1.5em'`) or the page width or height (`'50vw'`,
 * `'10vh'`), or a CSS `calc()` of them (`'calc(10mm + 1em)'`).
 */
export type Length = AbsoluteLength | `${number}${'em' | 'vw' | 'vh'}` | `calc(${string})`;
/** A length or a percentage of the containing block. */
export type LengthPercent = Length | `${number}%`;
export type LengthAuto = LengthPercent | 'auto';

type BorderLineStyle = 'solid' | 'dashed' | 'dotted';

/** CSS `text-decoration-line` values. `lineThrough` is the Tailwind builder's spelling. */
type TextDecorationLine =
  | 'none'
  | 'underline'
  | 'line-through'
  | 'lineThrough'
  | 'underline line-through'
  | 'line-through underline';

/**
 * Inline styles: the same properties as Tailwind produces, applied after `className`. Colours
 * may be CSS strings (`'#1e4a99'`, `'oklch(…)'`) or RGBA float tuples; lengths are points or
 * strings with a unit (`'20mm'`).
 */
export interface StyleProp extends Omit<
  FlatStyle,
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
  | 'textDecoration'
  | 'decorationColor'
  | 'decorationThickness'
  | 'translate'
  | 'transform'
  | 'transformOrigin'
  | 'objectPosition'
  | 'fontFeatureSettings'
  | 'fontVariantNumeric'
  | 'textWrap'
  | 'verticalAlign'
> {
  fontFamily?: string | string[];
  /** 1–1000, or a CSS name (`'bold'`, `'light'`, `'semibold'`). */
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
  /**
   * CSS border shorthands: width, style and colour in any order (`'1px solid #e5e7eb'`,
   * `'2pt dashed red'`, `'none'`). A missing style is solid, a missing colour the text colour.
   * The keys below win over them, and a side (`borderTop`) wins over `border`.
   */
  border?: string | number;
  borderTop?: string | number;
  borderRight?: string | number;
  borderBottom?: string | number;
  borderLeft?: string | number;
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
  /** Both lines (`'underline line-through'`) draw a line-through: a run has one decoration line. */
  textDecoration?: TextDecorationLine;
  decorationColor?: string | Color;
  decorationThickness?: Length;
  /** CSS names for `decorationColor`, `decorationStyle` and `decorationThickness`. */
  textDecorationColor?: string | Color;
  textDecorationStyle?: FlatStyle['decorationStyle'];
  textDecorationThickness?: Length;
  /** `'ellipsis'` is `ellipsis: true` (with `maxLines`); `'clip'` is `ellipsis: false`. */
  textOverflow?: 'ellipsis' | 'clip';
  /**
   * OpenType features: a CSS string (`"'tnum' 1, 'smcp'"`), a list of tags to turn on
   * (`['tnum', 'smcp']`) or tags with a value (`{ tnum: 1, liga: false }`). Inherited, and a
   * `Text` that sets its own replaces them.
   */
  fontFeatureSettings?: FontFeatureSettings;
  /** CSS `font-variant-numeric` (`'tabular-nums slashed-zero'`): turns on the features it names. */
  fontVariantNumeric?: string;
  /**
   * How lines break: `'wrap'` (the default) fills each line in turn; `'pretty'` chooses the
   * breaks over the whole paragraph for even lines. Inherited. `'balance'` is not supported yet.
   */
  textWrap?: FlatStyle['textWrap'] | 'balance';
  translate?: [LengthPercent, LengthPercent];
  /** Transform functions (`[{ rotate: 10 }, { scale: [1.2, 1.2] }]`) or a CSS transform string. */
  transform?: TransformOp[] | string;
  /** `[x, y]` or a CSS value (`'top left'`, `'50% 0'`). */
  transformOrigin?: [LengthPercent, LengthPercent] | string;
  /**
   * `Image` and `ImageBackground` only: where a `contain`, `cover` or `none` image sits in its
   * box, as CSS `object-position` (`'left top'`, `'right'`, `'25% 75%'`, `'10pt 20pt'`). Default
   * centred.
   */
  objectPosition?: string;
  /**
   * A `Cell`'s content (`top`, `middle`, `bottom`), a run of a `Text` on its line (`super`,
   * `sub`, `baseline`), or an `Image` in a `Text` (also `text-top`, `text-bottom` and a number of
   * points its bottom sits above the baseline).
   */
  verticalAlign?: FlatStyle['verticalAlign'] | 'text-top' | 'text-bottom';
}

/** One style object, or a list of them merged left to right (falsy entries are skipped). */
export type Style = StyleProp | ReadonlyArray<StyleProp | false | null | undefined>;

/** Named styles, typed once (`StyleSheet.create({ title: { fontSize: 18 } })`). */
export const StyleSheet = {
  create<T extends Record<string, StyleProp>>(styles: T): T {
    return styles;
  },
};

/** A CSS font weight name, for `fontWeight` in place of its number (`'bold'` is 700). */
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

/**
 * `break="avoid"` keeps the element on one page; `"page-before"` / `"page-after"` start a new page
 * when content follows. They are errors inside a `flex-row`, grid, wrapping or reversed `View` and
 * inside an absolutely positioned, rotated or transformed element. On the element itself only
 * absolute positioning makes them an error: on a rotated or transformed element they work.
 */
export type BreakProp = 'avoid' | 'page-before' | 'page-after' | Break;

export type BookmarkProp = string | { title: string; level?: number };

/**
 * Splits a word into the parts a line may break between: `'hyphenation'` →
 * `['hy', 'phen', 'ation']`. `[word]` keeps the word whole. The word is as displayed, after
 * `textTransform` (`uppercase` gives `'HYPHENATION'`).
 */
export type HyphenationCallback = (word: string) => string[];

interface Common {
  className?: string;
  style?: Style;
  /** Node id: the target of `href="#id"` links. */
  id?: string;
  /**
   * Draws the element's boxes over the page: margin (orange), border (yellow), padding (green),
   * content box (blue outline) and its size in points. A development aid; it changes the output.
   * Not on a `<Text>` or `<Link>` inside a `<Text>`: a run of the paragraph has no box of its own,
   * so put it on the outer `<Text>`.
   */
  debug?: boolean;
  children?: ReactNode;
}

interface Flow extends Common {
  /** `true` is the same as `'page-before'`. */
  break?: BreakProp | boolean;
  /** `wrap={false}` is the same as `break="avoid"`. */
  wrap?: boolean;
  /** The same as `break={{ minPresenceAhead }}`, in points. */
  minPresenceAhead?: number;
  /**
   * Repeats the element on every page of its `Page`: a watermark, a page border, a stamp. Only
   * for an element placed directly in the `Page`, with `position: 'absolute'`: it is placed
   * against the page content box (inside the margins) and takes no space. `render` and `pages`
   * work inside it; `break`, `wrap` and `minPresenceAhead` do not, on it or inside it. For content
   * that needs room on every page, use `<Header>` or `<Footer>`.
   */
  fixed?: boolean;
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

/**
 * What a reader who opens an encrypted PDF without the owner password may do. Every permission
 * is off unless set, except `contentAccessibility`, which is on. Readers enforce them.
 */
export interface Permissions {
  printing?: 'lowResolution' | 'highResolution';
  modifying?: boolean;
  copying?: boolean;
  annotating?: boolean;
  fillingForms?: boolean;
  contentAccessibility?: boolean;
  documentAssembly?: boolean;
}

/** A PDF/A part and conformance level; a bare part (`'PDF/A-2'`) is level b. */
export type Conformance = 'PDF/A-1' | 'PDF/A-1b' | 'PDF/A-2' | 'PDF/A-2b' | 'PDF/A-3' | 'PDF/A-3b';

export interface DocumentProps {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string;
  creator?: string;
  /** The PDF producer. Default: `Inkoxide <version>`. */
  producer?: string;
  /** BCP-47 language of the document (`"en-US"`); also selects hyphenation patterns. */
  lang?: string;
  /** RFC 3339 timestamp written to the PDF. Omit it for deterministic output. */
  creationDate?: string;
  /**
   * What the viewer shows when it opens the PDF: no panel, the bookmarks, page thumbnails or
   * the pages full screen. Default: `'useOutlines'` when there are bookmarks, else `'useNone'`.
   */
  pageMode?: Meta['pageMode'];
  /**
   * How the viewer lays out the pages: one at a time, one scrolling column, two columns or two
   * pages side by side (odd pages on the left or the right). Default: the viewer's setting.
   */
  pageLayout?: Meta['pageLayout'];
  /**
   * Font ids tried for characters missing from a run's font, at most 256; an id listed again is
   * skipped. Default: `['NotoSans-Regular']`.
   */
  fallbackFonts?: string[];
  /** Outlines the boxes of every element on every page (`debug` on each `<Page>`). */
  debug?: boolean;
  /**
   * Write PDF/A, for archiving: `'PDF/A-2b'` (or `'PDF/A-2'`), `'PDF/A-3b'` or `'PDF/A-1b'`.
   * PDF/A-1b has no transparency: a render fails with `PDFA_CONFLICT` at the first colour with
   * alpha, `opacity`, image with an alpha channel or `debug` outline. The render option `pdfA`
   * wins over this prop. Default: plain PDF.
   */
  conformance?: Conformance;
  /**
   * Encrypts the PDF (AES-256): the password that opens it. Printable ASCII, at most 127 bytes.
   * Not part of the document tree, and never in the render result. The render option
   * `encryption` replaces these three props.
   */
  userPassword?: string;
  /** The password that lifts `permissions`. Default: the user password. */
  ownerPassword?: string;
  /** What a reader without the owner password may do; needs `ownerPassword`. */
  permissions?: Permissions;
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
  | AbsoluteLength
  | [width: AbsoluteLength, height: AbsoluteLength]
  | [side: AbsoluteLength]
  | {
      width: AbsoluteLength;
      height: AbsoluteLength;
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
  /** Outlines the page's margins and the boxes of every element on it, header and footer included. */
  debug?: boolean;
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
  /**
   * Hyphenate this text (same as `hyphens-auto`). Needs patterns for its language. `false`
   * (`hyphens-none`) also turns off the registered `hyphenationCallback`.
   */
  hyphenate?: boolean;
  /** Language of this text, when it differs from the document's. */
  lang?: string;
  /**
   * Breaks words where this function says, instead of the built-in patterns (`hyphenate`), which
   * then leave this text alone. Overrides the registered callback
   * (`Font.registerHyphenationCallback`); `null` turns it off for this text, which goes back to
   * `hyphenate`.
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

/** A `Link` needs a target: `href`, or `src`, which is the same. */
export type LinkProps = Flow &
  (
    | { href: string; src?: string }
    | {
        /** The same as `href`. */
        src: string;
      }
  );

/**
 * A request for a remote image: `<Image src={{ uri, method, headers, body }} />`. The `uri` must
 * be allowed by the remote image policy, like a URL string.
 */
export interface ImageRequest {
  uri: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string | Uint8Array;
}

/**
 * What a function or promise `src` resolves to: bytes (a `Uint8Array`, `Buffer` or
 * `ArrayBuffer`), any string `src`, or a request.
 */
export type ImageSrcValue = Uint8Array | ArrayBuffer | string | ImageRequest;

/**
 * An image `src`: bytes, a string (an image id, a `data:` URL, a file path or `file:` URL, an
 * allowed remote URL), a request, or a function or promise resolving to one of those.
 */
export type ImageSrc =
  | ImageSrcValue
  | Promise<ImageSrcValue>
  | (() => ImageSrcValue | Promise<ImageSrcValue>);

export interface ImageProps extends Omit<Flow, 'children'> {
  /**
   * Bytes (JPEG, PNG, GIF, WebP, SVG; a `Uint8Array`, `Buffer` or `ArrayBuffer`), a `data:` URL,
   * the id of an image passed in the render options, a file path (`./logo.png`,
   * `/srv/img/logo.png`) or `file:` URL in a folder allowed by the render option `localImages`, an
   * `https:` URL or a request allowed by the render option `remoteImages`, or a function or
   * promise resolving to one of those. A string naming an image in the render options is that
   * image, path or not.
   */
  src: ImageSrc;
  fit?: 'fill' | 'contain' | 'cover' | 'none';
  /**
   * Inside a `<Text>`: the text the image copies and searches as (its alternative text, at most
   * 1024 characters).
   */
  alt?: string;
}

export interface ImageBackgroundProps extends Flow {
  /** The image, in any form `<Image src>` takes. */
  src: ImageSrc;
  /**
   * How the image fills the padding box: `fill` (default) stretches it, `contain` and `cover`
   * keep its aspect ratio, `none` keeps its size. Wins over `objectFit`.
   */
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
  /** Outlines the row's box. */
  debug?: boolean;
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
  /** Outlines the spacer's box. */
  debug?: boolean;
}

export interface PageBreakProps {
  id?: string;
}

interface FieldCommon extends Omit<Flow, 'children' | 'href' | 'bookmark' | 'fixed'> {
  /**
   * Field name, unique in the document (radio buttons of one group share it). A `<FieldSet>`
   * puts its name in front; dots make groups the same way (`address.street`).
   */
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
  /** How the value shows and is typed: a number, a date, a phone number… */
  format?: TextFormat;
}

/**
 * The format of a single-line `TextField`. `value` must fit it: a number or percent as digits
 * (`'-1234.5'`, `'0.125'` for 12.50 %), a date as `'2026-10-14'` (a time may follow,
 * `T18:30`), a time as `'18:30'` or a date with a time, and a zip code, phone or social security
 * number as its digits.
 *
 * The second key of each pair is its other name. A number or percent format with any of
 * `nDec`, `sepComma`, `negStyle`, `currencyPrepend` and `param` defaults to no decimals and no
 * thousands separator (`nDec: 0`, `sepComma: false`); one without them, to 2 decimals and
 * `'1,234.56'`.
 */
export interface TextFormat {
  /** What the value is; `'zip4'` is another name for `'zipPlus4'`. */
  type: 'number' | 'percent' | 'date' | 'time' | 'zip' | 'zipPlus4' | 'zip4' | 'phone' | 'ssn';
  /** Number and percent: digits after the decimal separator; default 2. */
  decimals?: number;
  nDec?: number;
  /** Number and percent: the separators, by example; default `'1,234.56'`. */
  separators?: '1,234.56' | '1234.56' | '1.234,56' | '1234,56';
  /** `true` is `'1,234.56'`, `false` `'1234.56'`. */
  sepComma?: boolean;
  /** Number: how negative numbers show; default `'minus'`. */
  negative?: 'minus' | 'red' | 'parens' | 'parensRed';
  negStyle?: 'MinusBlack' | 'Red' | 'ParensBlack' | 'ParensRed';
  /** Number: a currency symbol, as it shows (`'$'`, `' €'`). */
  currency?: string;
  /** Number: the currency goes before the number; default `true`. */
  currencyBefore?: boolean;
  currencyPrepend?: boolean;
  /** Date and time: how the value shows, as `'dd/mm/yyyy'` or `'HH:MM'`. */
  pattern?: string;
  /**
   * Date: the pattern. Time: one of four styles, `0` `'HH:MM'`, `1` `'h:MM tt'`, `2`
   * `'HH:MM:ss'` and `3` `'h:MM:ss tt'`.
   */
  param?: string | number;
}

export interface CheckboxProps extends FieldCommon {
  checked?: boolean;
}

export interface RadioProps extends FieldCommon {
  /** The value the group exports when this button is selected. */
  value: string;
  checked?: boolean;
}

/** The choices of a `Select` or a `List`: `options`, or its other name `select`. */
type Choices = { options: string[]; select?: never } | { select: string[]; options?: never };

export type SelectProps = FieldCommon &
  Choices & {
    /** The selected option. */
    value?: string;
  };

export type ListProps = FieldCommon &
  Choices &
  (
    | {
        multiSelect?: false;
        /** The selected option. */
        value?: string;
      }
    | {
        /** Several options can be selected at once. */
        multiSelect: true;
        /** The selected options, or one. */
        value?: string | string[];
      }
  );

export interface NoteProps extends Omit<Flow, 'href' | 'bookmark'> {
  /** Who wrote the note; viewers show it as its title. */
  author?: string;
  /** Shows the note's text when the document opens. */
  open?: boolean;
  /** The note's text: strings and numbers only. */
  children?: ReactNode;
}

export interface FieldSetProps extends ViewProps {
  /** The group's name, put in front of the name of every field inside it: `address.street`. */
  name: string;
}

/** Any field; `kind` picks the type. The font, size and colour come from the text utilities. */
export type FieldProps =
  | ({ kind: 'text' } & TextFieldProps)
  | ({ kind: 'checkbox' } & CheckboxProps)
  | ({ kind: 'radio' } & RadioProps)
  | ({ kind: 'select' } & SelectProps)
  | ({ kind: 'list' } & ListProps);

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
  /** A `<Marker>` drawn at the first vertex of a path, line or poly shape: `url(#id)`. */
  markerStart?: string;
  /** A `<Marker>` drawn at every vertex between the first and the last: `url(#id)`. */
  markerMid?: string;
  /** A `<Marker>` drawn at the last vertex: `url(#id)`. */
  markerEnd?: string;
  visibility?: 'visible' | 'hidden' | 'collapse';
  /** Text inside: the font, as in CSS. A family list (`"Inter, sans-serif"`) is a stack. */
  fontFamily?: string;
  fontSize?: number | string;
  fontWeight?: number | FontWeightName;
  fontStyle?: 'normal' | 'italic';
  /** Text inside: where its position is on the text (`start`, `middle`, `end`). */
  textAnchor?: 'start' | 'middle' | 'end';
  /** Text inside: which line of the text sits at its `y`. */
  dominantBaseline?:
    | 'auto'
    | 'alphabetic'
    | 'middle'
    | 'central'
    | 'hanging'
    | 'mathematical'
    | 'ideographic'
    | 'text-before-edge'
    | 'text-after-edge';
  id?: string;
  children?: ReactNode;
}

/** Numbers for each character in turn: a number, a list, or SVG list syntax. */
type SvgNumberList = number | string | readonly number[];

/** A `Text` inside an `<Svg>`: SVG text, positioned in the drawing's coordinates. */
export interface SvgTextProps extends SvgPresentation {
  /** Where the text starts (with `textAnchor`); a list positions each character. */
  x?: SvgNumberList;
  y?: SvgNumberList;
  /** Shifts from where each character would be. */
  dx?: SvgNumberList;
  dy?: SvgNumberList;
  /** Degrees each character is turned; the last value repeats. */
  rotate?: SvgNumberList;
  letterSpacing?: number | string;
  wordSpacing?: number | string;
  /** `underline`, `overline`, `line-through`, or several. */
  textDecoration?: string;
  /** The width to fit the text to, by its spacing or (`lengthAdjust`) its glyphs too. */
  textLength?: number | string;
  lengthAdjust?: 'spacing' | 'spacingAndGlyphs';
  direction?: 'ltr' | 'rtl';
}

/** A span of SVG text: a `Tspan` inside a `Text` in an `<Svg>`. */
export interface TspanProps extends SvgTextProps {
  /** Raises (positive, `super`) or lowers (`sub`) the span from the baseline. */
  baselineShift?: number | string;
  alignmentBaseline?: SvgPresentation['dominantBaseline'];
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
  /** Repeats the drawing on every page, like `fixed` on a `View`. */
  fixed?: boolean;
  /** Outlines the drawing's box. */
  debug?: boolean;
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
export interface DefsProps {
  children?: ReactNode;
}
export interface ClipPathProps {
  /** Referenced as `clipPath="url(#id)"`. */
  id: string;
  /** What the contents' coordinates are in: the user space (default) or the clipped shape's bounding box. */
  clipPathUnits?: 'userSpaceOnUse' | 'objectBoundingBox';
  children?: ReactNode;
}
export interface MarkerProps extends SvgPresentation {
  /** Referenced as `markerStart="url(#id)"` (or `markerMid`, `markerEnd`). */
  id: string;
  viewBox?: string;
  preserveAspectRatio?: string;
  /** The point of the marker's contents placed on the vertex, in `viewBox` units. */
  refX?: number | string;
  refY?: number | string;
  /** The size of the marker's viewport; 3 × 3 by default. */
  markerWidth?: number | string;
  markerHeight?: number | string;
  /**
   * Turns the marker with the path's direction at the vertex (`'auto'`), the same but reversed at
   * the start (`'auto-start-reverse'`, for arrows at both ends), or by a fixed angle in degrees.
   */
  orient?: 'auto' | 'auto-start-reverse' | number | string;
  /** `strokeWidth` (default) scales the marker by the shape's stroke width; `userSpaceOnUse` does not. */
  markerUnits?: 'strokeWidth' | 'userSpaceOnUse';
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
  /** Radius of the focal circle of a radial gradient. */
  fr?: number | string;
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
/** A paragraph, or a run inside one; inside an `<Svg>`, SVG text (`SvgTextProps`). */
export const Text = host<TextProps | SvgTextProps>(HOST.Text);
/**
 * A link to `href` (`https://…`, `mailto:…` or `#id`; `src` is the same). Inside a `Text` it is
 * an inline run; elsewhere a box, like a `View` with `href`. It adds no colour or underline.
 */
export const Link = host<LinkProps>(HOST.Link);
export const Image = host<ImageProps>(HOST.Image);
/**
 * A `View` with an image behind its children: the image fills the padding box (inside the
 * borders), placed by `fit` and `objectPosition`, over the background colour and under the
 * borders, clipped to the box and its `borderRadius`.
 */
export const ImageBackground = host<ImageBackgroundProps>(HOST.ImageBackground);
export const Table = host<TableProps>(HOST.Table);
export const Row = host<RowProps>(HOST.Row);
export const Cell = host<CellProps>(HOST.Cell);
/** The current page number. Only inside `Text`. */
export const PageNumber = host<Record<string, never>>(HOST.PageNumber);
/** The total page count. Only inside `Text`. */
export const TotalPages = host<Record<string, never>>(HOST.TotalPages);
/**
 * Starts a new page when content follows it, at any depth of the body. An error inside a
 * `flex-row`, grid, wrapping or reversed `View` and inside an absolutely positioned, rotated or
 * transformed element.
 */
export const PageBreak = host<PageBreakProps>(HOST.PageBreak);
export const Spacer = host<SpacerProps>(HOST.Spacer);
/** An interactive form field (AcroForm). */
export const Field = host<FieldProps>(HOST.Field);
/**
 * A group of form fields: a `View` whose `name` goes in front of the name of every field inside
 * it, at any depth (`<FieldSet name="address"><TextField name="street" /></FieldSet>` is the
 * field `address.street`). Sets nest: `a.b.c`.
 */
export const FieldSet = host<FieldSetProps>(HOST.FieldSet);
/**
 * A text note: a sticky-note icon that viewers open to show its text (the children, which are
 * strings). It takes no space; the icon sits at the top-left corner of its box. Its colour is its
 * `backgroundColor` (default yellow).
 */
export const Note = host<NoteProps>(HOST.Note);
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
export const Defs = host<DefsProps>(HOST.Defs);
export const ClipPath = host<ClipPathProps>(HOST.ClipPath);
/**
 * A shape drawn at the vertices of the paths, lines and poly shapes that reference it through
 * `markerStart`, `markerMid` or `markerEnd`. Defined once, in `Defs`.
 */
export const Marker = host<MarkerProps>(HOST.Marker);
/** A span of SVG text, inside a `Text` in an `<Svg>`: its own position, font or paint. */
export const Tspan = host<TspanProps>(HOST.Tspan);
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
/** The same as `TextField`. */
export const TextInput: FC<TextFieldProps> = field('text', 'TextInput');
/** A checkbox. */
export const Checkbox: FC<CheckboxProps> = field('checkbox', 'Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio: FC<RadioProps> = field('radio', 'Radio');
/** A drop-down list. */
export const Select: FC<SelectProps> = field('select', 'Select');
/** A list box: several options in view (four without a height), one or several selected. */
export const List: FC<ListProps> = field('list', 'List');
