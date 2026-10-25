import type { Type } from '@angular/core';
// Angular templates to document trees: the component renders once, synchronously, then the host
// nodes are walked into the schema tree.
import type { Document } from '@inkoxide/schema';
import { buildDocument, loadTailwind, type TreeOptions, type TreeResult } from '@inkoxide/tree';

import { mount, renderPage } from './renderer.ts';

/**
 * What Angular renders to a document: a component whose template is the document, with the values
 * of its inputs (`{ component: InvoiceComponent, inputs: { invoice } }`), or a component alone.
 */
export type Renderable = Type<unknown> | { component: Type<unknown>; inputs?: Record<string, unknown> };

const parts = (input: Renderable) => (typeof input === 'function' ? { component: input, inputs: {} } : input);

/**
 * Renders an Angular template component to an Inkoxide document. Synchronous: the component is
 * created and checked once, then the host nodes are walked into the schema tree. Throws
 * TemplateError with a component stack.
 */
export function renderTemplate(input: Renderable, opts: TreeOptions = {}): TreeResult {
  const t0 = performance.now();
  const { component, inputs } = parts(input);
  const container = mount(component, inputs);
  return buildDocument(container, { renderPage }, opts, performance.now() - t0);
}

/**
 * The document tree of an Angular template component (the simple API). Loads Tailwind first when
 * it is installed. A template error is a rejected promise, not a throw.
 */
export async function renderToTree(input: Renderable, opts: TreeOptions = {}): Promise<Document> {
  await loadTailwind();
  return renderTemplate(input, opts).document;
}

/** The name of the component an input renders, for errors. */
export const componentName = (input: Renderable): string => parts(input).component?.name || 'document';
