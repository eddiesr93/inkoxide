// Angular templates to document trees: the component renders once, synchronously, then the host
// nodes are walked into the schema tree.
import type { Type } from '@angular/core';
import { type TemplateFunctions, templateFunctions } from '@inkoxide/tree';
import { componentAndValues } from '@inkoxide/tree/internal';

import { mount, renderPage } from './renderer.ts';

/**
 * What Angular renders to a document: a component whose template is the document, with the values
 * of its inputs (`{ component: InvoiceComponent, inputs: { invoice } }`), or a component alone.
 */
export type Renderable = Type<unknown> | { component: Type<unknown>; inputs?: Record<string, unknown> };

/**
 * Angular's template functions: the component is created with its inputs and checked once,
 * synchronously, into host nodes.
 */
export const { renderTemplate, renderToTree }: TemplateFunctions<Renderable> = templateFunctions(
  (input: Renderable) => {
    const { component, values } = componentAndValues(input, 'inputs');
    return mount(component, values);
  },
  renderPage,
);

/** The name of the component an input renders, for errors. */
export const componentName = (input: Renderable): string =>
  componentAndValues(input, 'inputs').component?.name || 'document';
