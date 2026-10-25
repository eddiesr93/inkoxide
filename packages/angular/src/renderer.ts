import {
  createComponent,
  createEnvironmentInjector,
  DOCUMENT,
  type EnvironmentInjector,
  InjectionToken,
  Injector,
  type Renderer2,
  RendererFactory2,
  type RendererStyleFlags2,
  type RendererType2,
  type TemplateRef,
  type Type,
} from '@angular/core';
// Angular's renderer for Inkoxide: a `RendererFactory2` whose renderers create and insert host
// nodes of `@inkoxide/tree` instead of DOM nodes. A template component renders once: it is
// created, its inputs set, and its view checked once, synchronously.
import {
  BOOLEAN_PROPS,
  checkTree,
  type Container,
  dedupe,
  hostName,
  type HostType,
  type Instance,
  isHostType,
  type Node,
  type PageInfo,
  TAGS,
  TemplateError,
  type TextInstance,
  type Trace,
  trimTemplateText,
} from '@inkoxide/tree';

/** The page a `render` template renders for: `inject(INK_PAGE)` in a component inside it. */
export const INK_PAGE = new InjectionToken<PageInfo>('inkoxide page');

/** The component whose template made a node, and the element it is rendered in. */
interface Owner {
  name: string | undefined;
  host: AnyNode | undefined;
}

/** What a node keeps for its component stack. */
interface Origin {
  owner: Owner | undefined;
}

/** Where Angular renders nothing (`@if`, `@for`, `ng-container`), kept until the tree is read. */
interface Comment {
  kind: 'comment';
  origin: Origin;
}

/** An element that is not a host type: a component's (`<app-address>`), or a mistake (`<div>`). */
interface Foreign extends Omit<Instance, 'type'> {
  type: string;
}

type AnyNode = Instance | Foreign | TextInstance | Comment;
type Parent = Instance | Foreign;

const parents = new WeakMap<AnyNode, Parent>();
/** The elements components are rendered in: their content replaces them in the tree. */
const componentHosts = new WeakSet<AnyNode>();

/** The components above an owner, innermost first. */
function ownerNames(owner: Owner | undefined): string[] {
  const names: string[] = [];
  for (let o = owner; o; o = (o.host?.origin as Origin | undefined)?.owner) if (o.name) names.push(o.name);
  return names;
}

/** An element's stack: its component, then those above it. A string's: the element it is in. */
const trace: Trace = (origin) => dedupe(ownerNames((origin as Origin | undefined)?.owner));
const elementTrace =
  (type: string): Trace =>
  (origin) =>
    dedupe([hostName(type), ...ownerNames((origin as Origin | undefined)?.owner)]);

function detach(node: AnyNode): void {
  const p = parents.get(node);
  if (!p) return;
  const i = p.children.indexOf(node as Node);
  if (i >= 0) p.children.splice(i, 1);
  parents.delete(node);
}

const camel = (name: string): string => name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

/** A prop from an attribute or a property: `class` is `className`, a bare boolean attribute `true`. */
function setProp(el: Parent, name: string, value: unknown): void {
  if (name === 'class') {
    el.props.className = value;
    return;
  }
  if (
    name === 'render' &&
    value &&
    typeof (value as TemplateRef<unknown>).createEmbeddedView === 'function'
  ) {
    // The builder calls `render` for each page; `renderPage` renders the template it carries.
    el.props.render = Object.assign(() => undefined, { template: value });
    return;
  }
  if (name === 'inkStyle') {
    el.props.style = el.props.style === undefined ? value : [el.props.style, value];
    return;
  }
  el.props[name] = value === '' && BOOLEAN_PROPS.has(name) ? true : value;
}

class InkRenderer implements Renderer2 {
  readonly data: Record<string, unknown> = {};
  readonly destroyNode = null;

  constructor(private readonly owner: Owner | undefined) {}

  destroy(): void {}

  createElement(name: string): AnyNode {
    const tag = TAGS.get(name);
    const type: string = tag?.type ?? name;
    const el: Foreign = {
      kind: 'element',
      type,
      props: tag?.kind ? { kind: tag.kind } : {},
      children: [],
      origin: { owner: this.owner } satisfies Origin,
      trace: elementTrace(type),
    };
    return el;
  }

  createComment(): AnyNode {
    return { kind: 'comment', origin: { owner: this.owner } };
  }

  createText(value: string): AnyNode {
    return { kind: 'text', text: value, origin: { owner: this.owner } satisfies Origin, trace };
  }

  appendChild(parent: Parent, child: AnyNode): void {
    this.insertBefore(parent, child, null);
  }

  insertBefore(parent: Parent, child: AnyNode, ref: AnyNode | null): void {
    if (!parent || !('children' in parent)) return;
    detach(child);
    const at = ref ? parent.children.indexOf(ref as Node) : -1;
    parent.children.splice(at < 0 ? parent.children.length : at, 0, child as Node);
    parents.set(child, parent);
  }

  removeChild(_parent: Parent | null, old: AnyNode): void {
    detach(old);
  }

  selectRootElement(selectorOrNode: unknown): AnyNode {
    return selectorOrNode as AnyNode;
  }

  parentNode(node: AnyNode): Parent | null {
    return parents.get(node) ?? null;
  }

  nextSibling(node: AnyNode): AnyNode | null {
    const p = parents.get(node);
    if (!p) return null;
    return (p.children[p.children.indexOf(node as Node) + 1] as AnyNode | undefined) ?? null;
  }

