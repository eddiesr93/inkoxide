<script lang="ts">
  // Renders `document` and shows what its children snippet makes of the render's state (`blob`,
  // `url`, `loading`, `error`). It renders when it mounts and when `document` changes.
  import type { BrowserRenderOptions } from '@inkoxide/render/browser';
  import type { Snippet } from 'svelte';

  import { createPdf, type PdfState } from '../helpers.svelte.ts';
  import type { Renderable } from '../template.ts';

  let { document, options, children }: { document: Renderable; options?: BrowserRenderOptions; children: Snippet<[PdfState]> } =
    $props();
  const pdf = createPdf(
    () => document,
    () => options,
  );
</script>

{@render children(pdf)}
