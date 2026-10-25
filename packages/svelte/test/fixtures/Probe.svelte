<script lang="ts">
  import { createPdf, type PdfState } from '../../src/helpers.svelte.ts';
  import type { Renderable } from '../../src/template.ts';

  let { document, seen }: { document: Renderable; seen: (pdf: PdfState) => void } = $props();
  seen(createPdf(() => document));
</script>
