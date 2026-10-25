/**
 * How the components of this package tell Inkoxide what they are. Svelte has no custom renderers
 * yet: each component renders an element of its own (`<ink-view data-ink="3">`), whose number is
 * its entry in the props table of the render, given by context, where it puts the props markup
 * cannot carry (style objects, functions, bytes, snippets).
 */
import { type PageInfo, TAGS, TemplateError } from '@inkoxide/tree';
import { getAllContexts, getContext, type Snippet } from 'svelte';

/** The context of the render's props table. */
export const PROPS_TABLE = Symbol('inkoxide props table');
/** The context of the page a `render` snippet renders for. */
export const PAGE = Symbol('inkoxide page');

/** What a component put in the table. */
export interface TableEntry {
  /** The element it renders (`ink-view`, `ink-text-field`). */
  tag: string;
  props: Record<string, unknown>;
  /** Its `render` snippet, and the contexts it renders in. */
  render?: { snippet: Snippet<[PageInfo]>; contexts: Map<unknown, unknown> };
  /** Where it was rendered from: its trace names the components around it. */
  at: Error;
}

/** The props table of a render. */
export interface PropsTable {
  entries: TableEntry[];
}

/**
 * Called by each component as it is created: enters its props in the table and gives the number
 * its element carries (`data-ink`).
 */
export function enter(tag: string, props: Record<string, unknown>, render?: Snippet<[PageInfo]>): number {
  const table = getContext<PropsTable | undefined>(PROPS_TABLE);
  if (!table) {
    const name = tag.replace(/^ink-/, '');
    throw new TemplateError(
      `<${name}> renders only inside an Inkoxide render: pass the template to renderToBuffer, renderToFile or renderTemplate.`,
      { kind: 'render' },
    );
  }
  if (!TAGS.has(tag)) throw new TemplateError(`<${tag}> is not an Inkoxide component.`);
  table.entries.push({
    tag,
    props,
    render: render && { snippet: render, contexts: getAllContexts() },
    at: new Error(),
  });
  return table.entries.length - 1;
}
