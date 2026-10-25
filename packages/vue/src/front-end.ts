import type { FrontEnd, TemplateLike as AnyTemplate } from '@inkoxide/render';
import { type Component, h, isVNode } from 'vue';
import type { z } from 'zod';

import { type Renderable, renderTemplate } from './template.ts';

/**
 * A template for `generate`: a component that takes the prop `data`, the zod schema of that data
 * and the static images it refers to by id.
 */
// oxlint-disable-next-line no-unused-vars -- S types the data the component takes.
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<S, Component>;

/** Vue as a front end of `@inkoxide/render`: VNodes or components in, document trees out. */
export const vue: FrontEnd<Renderable, Component> = {
  renderTemplate,
  withData: (Template, data) => h(Template, { data }),
  componentName(input) {
    const t = (isVNode(input) ? input.type : input) as { name?: string; __name?: string } | string | null;
    if (typeof t === 'string') return t;
    return (t && (t.name ?? t.__name)) || 'document';
  },
};
