// Renders on a server: `svelte/server` writes the template's HTML, which is read back.
import { render } from 'svelte/server';

import { readHtml } from './markup.ts';
import { type Mounter, svelteTemplates } from './template.ts';

const serverMounter: Mounter = (component, props, context, table) =>
  readHtml(render(component, { props, context }).body, table);

export const { renderTemplate, renderToTree } = svelteTemplates(serverMounter);
