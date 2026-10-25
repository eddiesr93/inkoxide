import type { BrowserRenderOptions } from '@inkoxide/render/browser';
import type { Component, Snippet } from 'svelte';
import type { HTMLAnchorAttributes } from 'svelte/elements';

import type { PdfState } from '../helpers.svelte.ts';
import type { Renderable } from '../template.ts';

/** The props of `<PdfDownloadLink>`. */
interface Props extends Omit<HTMLAnchorAttributes, 'href' | 'download' | 'children'> {
  /** The document to render. */
  document: Renderable;
  /** The name the file is saved under. Default `document.pdf`. */
  fileName?: string;
  options?: BrowserRenderOptions;
  /** What the link shows, from the render's state. */
  children?: Snippet<[PdfState]>;
}

/** A link that downloads `document` as a PDF, once it has rendered. */
declare const PdfDownloadLink: Component<Props>;
export default PdfDownloadLink;
