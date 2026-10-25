import { FIELD_KINDS, HOST, type PageInfo, type PropsByComponent } from '@inkoxide/tree';
import { createElement, type FC, type ReactNode } from 'react';

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
const host = <N extends keyof typeof HOST>(name: N) =>
  HOST[name] as unknown as FC<ReactProps<PropsByComponent[N]>>;

export const Document = host('Document');
export const Page = host('Page');
export const Header = host('Header');
export const Footer = host('Footer');
export const View = host('View');
/** A paragraph, or a run inside one; inside an `<Svg>`, SVG text (`SvgTextProps`). */
export const Text = host('Text');
/**
 * A link to `href` (`https://…`, `mailto:…` or `#id`; `src` is the same). Inside a `Text` it is
 * an inline run; elsewhere a box, like a `View` with `href`. It adds no colour or underline.
 */
export const Link = host('Link');
export const Image = host('Image');
/**
 * A `View` with an image behind its children: the image fills the padding box (inside the
 * borders), placed by `fit` and `objectPosition`, over the background colour and under the
 * borders, clipped to the box and its `borderRadius`.
 */
export const ImageBackground = host('ImageBackground');
export const Table = host('Table');
export const Row = host('Row');
export const Cell = host('Cell');
/** The current page number. Only inside `Text`. */
export const PageNumber = host('PageNumber');
/** The total page count. Only inside `Text`. */
export const TotalPages = host('TotalPages');
/**
 * Starts a new page when content follows it, at any depth of the body. An error inside a
 * `flex-row`, grid, wrapping or reversed `View` and inside an absolutely positioned, rotated or
 * transformed element.
 */
export const PageBreak = host('PageBreak');
export const Spacer = host('Spacer');
/** An interactive form field (AcroForm). */
export const Field = host('Field');
/**
 * A group of form fields: a `View` whose `name` goes in front of the name of every field inside
 * it, at any depth (`<FieldSet name="address"><TextField name="street" /></FieldSet>` is the
 * field `address.street`). Sets nest: `a.b.c`.
 */
export const FieldSet = host('FieldSet');
/**
 * A text note: a sticky-note icon that viewers open to show its text (the children, which are
 * strings). It takes no space; the icon sits at the top-left corner of its box. Its colour is its
 * `backgroundColor` (default yellow).
 */
export const Note = host('Note');
/**
 * An icon that opens a file of `<Document attachments>`, as a `<Note>` shows its text: it takes
 * no room, and inside a `<Text>` it hangs from the line. Its colour is `backgroundColor`.
 */
export const Attachment = host('Attachment');
/** A vector drawing; becomes an SVG image. Use the SVG primitives (`Path`, `Rect`, …) inside. */
export const Svg = host('Svg');
export const G = host('G');
export const Path = host('Path');
export const Rect = host('Rect');
export const Circle = host('Circle');
export const Ellipse = host('Ellipse');
export const Line = host('Line');
export const Polyline = host('Polyline');
export const Polygon = host('Polygon');
export const Defs = host('Defs');
export const ClipPath = host('ClipPath');
/**
 * A shape drawn at the vertices of the paths, lines and poly shapes that reference it through
 * `markerStart`, `markerMid` or `markerEnd`. Defined once, in `Defs`.
 */
export const Marker = host('Marker');
/** A span of SVG text, inside a `Text` in an `<Svg>`: its own position, font or paint. */
export const Tspan = host('Tspan');
/** Text along a path or shape (`href="#curve"`), directly inside a `Text` in an `<Svg>`. */
export const TextPath = host('TextPath');
export const LinearGradient = host('LinearGradient');
export const RadialGradient = host('RadialGradient');
export const Stop = host('Stop');
/** Imperative drawing with a painter (`paint={(p, w, h) => p.circle(w / 2, h / 2, 20).fill('red')}`). */
export const Canvas = host('Canvas');

function field<N extends keyof typeof FIELD_KINDS>(name: N): FC<PropsByComponent[N]> {
  const kind = FIELD_KINDS[name];
  const f: FC<PropsByComponent[N]> = (props) => createElement(HOST.Field, { ...(props as object), kind });
  f.displayName = name;
  return f;
}
/** A text field. */
export const TextField = field('TextField');
/** The same as `TextField`. */
export const TextInput = field('TextInput');
/** A checkbox. */
export const Checkbox = field('Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio = field('Radio');
/** A drop-down list. */
export const Select = field('Select');
/** A list box: several options in view (four without a height), one or several selected. */
export const List = field('List');
