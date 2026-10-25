// Vue's custom renderer for Inkoxide: Vue creates and inserts host nodes of `@inkoxide/tree`
// instead of DOM nodes. Templates render once and synchronously, so the update paths exist only
// for completeness.
import {
  assignProp,
  finishTemplate,
  componentNames,
  type Container,
  dedupe,
  formatStack,
  hostName,
  nodeLinks,
  type HostType,
  type Instance,
  type Node,
  type PageInfo,
  TemplateError,
  type TextInstance,
  templateError,
  type Trace,
} from '@inkoxide/tree';
import {
  camelize,
  type ComponentInternalInstance,
  type ComponentPublicInstance,
  createRenderer,
  defineComponent,
  getCurrentInstance,
  type PropType,
  provide,
  type VNode,
} from 'vue';

/** Where Vue renders nothing (`v-if`), kept until the tree is read; never in a finished tree. */
interface Comment {
  kind: 'comment';
}

type HostNode = Node | Comment;
type Parent = Instance | Container;

/**
 * The prop that carries the component instance that rendered a host element (a component of
 * this package), for the component stack. It never reaches the element's props.
 */
export const OWNER = '__inkOwner';

/** The provide key of the page a `render` slot renders for. */
export const PAGE = Symbol('inkoxide page');

const links = nodeLinks<HostNode, Parent>();

/** The name Vue gives a component: its `name`, else the one the SFC compiler infers from the file. */
function nameOf(i: ComponentInternalInstance): string | undefined {
  const t = i.type as { name?: string; __name?: string; displayName?: string };
  return t.name ?? t.__name ?? t.displayName;
}

/**
 * The components above the instance that rendered a host element, innermost first. A `render`
 * slot's content is rendered in a root of its own (`PageRoot`): the components around the element
 * that takes the slot carry its stack on.
 */
const vueNames: Trace = (origin) => {
  const names: string[] = [];
  for (let i = origin as ComponentInternalInstance | null; i; i = i.parent) {
    if (i.type === PageRoot) {
      names.push(...componentNames((i.props as { host: Instance }).host));
      break;
    }
    const n = nameOf(i);
    if (n) names.push(n);
  }
  return dedupe(names);
};

/** A string's stack: that of the element it is in. */
const textNames: Trace = (parent) => (parent ? componentNames(parent as Instance) : []);

function insert(child: HostNode, parent: Parent, anchor?: HostNode | null): void {
  links.insert(child, parent, anchor);
  // A string's stack is that of the element it is in.
  if (child.kind === 'text') child.origin = 'kind' in parent ? parent : undefined;
}

const { createApp } = createRenderer<HostNode, Parent>({
  createElement: (type): Instance => ({
    kind: 'element',
    type: type as HostType,
    props: {},
    children: [],
    origin: undefined,
    trace: vueNames,
  }),
  createText: (text): TextInstance => ({ kind: 'text', text, origin: undefined, trace: textNames }),
  createComment: (): Comment => ({ kind: 'comment' }),
  setText(node, text) {
    if (node.kind === 'text') node.text = text;
  },
  setElementText(el, text) {
    if (!('kind' in el)) return;
    for (const child of [...el.children] as HostNode[]) links.remove(child);
    if (text) insert({ kind: 'text', text, origin: undefined, trace: textNames }, el);
  },
  insert,
  remove: links.remove,
  parentNode: links.parent,
  nextSibling: links.next,
  patchProp(el, key, _prev, next, _namespace, parentComponent) {
    if (!('kind' in el)) return;
    if (key === OWNER) {
      el.origin = next;
      return;
    }
    if (/^on[A-Z]/.test(key) && next !== undefined && next !== null) {
      const stack = [hostName(el.type), ...(parentComponent ? vueNames(parentComponent) : [])];
      throw new TemplateError(
        `<${hostName(el.type)}> takes no listener (${key}): nothing happens in a PDF. Remove the @${key.slice(2, 3).toLowerCase()}${key.slice(3)} binding.`,
        { kind: 'props', componentStack: formatStack(dedupe(stack)) },
      );
    }
    assignProp(el.props, key === 'class' ? key : camelize(key), next);
  },
});

/**
 * The root of a `render` slot's content for one page: it gives the page to `usePage()`, and the
 * components around the element that takes the slot give it what they provide.
 */
export const PageRoot = defineComponent({
  name: 'PageRoot',
  props: {
    host: { type: Object as PropType<Instance>, required: true },
    page: { type: Object as PropType<PageInfo>, required: true },
    fn: { type: Function as PropType<(page: PageInfo) => unknown>, required: true },
  },
  setup(props) {
    const self = getCurrentInstance() as ComponentInternalInstance & { provides: object };
    const owner = props.host.origin as (ComponentInternalInstance & { provides: object }) | undefined;
    if (owner) self.provides = Object.create(owner.provides) as object;
    provide(PAGE, props.page);
    return () => props.fn(props.page) as VNode[];
  },
});

/** Removes the comments Vue leaves where nothing renders. */
function uncomment(nodes: Node[]): Node[] {
  const out = (nodes as HostNode[]).filter((n): n is Node => n.kind !== 'comment');
  for (const n of out) if (n.kind === 'element') n.children = uncomment(n.children);
  return out;
}

