import type { FrontEnd } from '@inkoxide/render';
import { type Component, h, isVNode } from 'vue';

import { type Renderable, renderTemplate } from './template.ts';

/** Vue as a front end of `@inkoxide/render`: VNodes or components in, document trees out. */
export const vue: FrontEnd<Renderable, Component> = {
  tree: renderTemplate,
  withData: (Template, data) => h(Template, { data }),
  name(input) {
    const t = (isVNode(input) ? input.type : input) as { name?: string; __name?: string } | string | null;
    if (typeof t === 'string') return t;
    return (t && (t.name ?? t.__name)) || 'document';
  },
};
