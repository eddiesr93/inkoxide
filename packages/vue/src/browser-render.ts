// Vue's functions in a browser: the engine in a Web Worker. Each is documented on `BrowserRenderer`
// (@inkoxide/render/browser), so that the front ends say the same.
import { createRenderer } from '@inkoxide/render/browser';

import { vue } from './front-end.ts';

export const browserRenderer = createRenderer(vue);

export const { renderToBuffer, renderToBlob, renderToStream, generate } = browserRenderer;
