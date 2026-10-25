// Svelte's functions in a browser: the templates mount in the page, the engine runs in a Web
// Worker. Each is documented on `BrowserRenderer` (@inkoxide/render/browser), so that the front
// ends say the same.
import { createRenderer } from '@inkoxide/render/browser';

import { templates } from './dom.ts';
import { svelteFrontEnd } from './front-end.ts';

export const browserRenderer = createRenderer(svelteFrontEnd(templates));

export const { renderToBuffer, renderToBlob, renderToStream, generate } = browserRenderer;
