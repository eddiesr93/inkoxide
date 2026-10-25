// The invoice, written as Vue single-file components, in the browser's PDF viewer and behind a
// download link, rendered in the engine's Web Worker.
import { engineStatus, PdfDownloadLink, PdfViewer, renderToBuffer } from '@inkoxide/vue/browser';
import { createApp, h } from 'vue';

import Invoice from '../../../packages/conformance/vue/Invoice.vue';
import { expose, OPTIONS } from './hooks.ts';
import { sample } from './sample.ts';

const invoice = () => h(Invoice, sample);
expose(() => renderToBuffer(invoice(), OPTIONS), engineStatus);

createApp({
  render: () => [
    h('header', [
      h('h1', 'Inkoxide with Vue'),
      h(
        PdfDownloadLink,
        { id: 'download', document: invoice(), fileName: 'invoice.pdf', options: OPTIONS },
        {
          default: ({ loading, error }: { loading: boolean; error: unknown }) =>
            h(
              'span',
              { 'data-state': error ? 'failed' : loading ? 'loading' : 'ready' },
              error ? String(error) : 'Download',
            ),
        },
      ),
    ]),
    h(PdfViewer, { id: 'viewer', document: invoice(), options: OPTIONS, title: 'The invoice' }),
  ],
}).mount('#root');
