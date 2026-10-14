import { createRequire } from 'node:module';

import { createContext } from 'react';
import { DefaultEventPriority, NoEventPriority } from 'react-reconciler/constants.js';

import { HOST, type HostType } from '../components/index.ts';
import { TemplateError } from '../errors.ts';
import { SVG_TAGS } from '../tree/svg.ts';
import {
  type Container,
  hostName,
  type Instance,
  isHostType,
  type Node,
  type TextInstance,
} from './instance.ts';

interface HostContext {
  /**
   * Host type of the nearest host ancestor (`'root'` at the top). React asks for the context of an
   * element's children before it creates the element, so this can be an unknown type (`div`):
   * its children fail first, against it.
   */
  parent: HostType | 'root' | (string & {});
}

/** This package's version: its package.json is two levels up from `src/reconciler` and `dist/reconciler`. */
const { version } = createRequire(import.meta.url)('../../package.json') as { version: string };

const ROOT_CONTEXT: HostContext = { parent: 'root' };
const contexts = new Map<string, HostContext>();
const contextFor = (parent: string): HostContext => {
  let c = contexts.get(parent);
  if (!c) {
    c = { parent };
    contexts.set(parent, c);
  }
  return c;
};

const FLOW = new Set<string>([
  HOST.View,
  HOST.Text,
  HOST.Link,
  HOST.Image,
  HOST.ImageBackground,
  HOST.Table,
  HOST.PageBreak,
  HOST.Spacer,
  HOST.Field,
  HOST.FieldSet,
  HOST.Note,
  HOST.Svg,
  HOST.Canvas,
]);
const INLINE = new Set<string>([HOST.Text, HOST.Link, HOST.Image, HOST.PageNumber, HOST.TotalPages]);
const SVG_SHAPES = new Set<string>([
  HOST.Path,
  HOST.Rect,
  HOST.Circle,
  HOST.Ellipse,
  HOST.Line,
  HOST.Polyline,
  HOST.Polygon,
]);
/**
 * What goes in an `<Svg>`, `<G>`, `<Defs>`, `<ClipPath>` or `<Marker>`: every SVG primitive but
 * `<Stop>`.
 */
const SVG_CONTENT = new Set<string>(Object.keys(SVG_TAGS).filter((t) => t !== HOST.Stop));
const LEAF = new Set<string>([
  HOST.Image,
  HOST.PageBreak,
  HOST.Spacer,
  HOST.PageNumber,
  HOST.TotalPages,
  HOST.Field,
  HOST.Canvas,
  HOST.Stop,
  ...SVG_SHAPES,
]);

/** Which host types may appear directly inside which. Strings: see TEXT_PARENTS. */
function allowed(parent: HostContext['parent'], child: HostType): boolean {
  switch (parent) {
    case 'root':
      return child === HOST.Document;
    case HOST.Document:
      return child === HOST.Page;
    case HOST.Page:
      return FLOW.has(child) || child === HOST.Header || child === HOST.Footer;
    case HOST.Text:
      return INLINE.has(child);
    case HOST.Table:
      return child === HOST.Row;
    case HOST.Row:
      return child === HOST.Cell;
    case HOST.Header:
    case HOST.Footer:
    case HOST.View:
    case HOST.ImageBackground:
    case HOST.Link:
    case HOST.FieldSet:
    case HOST.Cell:
      return FLOW.has(child);
    case HOST.Svg:
    case HOST.G:
    case HOST.Defs:
    case HOST.ClipPath:
    case HOST.Marker:
      return SVG_CONTENT.has(child);
    case HOST.LinearGradient:
    case HOST.RadialGradient:
      return child === HOST.Stop;
    default:
      return false;
  }
}

/** Containers where bare strings are allowed (they become an anonymous text node). */
const TEXT_PARENTS = new Set<string>([
  HOST.Text,
  HOST.View,
  HOST.ImageBackground,
  HOST.Link,
  HOST.FieldSet,
  HOST.Note,
  HOST.Cell,
  HOST.Header,
  HOST.Footer,
  HOST.Page,
]);

/** Where a string placed in `parent`, which takes no strings, belongs instead. */
function textAdvice(parent: HostContext['parent']): string {
  switch (parent) {
    case 'root':
      return 'render a <Document> with a <Page>, and put it in the page';
    case HOST.Document:
      return 'put it in a <Page>';
    case HOST.Table:
      return 'put it in a <Cell> of a <Row>';
    case HOST.Row:
      return 'put it in a <Cell>';
    default:
      return 'wrap it in <Text>';
  }
}

const name = (type: string): string => (type === 'root' ? 'the root' : `<${hostName(type)}>`);

function placementError(parent: HostContext['parent'], child: string): TemplateError {
  if (!isHostType(child)) {
    return new TemplateError(
      `<${child}> is not an Inkoxide component. Templates use only @inkoxide/react components (View, Text, Table, …), not HTML elements.`,
    );
  }
  if (LEAF.has(parent)) return new TemplateError(`${name(parent)} cannot have children.`);
  if (parent === HOST.Note) {
    return new TemplateError(`A <Note> contains only text: ${name(child)} cannot be inside it.`);
  }
  if (parent === HOST.Text) {
    return new TemplateError(
      `${name(child)} cannot be inside <Text>. A Text contains only strings, numbers, <Text>, <Link>, <Image>, <PageNumber> and <TotalPages>.`,
    );
  }
  if (child === HOST.PageNumber || child === HOST.TotalPages) {
    return new TemplateError(`${name(child)} must be inside a <Text>.`);
  }
  if (Object.hasOwn(SVG_TAGS, child)) {
    return new TemplateError(`${name(child)} is an SVG primitive: it goes inside <Svg>.`);
  }
  if (parent === HOST.Svg || SVG_CONTENT.has(parent)) {
    return new TemplateError(`${name(child)} cannot be inside ${name(parent)}; only SVG primitives can.`);
  }
  if (child === HOST.Header || child === HOST.Footer) {
    return new TemplateError(`${name(child)} must be a direct child of <Page>.`);
  }
  if (parent === 'root') return new TemplateError('A template must render one <Document> at the top.');
  return new TemplateError(`${name(child)} cannot be inside ${name(parent)}.`);
}

