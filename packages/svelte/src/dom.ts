// Renders in a browser: `mount` builds the template in an element of no document, which is read
// back, then unmounted.
import { flushSync, mount, unmount } from 'svelte';

import { readDom } from './markup.ts';
import { type Mounter, svelteTemplates } from './template.ts';

const domMounter: Mounter = (component, props, context, table) => {
  const target = document.createElement('div');
  const app = mount(component, { target, props, context });
  try {
    flushSync();
    return readDom(target.childNodes, table);
  } finally {
    void unmount(app);
  }
};

export const templates = svelteTemplates(domMounter);
export const { renderTemplate, renderToTree } = templates;
