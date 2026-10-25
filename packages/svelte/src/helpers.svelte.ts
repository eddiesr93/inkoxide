/**
 * The Svelte helper of the browser entry: `createPdf`, which renders a document to a PDF blob and
 * its URL, again whenever the document changes. `PdfViewer`, `PdfDownloadLink` and
 * `BlobProvider` are built on it.
 */
import type { BrowserRenderOptions } from '@inkoxide/render/browser';
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
  const x = a.props ?? {};
  const y = b.props ?? {};
  const keys = Object.keys(x);
  return keys.length === Object.keys(y).length && keys.every((k) => Object.is(x[k], y[k]));
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
  let blob = $state<Blob | null>(null);
  let url = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<unknown>(null);
  let last: Renderable | undefined;
  let running: AbortController | undefined;
  let shown: string | null = null;

  function update(next?: Renderable): void {
    if (next !== undefined) last = next;
    if (last === undefined) return;
    running?.abort();
    const controller = new AbortController();
    running = controller;
    const { signal } = controller;
    loading = true;
    browserRenderer.renderToBlob(last, { ...untrack(opts), signal }).then(
      (pdf) => {
        if (signal.aborted) return;
        if (shown) URL.revokeObjectURL(shown);
        shown = URL.createObjectURL(pdf);
        blob = pdf;
        url = shown;
        loading = false;
        error = null;
      },
      (e: unknown) => {
        if (signal.aborted) return;
        loading = false;
        error = e;
      },
    );
  }

  $effect.pre(() => {
    const next = document();
    untrack(() => {
      if (next === undefined || (last !== undefined && sameDocument(last, next))) return;
      update(next);
    });
  });
  onDestroy(() => {
    running?.abort();
    if (shown) URL.revokeObjectURL(shown);
    shown = null;
  });

  return {
    get blob() {
      return blob;
    },
    get url() {
      return url;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    update,
  };
}
