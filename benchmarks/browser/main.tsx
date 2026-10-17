// @inkoxide/render/browser: the engine as WebAssembly, rendering in its Web Worker.
import * as ink from '@inkoxide/render/browser';

import { Invoice } from '../documents/invoice.tsx';
import { Report } from '../documents/report.tsx';
import invoice from '../invoice.json';
import { measure } from './measure.ts';

// One thread unless the page asks for more: the page is cross-origin isolated, where the default
// would be a pool.
await ink.init({ threads: Number(new URLSearchParams(location.search).get('threads') ?? 1) });

const logo = new Uint8Array(await (await fetch(new URL('../logo.jpg', import.meta.url))).arrayBuffer());
// As in the server benchmark: no development checks of the tree.
const options = { validate: false };

await measure({
  invoice: () => ink.renderToBuffer(<Invoice logo={logo} {...invoice} />, options),
  report: () => ink.renderToBuffer(<Report />, options),
});
