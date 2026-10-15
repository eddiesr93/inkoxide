// The page: renders the invoice in a Web Worker and shows it, and gives the tests a few hooks.
import { init, renderToBlob, renderToBuffer } from '@inkoxide/render/browser';
import { createElement } from 'react';

import Invoice, { OPTIONS, sample } from './Invoice.tsx';

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

const invoice = () => createElement(Invoice, sample);

/** What the tests call. */
const hooks = {
  /** The SHA-256 of the invoice's PDF. */
  invoiceDigest: async () => sha256(await renderToBuffer(invoice(), OPTIONS)),
  /**
   * Starts a render and stops it when the page dispatches `inkoxide:stop` (the tests do once the
   * worker has the render); resolves to the reason it rejected with.
   */
  abortedRender: async () => {
    const controller = new AbortController();
    addEventListener('inkoxide:stop', () => controller.abort(new Error('stopped')), { once: true });
    const render = renderToBuffer(invoice(), { ...OPTIONS, signal: controller.signal });
    return render.then(
      () => 'rendered',
      (e: unknown) => (e instanceof Error ? e.message : String(e)),
    );
  },
};
Object.assign(globalThis, { inkoxide: hooks });

const status = document.querySelector('#status') as HTMLElement;
const viewer = document.querySelector('#viewer') as HTMLIFrameElement;
const download = document.querySelector('#download') as HTMLAnchorElement;

try {
  await init();
  const t = performance.now();
  const blob = await renderToBlob(invoice(), OPTIONS);
  const url = URL.createObjectURL(blob);
  viewer.src = url;
  download.href = url;
  status.textContent = `Rendered in ${Math.round(performance.now() - t)} ms, ${blob.size} bytes.`;
  status.dataset.state = 'ready';
} catch (e) {
  status.textContent = `Failed: ${e instanceof Error ? e.message : String(e)}`;
  status.dataset.state = 'failed';
  throw e;
}
