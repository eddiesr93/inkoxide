import type { FrontEnd, TemplateLike as AnyTemplate } from '@inkoxide/render';
import type { TemplateFunctions } from '@inkoxide/tree';
import type { Component } from 'svelte';
import type { z } from 'zod';

import { componentName, type Renderable } from './template.ts';

/** A template component: it takes its data in the prop `data`. */
// oxlint-disable-next-line typescript/no-explicit-any -- whatever data the template takes.
export type TemplateComponent = Component<any>;

/**
 * A template for `generate`: a component that takes the prop `data`, the zod schema of that data
 * and the static images it refers to by id.
 */
// oxlint-disable-next-line no-unused-vars -- S types the data the component takes.
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<S, TemplateComponent>;

/** Svelte as a front end of `@inkoxide/render`, with the template functions of where it runs. */
export function svelteFrontEnd({
  renderTemplate,
}: TemplateFunctions<Renderable>): FrontEnd<Renderable, TemplateComponent> {
  return {
    renderTemplate,
    withData: (component, data) => ({ component, props: { data } }),
    componentName,
  };
}
