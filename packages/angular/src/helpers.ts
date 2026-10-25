import { DestroyRef, effect, inject, isSignal, type Signal, signal, untracked } from '@angular/core';
import { DomSanitizer, type SafeResourceUrl } from '@angular/platform-browser';
/**
 * The Angular helper of the browser entry: `injectPdf`, which renders a document to a PDF blob and
 * its URL, again whenever the document changes, as signals a template binds to an `<iframe>` or a
 * link.
 */
import type { BrowserRenderOptions } from '@inkoxide/render/browser';

import { browserRenderer } from './browser-render.ts';
import type { Renderable } from './template.ts';

export interface InjectPdfResult {
  /** The PDF of the last render that finished, or null. */
  blob: Signal<Blob | null>;
  /** An object URL of `blob`, revoked when a new one replaces it and when the injector is destroyed; or null. */
  url: Signal<string | null>;
  /** `url`, trusted for `<iframe [src]>`; or null. */
  safeUrl: Signal<SafeResourceUrl | null>;
  /** Whether a render is running. */
  loading: Signal<boolean>;
  /** Why the last render failed (a `RenderError`, usually), or null. */
  error: Signal<unknown>;
  /** Renders again: `document` when given, otherwise the last one. */
  update(document?: Renderable): void;
}

/** Whether two documents render the same: the same component with the same input values. */
function sameDocument(a: Renderable, b: Renderable): boolean {
  if (a === b) return true;
  if (typeof a === 'function' || typeof b === 'function' || a.component !== b.component) return false;
  const x = a.inputs ?? {};
  const y = b.inputs ?? {};
  const keys = Object.keys(x);
  return keys.length === Object.keys(y).length && keys.every((k) => Object.is(x[k], y[k]));
}

/**
 * Renders `document` to a PDF in the engine's worker, at once and whenever it changes (a signal
 * or a function that reads signals: `injectPdf(() => ({ component: Invoice, inputs: { invoice:
 * this.invoice() } }))`), and again on `update()`. A render that starts stops the one still
 * running; destroying the injector stops it too. Call it in an injection context (a constructor,
 * a field initializer).
 */
export function injectPdf(
  document?: Renderable | Signal<Renderable | undefined> | (() => Renderable | undefined),
  opts: BrowserRenderOptions | (() => BrowserRenderOptions | undefined) = {},
): InjectPdfResult {
  const sanitizer = inject(DomSanitizer);
  const blob = signal<Blob | null>(null);
  const url = signal<string | null>(null);
  const safeUrl = signal<SafeResourceUrl | null>(null);
  const loading = signal(false);
  const error = signal<unknown>(null);
  let last: Renderable | undefined;
  let running: AbortController | undefined;
  const options = (): BrowserRenderOptions | undefined => (typeof opts === 'function' ? opts() : opts);

  const update = (next?: Renderable): void => {
    if (next !== undefined) last = next;
    if (last === undefined) return;
    running?.abort();
    const controller = new AbortController();
    running = controller;
    const { signal: aborted } = controller;
    loading.set(true);
    browserRenderer.renderToBlob(last, { ...untracked(options), signal: aborted }).then(
      (pdf) => {
        if (aborted.aborted) return;
        const old = url();
        if (old) URL.revokeObjectURL(old);
        const made = URL.createObjectURL(pdf);
        blob.set(pdf);
        url.set(made);
        safeUrl.set(sanitizer.bypassSecurityTrustResourceUrl(made));
        loading.set(false);
        error.set(null);
      },
      (e: unknown) => {
        if (aborted.aborted) return;
        loading.set(false);
        error.set(e);
      },
    );
  };

  const read = (): Renderable | undefined =>
    isSignal(document)
      ? document()
      : typeof document === 'function' && !('ɵcmp' in document)
        ? (document as () => Renderable | undefined)()
        : (document as Renderable | undefined);

  effect(() => {
    const next = read();
    untracked(() => {
      if (next === undefined || (last !== undefined && sameDocument(last, next))) return;
      update(next);
    });
  });
  inject(DestroyRef).onDestroy(() => {
    running?.abort();
    const old = url();
    if (old) URL.revokeObjectURL(old);
    url.set(null);
    safeUrl.set(null);
  });

  return {
    blob: blob.asReadonly(),
    url: url.asReadonly(),
    safeUrl: safeUrl.asReadonly(),
    loading: loading.asReadonly(),
    error: error.asReadonly(),
    update,
  };
}
