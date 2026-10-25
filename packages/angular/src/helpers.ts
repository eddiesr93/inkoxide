// The Angular helper of the browser entry: `injectPdf`, which renders a document to a PDF blob and
// its URL, again whenever the document changes, as signals a template binds to an `<iframe>` or a
// link.
import {
  computed,
  DestroyRef,
  effect,
  inject,
  isSignal,
  type Signal,
  signal,
  untracked,
} from '@angular/core';
import { DomSanitizer, type SafeResourceUrl } from '@angular/platform-browser';
import { type BrowserRenderOptions, PdfRun, type PdfSnapshot } from '@inkoxide/render/browser';

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
  const state = signal<PdfSnapshot>({ blob: null, url: null, loading: false, error: null });
  const options = (): BrowserRenderOptions | undefined => (typeof opts === 'function' ? opts() : opts);
  const run = new PdfRun<Renderable>(
    browserRenderer.renderToBlob,
    (next) => state.set(next),
    () => untracked(options),
  );
  const read = (): Renderable | undefined =>
    isSignal(document)
      ? document()
      : typeof document === 'function' && !('ɵcmp' in document)
        ? (document as () => Renderable | undefined)()
        : (document as Renderable | undefined);

  effect(() => {
    const next = read();
    untracked(() => {
      run.follow(next);
    });
  });
  inject(DestroyRef).onDestroy(() => run.stop());

  return {
    blob: computed(() => state().blob),
    url: computed(() => state().url),
    safeUrl: computed(() => {
      const url = state().url;
      return url === null ? null : sanitizer.bypassSecurityTrustResourceUrl(url);
    }),
    loading: computed(() => state().loading),
    error: computed(() => state().error),
    update: (next) => run.update(next),
  };
}
