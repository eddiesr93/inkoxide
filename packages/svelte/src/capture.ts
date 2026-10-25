/**
 * How the components of this package tell Inkoxide what they are. Svelte has no custom renderers
 * yet: each component renders an element of its own (`<ink-view data-ink="3">`), which says where
 * it is among the others, and puts its props, which markup cannot carry (style objects,
 * functions, bytes, snippets), in the table of the render, given by context.
 */
import { type PageInfo, TAGS, TemplateError } from '@inkoxide/tree';
import { getAllContexts, getContext, type Snippet } from 'svelte';

/** The context of the render's table. */
export const CAPTURE = Symbol('inkoxide capture');
/** The context of the page a `render` snippet renders for. */
export const PAGE = Symbol('inkoxide page');

/** What a component put in the table. */
export interface Captured {
  /** The element it renders (`ink-view`, `ink-text-field`). */
  tag: string;
  props: Record<string, unknown>;
  /** Its `render` snippet, and the contexts it renders in. */
  render?: { snippet: Snippet<[PageInfo]>; contexts: Map<unknown, unknown> };
}

/** The table of a render. */
export interface Capture {
  items: Captured[];
}

/**
 * Called by each component as it is created: puts its props in the table and gives the number its
 * element carries (`data-ink`).
 */
export function capture(tag: string, props: Record<string, unknown>, render?: Snippet<[PageInfo]>): number {
  const table = getContext<Capture | undefined>(CAPTURE);
  if (!table) {
    const name = tag.replace(/^ink-/, '');
    throw new TemplateError(
      `<${name}> renders only inside an Inkoxide render: pass the template to renderToBuffer, renderToFile or renderTemplate.`,
      { kind: 'render' },
    );
  }
  if (!TAGS.has(tag)) throw new TemplateError(`<${tag}> is not an Inkoxide component.`);
  table.items.push({ tag, props, render: render && { snippet: render, contexts: getAllContexts() } });
  return table.items.length - 1;
}
