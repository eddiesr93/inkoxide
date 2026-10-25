<script lang="ts">
  import { PdfDownloadLink, PdfViewer } from '@inkoxide/svelte/browser';

  import Invoice from '../../../packages/conformance/svelte/Invoice.svelte';
  import { OPTIONS } from './hooks.ts';
  import { sample } from './sample.ts';

  const document = { component: Invoice, props: sample };
</script>

<header>
  <h1>Inkoxide with Svelte</h1>
  <PdfDownloadLink id="download" {document} fileName="invoice.pdf" options={OPTIONS}>
    {#snippet children(pdf)}
      <span data-state={pdf.error ? 'failed' : pdf.loading ? 'loading' : 'ready'}>{pdf.error ? String(pdf.error) : 'Download'}</span>
    {/snippet}
  </PdfDownloadLink>
</header>
<PdfViewer id="viewer" {document} options={OPTIONS} title="The invoice" />
