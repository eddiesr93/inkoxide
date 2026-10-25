<script lang="ts">
  // `document`, rendered and shown in an `<iframe>`, in the browser's own PDF viewer. It renders
  // when it mounts and when `document` changes, and shows the last PDF until the next one is
  // ready; before the first, its `fallback` snippet.
  import type { BrowserRenderOptions } from '@inkoxide/render/browser';
  import type { Snippet } from 'svelte';
  import type { HTMLIframeAttributes } from 'svelte/elements';

  import { createPdf } from '../helpers.svelte.ts';
  import type { Renderable } from '../template.ts';

  interface Props extends Omit<HTMLIframeAttributes, 'src'> {
    document: Renderable;
    options?: BrowserRenderOptions;
    /** `false` asks the viewer to hide its toolbar (`#toolbar=0`). */
    showToolbar?: boolean;
    fallback?: Snippet;
  }

  let { document, options, showToolbar = true, fallback, title = 'PDF', ...rest }: Props = $props();
  const pdf = createPdf(
    () => document,
    () => options,
  );
</script>

{#if pdf.url === null}
  {@render fallback?.()}
{:else}
  <iframe {...rest} {title} src={showToolbar ? pdf.url : `${pdf.url}#toolbar=0`}></iframe>
{/if}
