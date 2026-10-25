import type { Type } from '@angular/core';
import type { FrontEnd } from '@inkoxide/render';

import { componentName, type Renderable, renderTemplate } from './template.ts';

/** Angular as a front end of `@inkoxide/render`: components and their inputs in, document trees out. */
export const angular: FrontEnd<Renderable, Type<unknown>> = {
  tree: renderTemplate,
  withData: (component, data) => ({ component, inputs: { data } }),
  name: componentName,
};
