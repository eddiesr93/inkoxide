/**
 * The Svelte helper of the browser entry: `createPdf`, which renders a document to a PDF blob and
 * its URL, again whenever the document changes. `PdfViewer`, `PdfDownloadLink` and
 * `BlobProvider` are built on it.
 */
import { type BrowserRenderOptions, PdfRun, type PdfSnapshot, sameProps } from '@inkoxide/render/browser';
import { onDestroy, untrack } from 'svelte';

import { browserRenderer } from './browser-render.ts';
import type { Renderable } from './template.ts';

export interface PdfState {
  /** The PDF of the last render that finished, or null. */
  readonly blob: Blob | null;
  /** An object URL of `blob`, revoked when a new one replaces it and when the component is destroyed; or null. */
  readonly url: string | null;
  /** Whether a render is running. */
  readonly loading: boolean;
  /** Why the last render failed (a `RenderError`, usually), or null. */
  readonly error: unknown;
  /** Renders again: `document` when given, otherwise the last one. */
  update(document?: Renderable): void;
}

/** Whether two documents render the same: the same component with the same prop values. */
function sameDocument(a: Renderable, b: Renderable): boolean {
  if (a === b) return true;
  if (typeof a === 'function' || typeof b === 'function' || a.component !== b.component) return false;
  return sameProps(a.props, b.props);
}

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
  let state = $state<PdfSnapshot>({ blob: null, url: null, loading: false, error: null });
  const run = new PdfRun<Renderable>(
    browserRenderer.renderToBlob,
    (next) => (state = next),
    () => untrack(opts),
  );

  $effect.pre(() => {
    const next = document();
    untrack(() => {
      const last = run.document;
      if (next === undefined || (last !== undefined && sameDocument(last, next))) return;
      run.update(next);
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
