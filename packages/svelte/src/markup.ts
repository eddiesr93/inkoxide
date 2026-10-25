/**
 * Reads what Svelte rendered into the nodes of `@inkoxide/tree`: on a server the HTML that
 * `svelte/server` writes, in a browser the DOM that `mount` builds. Elements are the components'
 * (`<ink-view data-ink="3">`): their props come from the render's table. Any other element is
 * kept, for the placement check to name.
 */
import {
  BOOLEAN_PROPS,
  formatStack,
  hostName,
  type HostType,
  type Instance,
  type Node,
  TAGS,
  TemplateError,
  type Trace,
} from '@inkoxide/tree';

import type { Capture } from './capture.ts';

/** An element's stack: its component, then the elements around it. */
const trace: Trace = (origin) => {
  const names: string[] = [];
  for (let el = origin as Instance | undefined; el; el = el.origin as Instance | undefined)
    names.push(hostName(el.type));
  return names;
};

/** A string's stack: that of the element it is in. */
const textTrace: Trace = (parent) => (parent ? trace(parent) : []);

/** `class` as Svelte takes it (a string, a list, an object of flags) to `className`. */
function classNames(value: unknown): string {
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

/** The props of an element from what its component put in the table. */
function propsOf(table: Capture, tag: string, id: string | null): Record<string, unknown> {
  const item = id === null ? undefined : table.items[Number(id)];
  const props: Record<string, unknown> = {};
  const kind = TAGS.get(tag)?.kind;
  if (kind) props.kind = kind;
  if (!item) return props;
  for (const [name, value] of Object.entries(item.props)) {
    if (value === undefined) continue;
    if (name === 'class') props.className = classNames(value);
    else props[name] = value === '' && BOOLEAN_PROPS.has(name) ? true : value;
  }
  if (item.render) {
    // The builder calls `render` for each page; `renderPage` renders the snippet it carries.
    props.render = Object.assign(() => undefined, {
      snippet: item.render.snippet,
      contexts: item.render.contexts,
    });
  }
  return props;
}

/** An element: a component's, with its props, or another, kept as it is. */
function element(table: Capture, tag: string, id: string | null, parent: Instance | undefined): Instance {
  const known = TAGS.get(tag);
  if (known && (id === null || table.items[Number(id)]?.tag !== tag)) {
    // Not rendered by its component: written in the template, or in markup `{@html}` inserts.
    throw new TemplateError(
      `<${tag}> is rendered by the ${hostName(known.type)} component: write <${hostName(known.type)}>, not the element, and no Inkoxide markup in {@html}.`,
      { kind: 'structure', componentStack: formatStack(trace(parent)) },
    );
  }
  return {
    kind: 'element',
    type: (known?.type ?? tag) as HostType,
    props: known ? propsOf(table, tag, id) : {},
    children: [],
    origin: parent,
    trace: (origin) => [hostName(known?.type ?? tag), ...trace(origin)],
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
export function readHtml(html: string, table: Capture): Node[] {
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
export function readDom(nodes: ArrayLike<DomNode>, table: Capture, parent?: Instance): Node[] {
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
