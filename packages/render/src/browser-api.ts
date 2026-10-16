/**
 * What the browser entry adds to the shared pipeline: `init` with the host's options, a render
 * that can be stopped, and the PDF as a `Blob`.
 */
import type { ReactElement } from 'react';

import { type BrowserOptions, configureBrowser } from './browser-host.ts';
import type { EngineStatus } from './engine.ts';
import { init as start, renderToBuffer as render, type RenderToBufferOptions } from './pipeline.ts';
import type { Abortable } from './types.ts';

export { type BrowserOptions, clearCaches } from './browser-host.ts';

/** `renderToBuffer`'s options, and a `signal` or `timeoutMs` that stops the render by ending its worker. */
export interface BrowserRenderOptions extends RenderToBufferOptions, Abortable {}

/**
 * Starts the engine with `opts` (where its module and the bundled assets are, and whether it runs
 * in a worker), and loads Tailwind when it is installed and the fonts added with `Font.register`.
 * Every render calls it; only the options of the first call count.
 */
export function init(opts?: BrowserOptions): Promise<EngineStatus> {
  configureBrowser(opts);
  return start();
}

/**
 * A `<Document>` element to PDF bytes, rendered in a Web Worker: the main entry's, with options
 * that stop it. A `signal` that aborts, or `timeoutMs` that passes, ends the worker (the next
 * render starts a new one) and rejects with the signal's reason.
 */
export const renderToBuffer: (element: ReactElement, opts?: BrowserRenderOptions) => Promise<Uint8Array> =
  render;

/** `renderToBuffer`, as a `Blob` of type `application/pdf`, for a link or a viewer. */
export async function renderToBlob(element: ReactElement, opts: BrowserRenderOptions = {}): Promise<Blob> {
  const pdf = await render(element, opts);
  return new Blob([pdf as Uint8Array<ArrayBuffer>], { type: 'application/pdf' });
}