  setAttribute(el: Parent, name: string, value: string): void {
    if (name === 'ng-version') return;
    setProp(el, name, value);
  }

  removeAttribute(el: Parent, name: string): void {
    delete el.props[name === 'class' ? 'className' : name];
  }

  addClass(el: Parent, name: string): void {
    const now = typeof el.props.className === 'string' ? el.props.className.split(/\s+/) : [];
    if (!now.includes(name)) el.props.className = [...now, name].filter(Boolean).join(' ');
  }

  removeClass(el: Parent, name: string): void {
    if (typeof el.props.className !== 'string') return;
    el.props.className = el.props.className
      .split(/\s+/)
      .filter((c) => c && c !== name)
      .join(' ');
  }

  setStyle(el: Parent, style: string, value: unknown, _flags?: RendererStyleFlags2): void {
    const flat = (el.props.style ?? {}) as Record<string, unknown>;
    el.props.style = { ...flat, [camel(style)]: value };
  }

  removeStyle(el: Parent, style: string): void {
    const flat = { ...(el.props.style as Record<string, unknown> | undefined) };
    delete flat[camel(style)];
    el.props.style = flat;
  }

  setProperty(el: Parent, name: string, value: unknown): void {
    setProp(el, name, value);
  }

  setValue(node: AnyNode, value: string): void {
    if (node.kind === 'text') node.text = value;
  }

  // Listeners mean nothing in a PDF.
  listen(): () => void {
    return () => {};
  }
}

/** Gives each component's template a renderer that knows the component, for the stacks. */
class InkRendererFactory implements RendererFactory2 {
  createRenderer(host: unknown, type: RendererType2 | null): Renderer2 {
    if (!type) return new InkRenderer(undefined);
    if (host) componentHosts.add(host as AnyNode);
    const component = (type as { type?: { name?: string } }).type;
    const owner: Owner = { name: component?.name || undefined, host: (host as AnyNode | null) ?? undefined };
    return new InkRenderer(owner);
  }
}

/** The tree as Inkoxide reads it: components' host elements replaced by their content, no comments. */
function read(nodes: readonly AnyNode[]): Node[] {
  const out: Node[] = [];
  for (const n of nodes) {
    if (n.kind === 'comment') continue;
    if (n.kind === 'element' && componentHosts.has(n) && !isHostType(n.type)) {
      out.push(...read(n.children as AnyNode[]));
      continue;
    }
    if (n.kind === 'element') n.children = read(n.children as AnyNode[]);
    out.push(n as Node);
  }
  return out;
}

const rendererFactory = new InkRendererFactory();
/** Angular asks for a document to put component styles in; templates have none to put. */
const NO_DOCUMENT = { head: null };

let environment: EnvironmentInjector | undefined;

/** The injector templates are created in: their renderer, and no zone or scheduler. */
function environmentInjector(): EnvironmentInjector {
  environment ??= createEnvironmentInjector(
    [
      { provide: RendererFactory2, useValue: rendererFactory },
      { provide: DOCUMENT, useValue: NO_DOCUMENT },
    ],
    null as unknown as EnvironmentInjector,
    'Inkoxide',
  );
  return environment;
}

/** Turns what a template threw into a TemplateError. */
function templateError(error: unknown): TemplateError {
  if (error instanceof TemplateError) return error;
  const message = error instanceof Error ? error.message : String(error);
  return new TemplateError(`The template threw: ${message}`, { kind: 'render', cause: error });
}

/** Reads a finished render: host elements of components replaced, white space trimmed, checked. */
function finish(nodes: readonly AnyNode[], parent?: HostType): Container {
  const container: Container = parent === undefined ? { children: [] } : { children: [], parent };
  container.children = trimTemplateText(read(nodes), parent);
  checkTree(container);
  return container;
}

/**
 * Renders a component into host nodes, synchronously: created with `inputs`, checked once, then
 * read. Throws a TemplateError.
 */
export function mount(component: Type<unknown>, inputs: Record<string, unknown> = {}): Container {
  const root: Foreign = { kind: 'element', type: 'root', props: {}, children: [], origin: undefined };
  try {
    const ref = createComponent(component, {
      environmentInjector: environmentInjector(),
      hostElement: root as unknown as Element,
    });
    for (const [name, value] of Object.entries(inputs)) ref.setInput(name, value);
    ref.changeDetectorRef.detectChanges();
  } catch (e) {
    throw templateError(e);
  }
  return finish(root.children as AnyNode[]);
}

/**
 * The content of an element's `render` template for one page: the template rendered with the page
 * as its context (`let-page`, `let-n="pageNumber"`) and `INK_PAGE`, inside the element.
 */
export function renderPage(inst: Instance, page: PageInfo): Node[] {
  const template = (inst.props.render as { template?: TemplateRef<unknown> }).template;
  if (!template) {
    throw new TemplateError(
      `<${hostName(inst.type)} [render]> takes an <ng-template>: [render]="page" with <ng-template #page let-page>.`,
      {
        kind: 'props',
      },
    );
  }
  try {
    const injector = Injector.create({ providers: [{ provide: INK_PAGE, useValue: page }] });
    const view = template.createEmbeddedView({ $implicit: page, ...page }, injector);
    view.detectChanges();
    return finish(view.rootNodes as AnyNode[], inst.type).children;
  } catch (e) {
    throw templateError(e);
  }
}
