import { HOST_NAME, type HostType } from './components/index.ts';

/**
 * The components above a host node, innermost first: its own name (`Text`), then the names of
 * the components that rendered it (`Field`, `Invoice`). Each front end reads them from what it
 * keeps in `origin` (a React fiber, a Vue component instance, …).
 */
export type Trace = (origin: unknown) => readonly string[];

/** A host element: plain data, no DOM and no engine types. */
export interface Instance {
  kind: 'element';
  type: HostType;
  props: Record<string, unknown>;
  children: Node[];
  /** What the front end keeps to name the components above the element; read by `trace`. */
  origin: unknown;
  /** Names the components above the element, from `origin`; shared by every node of a render. */
  trace?: Trace;
}

export interface TextInstance {
  kind: 'text';
  text: string;
  origin: unknown;
  trace?: Trace;
}

export type Node = Instance | TextInstance;

export interface Container {
  children: Node[];
  /** Host type the content is rendered into, for nested roots (`render` props). */
  parent?: HostType;
}

/** True for the host types of the components; false for `div` and other elements. */
export function isHostType(type: string): type is HostType {
  return Object.hasOwn(HOST_NAME, type);
}

/** The component name of a host type (`View`), or the type of an element that is not one (`div`). */
export function hostName(type: string): string {
  return isHostType(type) ? HOST_NAME[type] : type;
}

/** The components above a host node, innermost first: its own name, then its parents'. */
export function componentNames(node: Node | undefined): string[] {
  if (!node) return [];
  if (node.trace) return [...node.trace(node.origin)];
  return node.kind === 'element' ? [hostName(node.type)] : [];
}

/**
 * The component stack of a host node, innermost first, in the format of React's
 * (`\n    in Text\n    in Field\n    in Template`), whichever front end built it.
 */
export function componentStack(node: Node | undefined): string {
  return formatStack(componentNames(node));
}

/** Component names, innermost first, as a stack (`\n    in Text\n    in Field`). */
export function formatStack(names: readonly string[]): string {
  return names.map((n) => `\n    in ${n}`).join('');
}

/** The innermost component of a stack: `Text` for `\n    in Text\n    in Field`. */
export function innermostComponent(stack: string | undefined): string | undefined {
  return stack?.split('\n')[1]?.trim().replace(/^in /, '');
}

/** Removes consecutive repeats from a list of names (a component that renders itself once). */
export function dedupe(names: Iterable<string>): string[] {
  const out: string[] = [];
  for (const n of names) if (out[out.length - 1] !== n) out.push(n);
  return out;
}

/**
 * Text as JSX would give it, for front ends whose templates keep the white space around content
 * (Vue, Svelte, Angular): strings that are only the indentation of the template (white space
 * with a line break) are dropped, and white space at the start and the end of an element's
 * content is trimmed. Empty strings go too.
 */
export function trimTemplateText(nodes: Node[]): Node[] {
  const out = nodes.filter((n) => n.kind === 'element' || (n.text !== '' && !/^\s*\n\s*$/.test(n.text)));
  const first = out[0];
  if (first?.kind === 'text') first.text = first.text.trimStart();
  const last = out[out.length - 1];
  if (last?.kind === 'text') last.text = last.text.trimEnd();
  for (const n of out) if (n.kind === 'element') n.children = trimTemplateText(n.children);
  return out.filter((n) => n.kind === 'element' || n.text !== '');
}
