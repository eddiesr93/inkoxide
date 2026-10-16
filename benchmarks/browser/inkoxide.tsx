// @inkoxide/render/browser: the engine as WebAssembly, rendering in its Web Worker.
import * as ink from '@inkoxide/render/browser';

import { Invoice } from '../documents/invoice-inkoxide.tsx';
import { Report } from '../documents/report.tsx';
import invoice from '../invoice.json';
import { measure } from './measure.ts';

const logo = new Uint8Array(await (await fetch(new URL('../logo.jpg', import.meta.url))).arrayBuffer());
// As in the server benchmark: no development checks of the tree.
const options = { validate: false };

await measure({
  invoice: () => ink.renderToBuffer(<Invoice logo={logo} {...invoice} />, options),
  report: () => ink.renderToBuffer(<Report />, options),
});
