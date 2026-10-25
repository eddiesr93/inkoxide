/**
 * The Svelte helper of the browser entry: `createPdf`, which renders a document to a PDF blob and
 * its URL, again whenever the document changes. `PdfViewer`, `PdfDownloadLink` and
 * `BlobProvider` are built on it.
 */
import {
  type BrowserRenderOptions,
  PdfRun,
  type PdfSnapshot,
  type PdfState as RenderState,
} from '@inkoxide/render/browser';
import { onDestroy, untrack } from 'svelte';

import { browserRenderer } from './browser-render.ts';
import type { Renderable } from './template.ts';

/** What `createPdf` gives: the render's state, read as it changes, and `update()`. */
export type PdfState = Readonly<RenderState<Renderable>>;

/**
 * Renders the document `document()` gives to a PDF in the engine's worker, at once and whenever
 * what it reads changes (`createPdf(() => ({ component: Invoice, props: { invoice } }))`), and
 * again on `update()`. A render that starts stops the one still running; destroying the component
 * stops it too. Call it while a component is created.
 */
export function createPdf(
  document: () => Renderable | undefined,
  opts: () => BrowserRenderOptions | undefined = () => undefined,
): PdfState {
  const run = new PdfRun<Renderable>(
    browserRenderer.renderToBlob,
    (next) => (state = next),
    () => untrack(opts),
  );
  let state = $state<PdfSnapshot>(run.snapshot);

  $effect.pre(() => {
    const next = document();
    untrack(() => {
      run.follow(next);
    });
  });
  onDestroy(() => run.stop());

  return {
    get blob() {
      return state.blob;
    },
    get url() {
      return state.url;
    },
    get loading() {
      return state.loading;
    },
    get error() {
      return state.error;
    },
    update: (next) => run.update(next),
  };
}
