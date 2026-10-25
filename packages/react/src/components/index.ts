import {
  type CanvasProps,
  type CellProps,
  type CheckboxProps,
  type CircleProps,
  type ClipPathProps,
  type DefsProps,
  type DocumentProps,
  type EllipseProps,
  FIELD_KINDS,
  type FieldProps,
  type FieldSetProps,
  type FooterProps,
  type GradientProps,
  type HeaderProps,
  HOST,
  type HostType,
  type ImageBackgroundProps,
  type ImageProps,
  type LineProps,
  type LinkProps,
  type ListProps,
  type MarkerProps,
  type NoteProps,
  type AttachmentProps,
  type PageBreakProps,
  type PageInfo,
  type PageProps,
  type PathProps,
  type PolyProps,
  type RadioProps,
  type RectProps,
  type RowProps,
  type SelectProps,
  type SpacerProps,
  type StopProps,
  type SvgElementProps,
  type SvgProps,
  type SvgTextProps,
  type TableProps,
  type TextFieldProps,
  type TextPathProps,
  type TextProps,
  type TspanProps,
  type ViewProps,
} from '@inkoxide/tree';
import { createElement, type FC, type ReactNode } from 'react';

// The props and style types, shared by every front end.
export {
  type AbsoluteLength,
  type AttachmentFile,
  type AttachmentProps,
  type BookmarkProp,
  type BreakProp,
  type CanvasProps,
  type CellProps,
  type CheckboxProps,
  type CircleProps,
  type ClipPathProps,
  type Conformance,
  type DefsProps,
  type DocumentProps,
  type Edges,
  type EllipseProps,
  type FieldProps,
  type FieldSetProps,
  type FontStretch,
  type FontWeightName,
  type FooterProps,
  type GradientProps,
  type HeaderProps,
  HOST,
  HOST_NAME,
  type HostType,
  type HyphenationCallback,
  type ImageBackgroundProps,
  type ImageProps,
  type ImageRequest,
  type ImageSrc,
  type ImageSrcValue,
  type Length,
  type LengthAuto,
  type LengthPercent,
  type LineProps,
  type LinkProps,
  type ListProps,
  type MarkerProps,
  type MediaStyle,
  type NoteProps,
  type PageBreakProps,
  type PageInfo,
  type PagePreset,
  type PageProps,
  type PageSize,
  type PathProps,
  type Permissions,
  type PolyProps,
  type RadioProps,
  type RectProps,
  type Role,
  type RowProps,
  type SelectProps,
  type SpacerProps,
  type StopProps,
  type Style,
  type StyleProp,
  StyleSheet,
  type SvgElementProps,
  type SvgPresentation,
  type SvgProps,
  type SvgStyle,
  type SvgTextProps,
  type TableProps,
  type TextFieldProps,
  type TextFormat,
  type TextPathProps,
  type TextProps,
  type TspanProps,
  type ViewProps,
} from '@inkoxide/tree';

/**
 * Content computed per page: called once for every page the document has (after a first pass
 * that counts them), and painted only on that page. Pages with identical output share one node.
 */
export type RenderProp = (page: PageInfo) => ReactNode;

/** A component's props with React children and a React `render` prop. */
export type ReactProps<P> = P extends unknown
  ? Omit<P, 'children' | 'render'> &
      ('children' extends keyof P ? { children?: ReactNode } : unknown) &
      ('render' extends keyof P ? { render?: RenderProp } : unknown)
  : never;

/**
 * Components are host elements, not function components: one fiber per element and no wrapper
 * overhead, which matters for a 100-page report.
 */
const host = <P>(type: HostType): FC<ReactProps<P>> => type as unknown as FC<ReactProps<P>>;

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
/**
 * An icon that opens a file of `<Document attachments>`, as a `<Note>` shows its text: it takes
 * no room, and inside a `<Text>` it hangs from the line. Its colour is `backgroundColor`.
 */
export const Attachment = host<AttachmentProps>(HOST.Attachment);
/** A vector drawing; becomes an SVG image. Use the SVG primitives (`Path`, `Rect`, …) inside. */
export const Svg = host<SvgProps>(HOST.Svg);
export const G = host<SvgElementProps>(HOST.G);
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
/** Text along a path or shape (`href="#curve"`), directly inside a `Text` in an `<Svg>`. */
export const TextPath = host<TextPathProps>(HOST.TextPath);
export const LinearGradient = host<GradientProps>(HOST.LinearGradient);
export const RadialGradient = host<GradientProps>(HOST.RadialGradient);
export const Stop = host<StopProps>(HOST.Stop);
/** Imperative drawing with a painter (`paint={(p, w, h) => p.circle(w / 2, h / 2, 20).fill('red')}`). */
export const Canvas = host<CanvasProps>(HOST.Canvas);

function field<P extends object>(name: keyof typeof FIELD_KINDS): FC<P> {
  const kind = FIELD_KINDS[name];
  const f: FC<P> = (props) => createElement(HOST.Field, { ...props, kind });
  f.displayName = name;
  return f;
}
/** A text field. */
export const TextField: FC<TextFieldProps> = field('TextField');
/** The same as `TextField`. */
export const TextInput: FC<TextFieldProps> = field('TextInput');
/** A checkbox. */
export const Checkbox: FC<CheckboxProps> = field('Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio: FC<RadioProps> = field('Radio');
/** A drop-down list. */
export const Select: FC<SelectProps> = field('Select');
/** A list box: several options in view (four without a height), one or several selected. */
export const List: FC<ListProps> = field('List');