interface VNodeLike {
  type: unknown;
  component: ComponentInternalInstance | null;
  children: unknown;
  suspense?: unknown;
}

/** The error of a Vapor component, which renders without VNodes, so through no renderer of ours. */
function vaporError(
  type: { name?: string; __name?: string },
  parent?: ComponentInternalInstance | null,
): TemplateError {
  const name = type.name ?? type.__name ?? 'A component';
  return new TemplateError(
    `${name} is a Vapor component, which renders without VNodes, so Inkoxide's renderer never sees it. Write PDF templates as ordinary components (no <script setup vapor>).`,
    { kind: 'render', componentStack: formatStack(dedupe([name, ...(parent ? vueNames(parent) : [])])) },
  );
}

/**
 * Where Vue 3.6 mounts a Vapor component found in a VNode tree (`app._context.vapor`, which its
 * interop plugin would set): each way in is the error, so that Vue 3.6 says so instead of failing
 * on the missing interop. Vue 3.5 renders a Vapor component as an ordinary one, and `suspension`
 * refuses it after.
 */
const NO_VAPOR = {
  mount(
    vnode: { type: object },
    _container: unknown,
    _anchor: unknown,
    parent: ComponentInternalInstance | null,
  ) {
    throw vaporError(vnode.type, parent);
  },
  activate(
    vnode: { type: object },
    _container: unknown,
    _anchor: unknown,
    parent: ComponentInternalInstance | null,
  ) {
    throw vaporError(vnode.type, parent);
  },
  update(_old: unknown, vnode: { type: object }) {
    throw vaporError(vnode.type);
  },
};

const advice =
  'Suspense, async setup() and async components are not supported: a template renders once, synchronously. Load the data first and pass it in as props.';

/** The error of a component that did not render synchronously, if any. */
function suspension(
  vnode: VNodeLike | null | undefined,
  seen = new Set<unknown>(),
): TemplateError | undefined {
  if (!vnode || seen.has(vnode)) return undefined;
  seen.add(vnode);
  const type = vnode.type as {
    __isSuspense?: boolean;
    __asyncLoader?: unknown;
    __asyncResolved?: unknown;
    __vapor?: boolean;
    name?: string;
    __name?: string;
  };
  if (type && typeof type === 'object' && type.__isSuspense) {
    return new TemplateError(`A component used <Suspense>. ${advice}`, { kind: 'render' });
  }
  if (type && typeof type === 'object' && type.__vapor) {
    const name = type.name ?? type.__name ?? 'A component';
    return new TemplateError(
      `${name} is a Vapor component, which renders without VNodes, so Inkoxide's renderer never sees it. Write PDF templates as ordinary components (no <script setup vapor>).`,
      {
        kind: 'render',
        componentStack: vnode.component ? formatStack(vueNames(vnode.component)) : undefined,
      },
    );
  }
  const c = vnode.component;
  if (c) {
    const stack = formatStack(vueNames(c));
    if ((c.type as typeof type).__asyncLoader && !(c.type as typeof type).__asyncResolved) {
      return new TemplateError(`An async component was not loaded. ${advice}`, {
        kind: 'render',
        componentStack: stack,
      });
    }
    if ((c as { asyncDep?: unknown }).asyncDep && !(c as { asyncResolved?: boolean }).asyncResolved) {
      return new TemplateError(`${nameOf(c) ?? 'A component'} has an async setup(). ${advice}`, {
        kind: 'render',
        componentStack: stack,
      });
    }
    return suspension(c.subTree as unknown as VNodeLike, seen);
  }
  if (Array.isArray(vnode.children)) {
    for (const child of vnode.children) {
      const e = suspension(child as VNodeLike, seen);
      if (e) return e;
    }
  }
  return undefined;
}

/**
 * Renders a component (or a function that returns VNodes) into host nodes, synchronously: one
 * mount, then the container is read and checked. `parent` is the host type a `render` slot
 * renders into. Throws a TemplateError with the component stack. The container's `dispose`
 * unmounts the app; a render that fails is unmounted at once.
 */
export function mount(root: object, parent?: HostType): Container {
  const container: Container = { children: [] };
  let captured: { error: unknown; instance: ComponentInternalInstance | null } | undefined;
  const app = createApp(root);
  (app._context as { vapor?: unknown }).vapor = NO_VAPOR;
  app.config.errorHandler = (error: unknown, instance: ComponentPublicInstance | null) => {
    captured ??= { error, instance: instance?.$ ?? null };
  };
  app.config.warnHandler = () => {};
  let mounted = false;
  const unmount = () => {
    if (mounted) app.unmount();
    mounted = false;
  };
  try {
    app.mount(container);
    mounted = true;
  } catch (error) {
    captured ??= { error, instance: null };
  }
  if (captured) {
    const { error, instance } = captured;
    // The stack first: unmounting ends the instances it is read from.
    const stack = instance ? formatStack(vueNames(instance)) : undefined;
    unmount();
    throw templateError(error, stack);
  }
  const top = (app as unknown as { _instance: ComponentInternalInstance | null })._instance;
  const suspended = suspension(top?.subTree as unknown as VNodeLike);
  if (suspended) {
    unmount();
    throw suspended;
  }
  // A copy: unmounting takes the nodes out of the container.
  return finishTemplate(uncomment([...container.children]), parent, unmount);
}
