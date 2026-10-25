// Vue's render functions in Node: documents, and templates served by id. Each is documented on
// `NodeRenderer` (@inkoxide/render), so that the front ends say the same.
import { createRenderer } from '@inkoxide/render';

import { vue } from './front-end.ts';

export const { renderToBuffer, renderToFile, renderToStream, generate, renderTree } = createRenderer(vue);
