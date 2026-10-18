/**
 * What the browser entry adds to the shared pipeline: `init` with the host's options, a render
 * that can be stopped, and the PDF as a `Blob` or a web stream.
 */
import type { ReactElement } from 'react';
import type { z } from 'zod';

import { type BrowserOptions, configureBrowser } from './browser-host.ts';
import type { EngineStatus } from './engine.ts';
import {
  generate as runTemplate,
  init as start,
  renderToBuffer as render,
  type RenderToBufferOptions,
  type TemplateLike,
} from './pipeline.ts';
import type { Abortable, RenderResult } from './types.ts';

export { type BrowserOptions, clearCaches } from './browser-host.ts';

/** `renderToBuffer`'s options, and a `signal` or `timeoutMs` that stops the render by ending its worker. */
export interface BrowserRenderOptions extends RenderToBufferOptions, Abortable {}

/**
 * Starts the engine with `opts` (where its module and the bundled assets are, and whether it runs
 * in a worker), and loads Tailwind when it is installed and the fonts added with `Font.register`.
 * Every render calls it; only the options of the first call count. When the engine cannot start,
 * the next call starts it again, with its own options.
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

/** The size of the chunks of `renderToStream`'s stream. */
const CHUNK = 64 * 1024;

/**
 * `renderToBuffer`, as a `ReadableStream` of the PDF's bytes: for a `Response` (a service
 * worker's, say), or to write to a file the user picked (`showSaveFilePicker`). As in Node, the
 * engine writes the whole file at once, so the stream starts when the PDF is ready, and a render
 * that fails rejects the promise instead of erroring the stream.
 */
export async function renderToStream(
  element: ReactElement,
  opts: BrowserRenderOptions = {},
): Promise<ReadableStream<Uint8Array>> {
  const pdf = await render(element, opts);
  const size = pdf.byteLength;
  let at = 0;
  return new ReadableStream<Uint8Array>({
    // Views of the PDF, as the reader asks for them.
    pull(controller) {
      // A reader that transferred a chunk's buffer took the whole PDF with it: an error, not a
      // PDF cut short.
      if (pdf.byteLength !== size) {
        controller.error(new TypeError('renderToStream: the PDF was transferred while it was read'));
        return;
      }
      if (at >= size) {
        controller.close();
        return;
      }
      controller.enqueue(pdf.subarray(at, at + CHUNK));
      at += CHUNK;
    },
  });
}

/** The options of `generate`: those of `renderToBuffer`, with a `signal` or `timeoutMs` that stops the render. */
export type GenerateOptions = BrowserRenderOptions;

/**
 * Checks `data` against the template's schema and renders it, in a Web Worker: the templates
 * entry's `generate`, with options that stop it as `renderToBuffer`'s do.
 */
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = runTemplate;
