import { FIELD_KINDS, HOST, type PageInfo, type PropsByComponent } from '@inkoxide/tree';
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
 * The component of that name, which renders its host element (a field component, such as
 * `TextField`, a `Field` of its kind): its attributes go to the element as props (kebab-case names
 * are camelized, `class` is `className`), its default slot is its content, and a `render` slot
 * becomes the `render` prop, called for each page.
 */
function host<N extends keyof PropsByComponent>(name: N): InkComponent<PropsByComponent[N]> {
  const kind = FIELD_KINDS[name as keyof typeof FIELD_KINDS];
  const type = kind ? HOST.Field : HOST[name as keyof typeof HOST];
  const fixed = kind && { kind };
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
  }) as unknown as InkComponent<PropsByComponent[N]>;
}

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
/** A `View` with an image behind its children, in its padding box. */
export const ImageBackground = host('ImageBackground');
export const Table = host('Table');
export const Row = host('Row');
export const Cell = host('Cell');
/** The current page number. Only inside `Text`. */
export const PageNumber = host('PageNumber');
/** The total page count. Only inside `Text`. */
export const TotalPages = host('TotalPages');
/** Starts a new page when content follows it. */
export const PageBreak = host('PageBreak');
export const Spacer = host('Spacer');
/** An interactive form field (AcroForm). */
export const Field = host('Field');
/** A group of form fields, whose `name` goes in front of the name of every field inside it. */
export const FieldSet = host('FieldSet');
/** A text note: a sticky-note icon that viewers open to show its text. */
export const Note = host('Note');
/** An icon that opens a file of `<Document :attachments>`. */
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
/** A shape drawn at the vertices of paths that reference it, defined once in `Defs`. */
export const Marker = host('Marker');
/** A span of SVG text, inside a `Text` in an `<Svg>`. */
export const Tspan = host('Tspan');
/** Text along a path or shape (`href="#curve"`), directly inside a `Text` in an `<Svg>`. */
export const TextPath = host('TextPath');
export const LinearGradient = host('LinearGradient');
export const RadialGradient = host('RadialGradient');
export const Stop = host('Stop');
/** Imperative drawing with a painter (`:paint="(p, w, h) => p.circle(w / 2, h / 2, 20).fill('red')"`). */
export const Canvas = host('Canvas');

/** A text field. */
export const TextField = host('TextField');
/** The same as `TextField`. */
export const TextInput = host('TextInput');
/** A checkbox. */
export const Checkbox = host('Checkbox');
/** One button of a radio group (buttons share `name`). */
export const Radio = host('Radio');
/** A drop-down list. */
export const Select = host('Select');
/** A list box: several options in view, one or several selected. */
export const List = host('List');