let currentPriority: number = NoEventPriority;

const append = (parent: Instance | Container, child: Node): void => {
  parent.children.push(child);
};
const insert = (parent: Instance | Container, child: Node, before: Node): void => {
  const at = parent.children.indexOf(child);
  if (at >= 0) parent.children.splice(at, 1);
  const i = parent.children.indexOf(before);
  parent.children.splice(i < 0 ? parent.children.length : i, 0, child);
};
const remove = (parent: Instance | Container, child: Node): void => {
  const i = parent.children.indexOf(child);
  if (i >= 0) parent.children.splice(i, 1);
};

/**
 * The host config: a mutation-mode renderer whose instances are plain objects. Templates render
 * once and synchronously, so the update paths exist only for completeness.
 */
export const hostConfig = {
  rendererPackageName: '@inkoxide/react',
  rendererVersion: version,
  extraDevToolsConfig: null,
  isPrimaryRenderer: false,
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  supportsMicrotasks: true,
  scheduleMicrotask: queueMicrotask,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,
  NotPendingTransition: null,
  HostTransitionContext: createContext(null),

  // A nested root (a `render` prop's output) starts inside its owner's host type.
  getRootHostContext: (root: Container): HostContext =>
    root.parent ? contextFor(root.parent) : ROOT_CONTEXT,
  // A Link inside a Text is an inline run: its content follows the rules of Text.
  getChildHostContext: (parent: HostContext, type: string): HostContext =>
    contextFor(type === HOST.Link && parent.parent === HOST.Text ? HOST.Text : type),
  getPublicInstance: (instance: Instance): Instance => instance,
  prepareForCommit: (): null => null,
  resetAfterCommit: (): void => {},
  preparePortalMount: (): void => {},

  createInstance(
    type: string,
    props: Record<string, unknown>,
    _root: Container,
    ctx: HostContext,
    fiber: unknown,
  ) {
    if (!isHostType(type) || !allowed(ctx.parent, type)) throw placementError(ctx.parent, type);
    const instance: Instance = { kind: 'element', type, props, children: [], fiber };
    return instance;
  },
  createTextInstance(text: string, _root: Container, ctx: HostContext, fiber: unknown): TextInstance {
    if (ctx.parent === HOST.Svg || SVG_CONTENT.has(ctx.parent)) {
      throw new TemplateError('Text inside <Svg> is not supported; place a <Text> over the drawing instead.');
    }
    if (!TEXT_PARENTS.has(ctx.parent)) {
      const shown = `Text "${text.length > 40 ? `${text.slice(0, 40)}…` : text}"`;
      if (ctx.parent !== 'root' && !isHostType(ctx.parent)) throw placementError('root', ctx.parent);
      if (LEAF.has(ctx.parent))
        throw new TemplateError(`${shown}: ${name(ctx.parent)} cannot have children.`);
      throw new TemplateError(
        `${shown} cannot be directly inside ${name(ctx.parent)}; ${textAdvice(ctx.parent)}.`,
      );
    }
    return { kind: 'text', text, fiber };
  },
  appendInitialChild: append,
  appendChild: append,
  appendChildToContainer: append,
  insertBefore: insert,
  insertInContainerBefore: insert,
  removeChild: remove,
  removeChildFromContainer: remove,
  clearContainer(container: Container): void {
    container.children.length = 0;
  },
  finalizeInitialChildren: (): boolean => false,
  shouldSetTextContent: (): boolean => false,
  commitMount: (): void => {},
  commitUpdate(instance: Instance, _type: string, _old: unknown, next: Record<string, unknown>): void {
    instance.props = next;
  },
  commitTextUpdate(instance: TextInstance, _old: string, next: string): void {
    instance.text = next;
  },
  resetTextContent: (): void => {},
  hideInstance: (): void => {},
  unhideInstance: (): void => {},
  hideTextInstance: (): void => {},
  unhideTextInstance: (): void => {},
  detachDeletedInstance: (): void => {},

  setCurrentUpdatePriority(p: number): void {
    currentPriority = p;
  },
  getCurrentUpdatePriority: (): number => currentPriority,
  resolveUpdatePriority: (): number =>
    currentPriority !== NoEventPriority ? currentPriority : DefaultEventPriority,
  resolveEventType: (): null => null,
  resolveEventTimeStamp: (): number => -1.1,
  trackSchedulerEvent: (): void => {},
  shouldAttemptEagerTransition: (): boolean => false,
  requestPostPaintCallback: (): void => {},
  getInstanceFromNode: (): null => null,
  getInstanceFromScope: (): null => null,
  beforeActiveInstanceBlur: (): void => {},
  afterActiveInstanceBlur: (): void => {},
  prepareScopeUpdate: (): void => {},
  resetFormInstance: (): void => {},

  maySuspendCommit: (): boolean => false,
  maySuspendCommitOnUpdate: (): boolean => false,
  maySuspendCommitInSyncRender: (): boolean => false,
  preloadInstance: (): boolean => true,
  startSuspendingCommit: (): void => {},
  suspendInstance: (): void => {},
  waitForCommitToBeReady: (): null => null,
  getSuspendedCommitReason: (): null => null,
  // React calls it when it commits work it retries after a suspension; a template that suspends
  // fails before then, but the retry can still run.
  suspendOnActiveViewTransition: (): void => {},
};
