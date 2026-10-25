// What every front end does around its own render: the error of a template that threw, the
// reading of what it rendered, and the two functions a template becomes a document tree with.
import type { Document } from '@inkoxide/schema';

import { BOOLEAN_PROPS, type HostType } from './components/index.ts';
import { type BuildHooks, buildDocument, type TreeOptions, type TreeResult } from './document.ts';
import { TemplateError } from './errors.ts';
import { type Container, type Node, trimTemplateText } from './nodes.ts';
import { checkTree } from './placement.ts';
import { loadTailwind } from './tailwind.ts';

/**
 * The TemplateError of what a template threw: a TemplateError as it is (given the stack when it
 * has none), anything else as a `render` error that carries it.
 */
export function templateError(error: unknown, componentStack?: string): TemplateError {
  if (error instanceof TemplateError) {
    error.componentStack ??= componentStack;
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  return new TemplateError(`The template threw: ${message}`, {
    kind: 'render',
    componentStack,
    cause: error,
  });
}

/**
 * The container of the nodes a template language rendered: their text as JSX gives it
 * (`trimTemplateText`), checked for where each component is. `parent` is the host type a `render`
 * prop's content renders into. `dispose` releases the render: the container keeps it, and a render
 * the check refuses is released at once.
 */
export function finishTemplate(nodes: Node[], parent?: HostType, dispose?: () => void): Container {
  const container: Container = parent === undefined ? { children: [] } : { children: [], parent };
  container.children = trimTemplateText(nodes, parent);
  try {
    checkTree(container);
  } catch (e) {
    dispose?.();
    throw e;
  }
  if (dispose) container.dispose = dispose;
  return container;
}

/** A front end's template functions; `Input` is what its users render. */
export interface TemplateFunctions<Input> {
  /**
   * Renders a template to an Inkoxide document. Synchronous: the template renders once, then its
   * host nodes are walked into the schema tree. Throws TemplateError with a component stack.
   */
  renderTemplate(input: Input, opts?: TreeOptions): TreeResult;
  /**
   * The document tree of a template (the simple API). Loads Tailwind first when it is installed.
   * A template error is a rejected promise, not a throw.
   */
  renderToTree(input: Input, opts?: TreeOptions): Promise<Document>;
}

/** The template functions of a front end that renders an input with `mount`. */
export function templateFunctions<Input>(
  mount: (input: Input) => Container,
  hooks: BuildHooks,
): TemplateFunctions<Input> {
  const renderTemplate = (input: Input, opts: TreeOptions = {}): TreeResult => {
    const t0 = performance.now();
    const container = mount(input);
    return buildDocument(container, hooks, opts, performance.now() - t0);
  };
  return {
    renderTemplate,
    async renderToTree(input, opts = {}) {
      await loadTailwind();
      const tree = renderTemplate(input, opts);
      tree.dispose();
      return tree.document;
    },
  };
}

/**
 * The names an error's trace shows, as `pattern`'s first group finds them, innermost first, each
 * at its outermost place: a component's snippets and templates run inside the components it gives
 * them to, so its name shows there too, before the components around it. Empty when the trace has
 * none (bundled code that lost its names).
 */
export function namesInTrace(
  error: unknown,
  pattern: RegExp,
  keep: (name: string) => boolean = () => true,
): string[] {
  const trace = error instanceof Error ? error.stack : undefined;
  if (!trace) return [];
  const names = [...trace.matchAll(pattern)].map((m) => m[1] as string).filter(keep);
  return names.filter((name, i) => !names.includes(name, i + 1));
}

/**
 * Runs a template language's render with a long trace (for `namesInTrace`): what it throws is a
 * TemplateError, with the stack `stackOf` reads from the error.
 */
export function renderWithTrace<T>(render: () => T, stackOf: (error: unknown) => string | undefined): T {
  const limit = Error.stackTraceLimit;
  Error.stackTraceLimit = Math.max(limit, 200);
  try {
    return render();
  } catch (e) {
    throw templateError(e, stackOf(e));
  } finally {
    Error.stackTraceLimit = limit;
  }
}

/** `class` as template languages give it (a string, a list, an object of flags) as one string. */
export function classNames(value: unknown): string {
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(classNames).filter(Boolean).join(' ');
  if (value && typeof value === 'object') {
    return Object.entries(value)
      .filter(([, on]) => on)
      .map(([name]) => name)
      .join(' ');
  }
  return '';
}

/**
 * Sets a prop of a host element from an attribute or a binding of a template language: `class` is
 * `className`; a bare attribute (`''`) of a prop that takes `true` is `true`; `undefined` and
 * `null` take the prop away.
 */
export function assignProp(props: Record<string, unknown>, name: string, value: unknown): void {
  if (value === undefined || value === null) {
    delete props[name === 'class' ? 'className' : name];
    return;
  }
  if (name === 'class') props.className = classNames(value);
  else props[name] = value === '' && BOOLEAN_PROPS.has(name) ? true : value;
}

/** A node of a renderer's tree: what has children, and what goes in them. */
interface Linked {
  children: unknown[];
}

/**
 * The parent of each node a renderer inserts, for the node operations of front ends that build
 * the tree as the framework asks (Vue's renderer, Angular's): insert before an anchor, remove, the
 * parent and the next sibling.
 */
export function nodeLinks<N extends object, P extends Linked>() {
  const parents = new WeakMap<N, P>();
  const remove = (node: N): void => {
    const p = parents.get(node);
    if (!p) return;
    const i = p.children.indexOf(node);
    if (i >= 0) p.children.splice(i, 1);
    parents.delete(node);
  };
  return {
    insert(node: N, parent: P, anchor?: N | null): void {
      remove(node);
      const at = anchor ? parent.children.indexOf(anchor) : -1;
      parent.children.splice(at < 0 ? parent.children.length : at, 0, node);
      parents.set(node, parent);
    },
    remove,
    parent: (node: N): P | null => parents.get(node) ?? null,
    next(node: N): N | null {
      const p = parents.get(node);
      if (!p) return null;
      return (p.children[p.children.indexOf(node) + 1] as N | undefined) ?? null;
    },
  };
}
