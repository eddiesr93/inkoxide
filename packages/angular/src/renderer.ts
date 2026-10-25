// Angular's renderer for Inkoxide: a `RendererFactory2` whose renderers create and insert host
// nodes of `@inkoxide/tree` instead of DOM nodes. A template component renders once: it is
// created, its inputs set, and its view checked once, synchronously.

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
  ɵINJECTOR_SCOPE as INJECTOR_SCOPE,
} from '@angular/core';
import {
  type CommentNode,
  componentNames,
  type Container,
  finishTemplate,
  formatStack,
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
  withoutRepeats,
} from '@inkoxide/tree';
import { assignProp, listenerError, namesInTrace, nodeLinks, renderWithTrace } from '@inkoxide/tree/internal';

/** The page a `render` template renders for: `inject(INK_PAGE)` in a component inside it. */
export const INK_PAGE = new InjectionToken<PageInfo>('inkoxide page');

/** The component whose template made a node, the element it is rendered in, and its render's. */
interface Owner {
  name: string | undefined;
  host: Parent | undefined;
  render: Owners;
}

/** The last component of each name a render created, for the stacks of what its templates throw. */
type Owners = Map<string, Owner>;

/** What a node keeps for its component stack: the component whose template made it, and itself. */
interface Origin {
  owner: Owner | undefined;
  node?: AnyNode;
}

/** An element that is not a host type: a component's (`<app-address>`), or a mistake (`<div>`). */
interface Foreign extends Omit<Instance, 'type'> {
  type: string;
}

/** Where Angular renders nothing (`@if`, `@for`, `ng-container`) is a comment: `finishTemplate` drops it. */
type AnyNode = Instance | Foreign | TextInstance | CommentNode;
type Parent = Instance | Foreign;

const links = nodeLinks<AnyNode, Parent>();
/** The elements components are rendered in, and their components: their content replaces them in the tree. */
const componentHosts = new WeakMap<AnyNode, Owner>();
/** The root nodes of a `render` template's view, and the element whose template it is. */
const pageHosts = new WeakMap<AnyNode, Instance>();

/**
 * A node's stack, as React's names it: the elements around it, innermost first, by their host type
 * (`Text`), and in place of a component's host element its component (`Invoice`). The stack of a
 * `render` template's content goes on with the element whose template it is.
 */
const trace: Trace = (origin) => {
  const names: string[] = [];
  let top: AnyNode | undefined;
  for (let n = (origin as Origin | undefined)?.node ?? null; n; n = links.parent(n)) {
    top = n;
    if (n.kind !== 'element') continue;
    const component = componentHosts.get(n);
    if (!component) names.push(hostName(n.type));
    else if (component.name) names.push(component.name);
  }
  const page = top && pageHosts.get(top);
  if (page) names.push(...componentNames(page));
  return withoutRepeats(names);
};

const camel = (name: string): string => name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

/**
 * A prop from an attribute or a property, as every template front end reads it (`assignProp`);
 * `render` takes an `ng-template`, and `inkStyle` styles past what `[style]` takes.
 */
function setProp(el: Parent, name: string, value: unknown): void {
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
  assignProp(el.props, name, value);
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
      origin: undefined,
      trace,
    };
    el.origin = { owner: this.owner, node: el } satisfies Origin;
    return el;
  }

  createComment(): AnyNode {
    return { kind: 'comment' };
  }

  createText(value: string): AnyNode {
    const text: TextInstance = { kind: 'text', text: value, origin: undefined, trace };
    text.origin = { owner: this.owner, node: text } satisfies Origin;
    return text;
  }

  appendChild(parent: Parent, child: AnyNode): void {
    this.insertBefore(parent, child, null);
  }

  insertBefore(parent: Parent, child: AnyNode, ref: AnyNode | null): void {
    if (parent && 'children' in parent) links.insert(child, parent, ref);
  }

  removeChild(_parent: Parent | null, old: AnyNode): void {
    links.remove(old);
  }

  selectRootElement(selectorOrNode: unknown): AnyNode {
    return selectorOrNode as AnyNode;
  }

  parentNode(node: AnyNode): Parent | null {
    return links.parent(node);
  }

  nextSibling(node: AnyNode): AnyNode | null {
    return links.next(node);
  }

  setAttribute(el: Parent, name: string, value: string): void {
    if (name === 'ng-version') return;
    setProp(el, name, value);
  }

  removeAttribute(el: Parent, name: string): void {
    assignProp(el.props, name, undefined);
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

  listen(target: AnyNode | string, event: string): () => void {
    const el = typeof target === 'string' || target.kind !== 'element' ? undefined : target;
    throw listenerError(el?.type, event, `(${event})`, el ? trace(el.origin) : []);
  }
}

/** The components of the render running now; a render's own, so that renders do not mix them. */
let owners: Owners = new Map();

/** Runs a render with its own components, which `own` starts with. */
function withOwners<T>(own: Owners, run: () => T): T {
  const outer = owners;
  owners = own;
  try {
    return run();
  } finally {
    owners = outer;
  }
}

/** Gives each component's template a renderer that knows the component, for the stacks. */
class InkRendererFactory implements RendererFactory2 {
  createRenderer(host: unknown, type: RendererType2 | null): Renderer2 {
    if (!type) return new InkRenderer(undefined);
    const component = (type as { type?: { name?: string } }).type;
    const owner: Owner = {
      name: component?.name || undefined,
      host: (host as Parent | null) ?? undefined,
      render: owners,
    };
    if (owner.host) componentHosts.set(owner.host, owner);
    if (owner.name) owners.set(owner.name, owner);
    return new InkRenderer(owner);
  }
}

