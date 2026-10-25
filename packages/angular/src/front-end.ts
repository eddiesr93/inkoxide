import type { Type } from '@angular/core';
import type { FrontEnd, TemplateLike as AnyTemplate } from '@inkoxide/render';
import type { z } from 'zod';

import { componentName, type Renderable, renderTemplate } from './template.ts';

/**
 * A template for `generate`: a component that takes the input `data`, the zod schema of that data
 * and the static images it refers to by id.
 */
// oxlint-disable-next-line no-unused-vars -- S types the data the component takes.
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<S, Type<unknown>>;

/** Angular as a front end of `@inkoxide/render`: components and their inputs in, document trees out. */
export const angular: FrontEnd<Renderable, Type<unknown>> = {
  renderTemplate,
  withData: (component, data) => ({ component, inputs: { data } }),
  componentName,
};
