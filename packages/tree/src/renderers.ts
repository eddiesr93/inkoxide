// What the renderers of template languages share (Vue's, Angular's, Svelte's reading): the props
// as attributes set them, the links of nodes to their parents, and stacks from traces. Exported by
// `@inkoxide/tree/internal`, for Inkoxide's own front ends.
import { BOOLEAN_PROPS } from './components/index.ts';
import { TemplateError } from './errors.ts';
import { componentNames, formatStack, hostName, type Instance, type Trace, withoutRepeats } from './nodes.ts';
import { templateError } from './template.ts';

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

/**
 * The error of a listener on an element (`@click` in Vue, `(click)` in Angular): nothing happens
 * in a PDF. `type` is the element's (none when the framework gives no element), `event` the event
 * as the framework names it, `binding` the binding as the template writes it, and `names` the
 * element's stack.
 */
export function listenerError(
  type: string | undefined,
  event: string,
  binding: string,
  names: readonly string[] = [],
): TemplateError {
  const element = type === undefined ? 'An element' : `<${hostName(type)}>`;
  return new TemplateError(
    `${element} takes no listener (${event}): nothing happens in a PDF. Remove the ${binding} binding.`,
    { kind: 'props', componentStack: names.length > 0 ? formatStack(withoutRepeats(names)) : undefined },
  );
}

/** A string's stack, for renderers whose strings keep the element they are in as their `origin`. */
export const textInElement: Trace = (parent) => (parent ? componentNames(parent as Instance) : []);

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