/**
 * The tree as Inkoxide reads it: components' host elements replaced by their content. The comments
 * stay, for `finishTemplate` to drop.
 */
function read(nodes: readonly AnyNode[]): (Node | CommentNode)[] {
  const out: (Node | CommentNode)[] = [];
  for (const n of nodes) {
    if (n.kind === 'element' && componentHosts.has(n) && !isHostType(n.type)) {
      out.push(...read(n.children as AnyNode[]));
      continue;
    }
    // Comments among the children until `finishTemplate` drops them.
    if (n.kind === 'element') n.children = read(n.children as AnyNode[]) as Node[];
    out.push(n as Node | CommentNode);
  }
  return out;
}

const rendererFactory = new InkRendererFactory();
/** Angular asks for a document to put component styles in; templates have none to put. */
const NO_DOCUMENT = { head: null };

let environment: EnvironmentInjector | undefined;

/**
 * The injector templates are created in: their renderer, and no zone or scheduler. It is a root
 * injector, as an application's is, so that templates inject what is `providedIn: 'root'`
 * (`LOCALE_ID`, which the `date`, `currency` and `number` pipes read, and their own services);
 * each is made once, and every render shares it.
 */
function environmentInjector(): EnvironmentInjector {
  environment ??= createEnvironmentInjector(
    [
      { provide: INJECTOR_SCOPE, useValue: 'root' },
      { provide: RendererFactory2, useValue: rendererFactory },
      { provide: DOCUMENT, useValue: NO_DOCUMENT },
    ],
    null as unknown as EnvironmentInjector,
    'Inkoxide',
  );
  return environment;
}

/** A component in a trace: its template (`Invoice_Template`, `Invoice_For_3_Template`), constructor or method. */
const FRAME = /\bat (?:new )?([A-Z][\w$]*?)(?:_[\w$]*)?(?:_Template|_Factory|\.[\w$]+)?\b(?= \(|$)/gm;
/** Classes in a trace that are not components: Angular's, and Inkoxide's (`Builder` renders the pages). */
const NOT_COMPONENTS =
  /^(?:Object|Array|Function|Promise|Reflect|R3Injector|ChainedInjector|NodeInjector|ComponentFactory|ViewRef|ApplicationRef|InkRenderer|InkRendererFactory|Builder|Module|ModuleJob|Error|TypeError)$/;

/**
 * The stack of an error a template threw: the components its trace names (see `namesInTrace`),
 * innermost first, then the elements and components around the innermost one's host element, as
 * a placement error names them. A component's template runs from its parent's check, not from its
 * parent's template, so the trace alone does not say where it is.
 */
export function angularStack(error: unknown): string | undefined {
  const named = namesInTrace(error, FRAME, (name) => !NOT_COMPONENTS.test(name));
  const [innermost] = named;
  if (innermost === undefined) return undefined;
  const host = owners.get(innermost)?.host;
  const around = host ? trace(host.origin) : [];
  return formatStack(withoutRepeats([...named.filter((n) => !around.includes(n)), ...around]));
}

/**
 * Reads a finished render: host elements of components replaced, then as every template front end
 * does; `destroy` releases it.
 */
const finish = (nodes: readonly AnyNode[], parent: HostType | undefined, destroy: () => void): Container =>
  finishTemplate(read(nodes), parent, destroy);

/** Checks a view once; a view whose check fails is destroyed. */
function checked<V extends { destroy(): void }>(view: V, check: () => void): V {
  try {
    check();
  } catch (e) {
    view.destroy();
    throw e;
  }
  return view;
}

/**
 * Renders a component into host nodes, synchronously: created with `inputs`, checked once, then
 * read. Throws a TemplateError. The container's `dispose` destroys the component.
 */
export function mount(component: Type<unknown>, inputs: Record<string, unknown> = {}): Container {
  const root: Foreign = { kind: 'element', type: 'root', props: {}, children: [], origin: undefined };
  root.origin = { owner: undefined, node: root } satisfies Origin;
  const ref = withOwners(new Map(), () =>
    renderWithTrace(() => {
      const created = createComponent(component, {
        environmentInjector: environmentInjector(),
        hostElement: root as unknown as Element,
      });
      return checked(created, () => {
        for (const [name, value] of Object.entries(inputs)) created.setInput(name, value);
        created.changeDetectorRef.detectChanges();
      });
    }, angularStack),
  );
  return finish(root.children as AnyNode[], undefined, () => ref.destroy());
}

/**
 * The content of an element's `render` template for one page: the template rendered with the page
 * as its context (`let-page`, `let-n="pageNumber"`) and `INK_PAGE`, inside the element.
 */
export function renderPage(inst: Instance, page: PageInfo): Container {
  const template = (inst.props.render as { template?: TemplateRef<unknown> }).template;
  if (!template) {
    throw new TemplateError(
      `<${hostName(inst.type)} [render]> takes an <ng-template>: [render]="page" with <ng-template #page let-page>.`,
      {
        kind: 'props',
      },
    );
  }
  // The components of the document's render, which the template's code may run in, then its own.
  const around = (inst.origin as Origin | undefined)?.owner?.render;
  const view = withOwners(new Map(around), () =>
    renderWithTrace(() => {
      const injector = Injector.create({ providers: [{ provide: INK_PAGE, useValue: page }] });
      const created = template.createEmbeddedView({ $implicit: page, ...page }, injector);
      return checked(created, () => created.detectChanges());
    }, angularStack),
  );
  for (const node of view.rootNodes as AnyNode[]) pageHosts.set(node, inst);
  return finish(view.rootNodes as AnyNode[], inst.type, () => view.destroy());
}
