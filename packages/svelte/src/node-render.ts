// Svelte's render functions in Node: documents, and templates served by id; the templates render
// with `svelte/server`. Each is documented on `NodeRenderer` (@inkoxide/render), so that the front
// ends say the same.
import { createRenderer } from '@inkoxide/render';

import { svelteFrontEnd } from './front-end.ts';
import { templates } from './server.ts';

export const { renderToBuffer, renderToFile, renderToStream, generate, renderTree } = createRenderer(
  svelteFrontEnd(templates),
);
