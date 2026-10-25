<script lang="ts">
  // A link that downloads `document` as a PDF, once it has rendered. It renders when it mounts and
  // when `document` changes. Its children snippet gets the render's state.
  import type { BrowserRenderOptions } from '@inkoxide/render/browser';
  import type { Snippet } from 'svelte';
  import type { HTMLAnchorAttributes } from 'svelte/elements';

  import { createPdf, type PdfState } from '../helpers.svelte.ts';
  import type { Renderable } from '../template.ts';

  interface Props extends Omit<HTMLAnchorAttributes, 'href' | 'download' | 'children'> {
    document: Renderable;
    fileName?: string;
    options?: BrowserRenderOptions;
    children?: Snippet<[PdfState]>;
  }

  let { document, fileName = 'document.pdf', options, children, ...rest }: Props = $props();
  const pdf = createPdf(
    () => document,
    () => options,
  );
</script>

<a {...rest} href={pdf.url ?? undefined} download={fileName} aria-disabled={pdf.url === null}>{@render children?.(pdf)}</a>
