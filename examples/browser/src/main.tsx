// The page: the invoice in the browser's PDF viewer and a link that downloads it, both rendered
// in the engine's Web Worker; and a few hooks for the tests.
import { PdfDownloadLink, PdfViewer, renderToBuffer } from '@inkoxide/render/browser';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import Invoice, { OPTIONS, sample } from './Invoice.tsx';

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** What the tests call. */
const hooks = {
  /** The SHA-256 of the invoice's PDF, rendered with renderToBuffer. */
  invoiceDigest: async () => sha256(await renderToBuffer(<Invoice {...sample} />, OPTIONS)),
  /**
   * Starts a render and stops it when the page dispatches `inkoxide:stop` (the tests do once the
   * worker has the render); resolves to the reason it rejected with.
   */
  abortedRender: async () => {
    const controller = new AbortController();
    addEventListener('inkoxide:stop', () => controller.abort(new Error('stopped')), { once: true });
    const render = renderToBuffer(<Invoice {...sample} />, { ...OPTIONS, signal: controller.signal });
    return render.then(
      () => 'rendered',
      (e: unknown) => (e instanceof Error ? e.message : String(e)),
    );
  },
};
Object.assign(globalThis, { inkoxide: hooks });

function App() {
  const invoice = <Invoice {...sample} />;
  return (
    <>
      <header>
        <h1>Inkoxide in the browser</h1>
        <PdfDownloadLink id="download" document={invoice} fileName="invoice.pdf" options={OPTIONS}>
          {({ loading, error }) => (
            <span data-state={error ? 'failed' : loading ? 'loading' : 'ready'}>
              {error ? `Failed: ${String(error)}` : loading ? 'Rendering…' : 'Download the PDF'}
            </span>
          )}
        </PdfDownloadLink>
      </header>
      <PdfViewer
        id="viewer"
        document={invoice}
        options={OPTIONS}
        title="The invoice"
        fallback={<p>Rendering…</p>}
      />
    </>
  );
}

createRoot(document.querySelector('#root') as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
