/**
 * What the browser entry adds to the shared pipeline: `init` with the host's options, a render
 * that can be stopped, and the PDF as a `Blob` or a web stream.
 */
import type { TreeResult } from '@inkoxide/tree';
import type { z } from 'zod';

import { type BrowserOptions, configureBrowser } from './browser-host.ts';
import type { EngineStatus } from './engine.ts';
import {
  type FrontEnd,
  generate as runTemplate,
  init as start,
  renderToBuffer as render,
  type RenderToBufferOptions,
  renderTree,
  type TemplateLike,
} from './pipeline.ts';
import type { Abortable, RenderOpts, RenderResult } from './types.ts';

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

/** The options of `generate`: those of `renderToBuffer`, with a `signal` or `timeoutMs` that stops the render. */
export type GenerateOptions = BrowserRenderOptions;

/** The size of the chunks of `renderToStream`'s stream. */
const CHUNK = 64 * 1024;

/** The functions a front end exports in a browser, for what its users render (`Input`). */
export interface BrowserRenderer<Input, C> {
  /**
   * A document to PDF bytes, rendered in a Web Worker. A `signal` that aborts, or `timeoutMs`
   * that passes, ends the worker (the next render starts a new one) and rejects with the signal's
   * reason.
   */
  renderToBuffer(input: Input, opts?: BrowserRenderOptions): Promise<Uint8Array>;
  /** `renderToBuffer`, as a `Blob` of type `application/pdf`, for a link or a viewer. */
  renderToBlob(input: Input, opts?: BrowserRenderOptions): Promise<Blob>;
  /**
   * `renderToBuffer`, as a `ReadableStream` of the PDF's bytes: for a `Response` (a service
   * worker's, say), or to write to a file the user picked (`showSaveFilePicker`). As in Node, the
   * engine writes the whole file at once, so the stream starts when the PDF is ready, and a render
   * that fails rejects the promise instead of erroring the stream.
   */
  renderToStream(input: Input, opts?: BrowserRenderOptions): Promise<ReadableStream<Uint8Array>>;
  /** Checks `data` against the template's schema and renders it, in a Web Worker. */
  generate<S extends z.ZodType>(
    t: TemplateLike<S, C>,
    data: unknown,
    opts?: GenerateOptions,
  ): Promise<RenderResult>;
  /** Template + data → tree, without the engine (after `await init()`). */
  renderTree<S extends z.ZodType>(t: TemplateLike<S, C>, data: unknown, opts?: RenderOpts): TreeResult;
}

/** The browser functions of the front end `fe`: `@inkoxide/react/browser` exports React's, and so on. */
export function createRenderer<Input, C>(fe: FrontEnd<Input, C>): BrowserRenderer<Input, C> {
  const toBuffer = (input: Input, opts: BrowserRenderOptions = {}) => render(fe, input, opts);
  return {
    renderToBuffer: toBuffer,
    async renderToBlob(input, opts = {}) {
      const pdf = await toBuffer(input, opts);
      return new Blob([pdf as Uint8Array<ArrayBuffer>], { type: 'application/pdf' });
    },
    async renderToStream(input, opts = {}) {
      const pdf = await toBuffer(input, opts);
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
    },
    generate: (t, data, opts) => runTemplate(fe, t, data, opts),
    renderTree: (t, data, opts) => renderTree(fe, t, data, opts),
  };
}
