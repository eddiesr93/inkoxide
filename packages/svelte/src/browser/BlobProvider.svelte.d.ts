import type { BrowserRenderOptions } from '@inkoxide/render/browser';
import type { Component, Snippet } from 'svelte';

import type { PdfState } from '../helpers.svelte.ts';
import type { Renderable } from '../template.ts';

/** The props of `<BlobProvider>`. */
interface Props {
  /** The document to render. */
  document: Renderable;
  options?: BrowserRenderOptions;
  /** What it shows: a snippet of the render's state. */
  children: Snippet<[PdfState]>;
}

/** Renders `document` and shows what its children snippet makes of the render's state. */
declare const BlobProvider: Component<Props>;
export default BlobProvider;
