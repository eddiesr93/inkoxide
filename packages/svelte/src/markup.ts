/**
 * Reads what Svelte rendered into the nodes of `@inkoxide/tree`: on a server the HTML that
 * `svelte/server` writes, in a browser the DOM that `mount` builds. Elements are the components'
 * (`<ink-view data-ink="3">`): their props come from the render's table. Any other element is
 * kept, for the placement check to name.
 */
import {
  assignProp,
  componentNames,
  formatStack,
  hostName,
  type HostType,
  type Instance,
  type Node,
  TAGS,
  TemplateError,
  type Trace,
} from '@inkoxide/tree';

import { svelteNames } from './stack.ts';
import type { PropsTable, TableEntry } from './table.ts';

/** What an element keeps for its stack: the element it is in, and its table entry. */
interface Origin {
  parent: Instance | undefined;
  entry: TableEntry | undefined;
}

/** The Inkoxide components around an element, from the elements it is in. */
function around(el: Instance | undefined): string[] {
  const names: string[] = [];
  for (let e = el; e; e = (e.origin as Origin).parent) names.push(hostName(e.type));
  return names;
}

/**
 * An element's stack: the components its entry's trace names (the user's among them), else the
 * Inkoxide components around it.
 */
const elementTrace: Trace = (origin) => {
  const { parent, entry } = origin as Origin;
  const named = entry ? svelteNames(entry.at) : [];
  return named.length > 0 ? named : around(parent);
};

/** A string's stack: that of the element it is in. */
const textTrace: Trace = (parent) => (parent ? componentNames(parent as Instance) : []);

/** The props of an element from its entry in the table. */
function propsOf(tag: string, entry: TableEntry | undefined): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  const kind = TAGS.get(tag)?.kind;
  if (kind) props.kind = kind;
  if (!entry) return props;
  for (const [name, value] of Object.entries(entry.props)) assignProp(props, name, value);
  if (entry.render) {
    // The builder calls `render` for each page; `renderPage` renders the snippet it carries.
    props.render = Object.assign(() => undefined, {
      snippet: entry.render.snippet,
      contexts: entry.render.contexts,
    });
  }
  return props;
}

/** An element: a component's, with its props, or another, kept as it is. */
function element(table: PropsTable, tag: string, id: string | null, parent: Instance | undefined): Instance {
  const known = TAGS.get(tag);
  const entry = id === null ? undefined : table.entries[Number(id)];
  if (known && entry?.tag !== tag) {
    // Not rendered by its component: written in the template, or in markup `{@html}` inserts.
    throw new TemplateError(
      `<${tag}> is rendered by the ${hostName(known.type)} component: write <${hostName(known.type)}>, not the element, and no Inkoxide markup in {@html}.`,
      { kind: 'structure', componentStack: formatStack(around(parent)) },
    );
  }
  const type = (known?.type ?? tag) as HostType;
  return {
    kind: 'element',
    type,
    props: known ? propsOf(tag, entry) : {},
    children: [],
    origin: { parent, entry } satisfies Origin,
    trace: known ? elementTrace : (origin) => [hostName(type), ...around((origin as Origin).parent)],
  };
}

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };

function decode(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (all, e: string) => {
    if (e[0] === '#')
      return String.fromCodePoint(
        e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : Number(e.slice(1)),
      );
    return ENTITIES[e.toLowerCase()] ?? all;
  });
}

/** Elements that never have content in HTML: `<br>`, `<img>` and the like, for a template that uses one. */
const VOID = new Set([
  'area',
  'base',
  'br',
  'col',
  'embed',
  'hr',
  'img',
  'input',
  'link',
  'meta',
  'source',
  'track',
  'wbr',
]);

/** The nodes of the HTML `svelte/server` wrote. */
export function readHtml(html: string, table: PropsTable): Node[] {
  const root: Node[] = [];
  const stack: { el: Instance | undefined; children: Node[]; tag: string }[] = [
    { el: undefined, children: root, tag: '' },
  ];
  const token =
    /<!--[\s\S]*?-->|<\/([a-zA-Z][\w-]*)\s*>|<([a-zA-Z][\w-]*)((?:\s+[^\s=>/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*(\/?)>|([^<]+)/g;
  for (const m of html.matchAll(token)) {
    const top = stack[stack.length - 1]!;
    const [, close, open, attrs = '', selfClosing, text] = m;
    if (text !== undefined) {
      top.children.push({ kind: 'text', text: decode(text), origin: top.el, trace: textTrace });
    } else if (open !== undefined) {
      const tag = open.toLowerCase();
      const id = /\sdata-ink\s*=\s*"(\d+)"/.exec(attrs)?.[1] ?? null;
      const el = element(table, tag, id, top.el);
      top.children.push(el);
      if (!selfClosing && !VOID.has(tag)) stack.push({ el, children: el.children, tag });
    } else if (close !== undefined) {
      const tag = close.toLowerCase();
      const at = stack.findLastIndex((s) => s.tag === tag);
      if (at > 0) stack.length = at;
    }
  }
  return root;
}

interface DomNode {
  nodeType: number;
  nodeValue: string | null;
  childNodes: ArrayLike<DomNode>;
  localName?: string;
  getAttribute?(name: string): string | null;
}

/** The nodes of the DOM `mount` built. */
export function readDom(nodes: ArrayLike<DomNode>, table: PropsTable, parent?: Instance): Node[] {
  const out: Node[] = [];
  for (const n of Array.from(nodes)) {
    if (n.nodeType === 3)
      out.push({ kind: 'text', text: n.nodeValue ?? '', origin: parent, trace: textTrace });
    else if (n.nodeType === 1) {
      const tag = (n.localName ?? '').toLowerCase();
      const el = element(table, tag, n.getAttribute?.('data-ink') ?? null, parent);
      el.children = readDom(n.childNodes, table, el);
      out.push(el);
    }
  }
  return out;
}
