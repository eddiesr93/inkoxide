import type { BrowserRenderOptions } from '@inkoxide/render/browser';
import type { Component, Snippet } from 'svelte';
import type { HTMLIframeAttributes } from 'svelte/elements';

import type { Renderable } from '../template.ts';

/** The props of `<PdfViewer>`. */
interface Props extends Omit<HTMLIframeAttributes, 'src'> {
  /** The document to render. */
  document: Renderable;
  options?: BrowserRenderOptions;
  /** `false` asks the viewer to hide its toolbar (`#toolbar=0`). Default `true`. */
  showToolbar?: boolean;
  /** Shown in place of the frame before the first render is ready. */
  fallback?: Snippet;
}

/**
 * `document`, rendered and shown in an `<iframe>`, in the browser's own PDF viewer. It renders when
 * it mounts and when `document` changes, and shows the last PDF until the next one is ready.
 */
declare const PdfViewer: Component<Props>;
export default PdfViewer;
