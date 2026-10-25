import {
  type AttachmentProps,
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
  HOST_NAME,
  type HostType,
  type ImageBackgroundProps,
  type ImageProps,
  type LineProps,
  type LinkProps,
  type ListProps,
  type MarkerProps,
  type NoteProps,
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
import {
  defineComponent,
  type DefineSetupFnComponent,
  getCurrentInstance,
  h,
  type SlotsType,
  type VNode,
} from 'vue';

import { OWNER } from './renderer.ts';

/** A component's props in Vue: children come in the default slot, and `render` is a slot too. */
export type VueProps<P> = P extends unknown ? Omit<P, 'children' | 'render'> : never;

/** The slots of a component: its content, and `render` on the components that take it. */
export type VueSlots<P> = ('children' extends keyof P ? { default?: () => VNode[] } : {}) &
  ('render' extends keyof P ? { render?: (page: PageInfo) => VNode[] } : {});

/** A component of this package, typed for templates and render functions. */
export type InkComponent<P> = DefineSetupFnComponent<
  VueProps<P> & Record<string, unknown>,
  {},
  SlotsType<VueSlots<P>>
>;

/**
 * The component for a host type: its attributes go to the host element as props (kebab-case
 * names are camelized, `class` is `className`), its default slot is its content, and a `render`
 * slot becomes the `render` prop, called for each page.
 */
function host<P>(type: HostType, name = HOST_NAME[type], fixed?: Record<string, unknown>): InkComponent<P> {
  return defineComponent({
    name,
    inheritAttrs: false,
    setup(_, { attrs, slots }) {
      const owner = getCurrentInstance();
      return () => {
        const props: Record<string, unknown> = { ...attrs, ...fixed, [OWNER]: owner };
        const render = slots.render;
        if (render) props.render = (page: PageInfo) => render(page);
        return h(type, props, slots.default?.());
      };
    },
  }) as unknown as InkComponent<P>;
}

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
/** A `View` with an image behind its children, in its padding box. */
export const ImageBackground = host<ImageBackgroundProps>(HOST.ImageBackground);
export const Table = host<TableProps>(HOST.Table);
export const Row = host<RowProps>(HOST.Row);
export const Cell = host<CellProps>(HOST.Cell);
/** The current page number. Only inside `Text`. */
export const PageNumber = host<Record<string, never>>(HOST.PageNumber);
/** The total page count. Only inside `Text`. */
export const TotalPages = host<Record<string, never>>(HOST.TotalPages);
/** Starts a new page when content follows it. */
export const PageBreak = host<PageBreakProps>(HOST.PageBreak);
export const Spacer = host<SpacerProps>(HOST.Spacer);
/** An interactive form field (AcroForm). */
export const Field = host<FieldProps>(HOST.Field);
/** A group of form fields, whose `name` goes in front of the name of every field inside it. */
export const FieldSet = host<FieldSetProps>(HOST.FieldSet);
/** A text note: a sticky-note icon that viewers open to show its text. */
export const Note = host<NoteProps>(HOST.Note);
/** An icon that opens a file of `<Document :attachments>`. */
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
/** A shape drawn at the vertices of paths that reference it, defined once in `Defs`. */
export const Marker = host<MarkerProps>(HOST.Marker);
/** A span of SVG text, inside a `Text` in an `<Svg>`. */
export const Tspan = host<TspanProps>(HOST.Tspan);
/** Text along a path or shape (`href="#curve"`), directly inside a `Text` in an `<Svg>`. */
export const TextPath = host<TextPathProps>(HOST.TextPath);
export const LinearGradient = host<GradientProps>(HOST.LinearGradient);
export const RadialGradient = host<GradientProps>(HOST.RadialGradient);
export const Stop = host<StopProps>(HOST.Stop);
/** Imperative drawing with a painter (`:paint="(p, w, h) => p.circle(w / 2, h / 2, 20).fill('red')"`). */
export const Canvas = host<CanvasProps>(HOST.Canvas);

const field = <P>(name: keyof typeof FIELD_KINDS) => host<P>(HOST.Field, name, { kind: FIELD_KINDS[name] });
/** A text field. */
export const TextField = field<TextFieldProps>('TextField');
/** The same as `TextField`. */
export const TextInput = field<TextFieldProps>('TextInput');
/** A checkbox. */
export const Checkbox = field<CheckboxProps>('Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio = field<RadioProps>('Radio');
/** A drop-down list. */
export const Select = field<SelectProps>('Select');
/** A list box: several options in view, one or several selected. */
export const List = field<ListProps>('List');
