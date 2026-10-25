/**
 * What renders do only in Node: write the PDF to a file, or hand it on as a Node stream.
 */
import { writeFile } from 'node:fs/promises';
import { Readable } from 'node:stream';

import type { TreeResult } from '@inkoxide/tree';
import type { z } from 'zod';

import {
  type FrontEnd,
  type GenerateOptions,
  generate,
  renderToBuffer,
  type RenderToBufferOptions,
  renderTree,
  type TemplateLike,
} from './pipeline.ts';
import { RenderError, type RenderOpts, type RenderResult } from './types.ts';

/** The functions a front end exports in Node, for what its users render (`Input`). */
export interface NodeRenderer<Input, C> {
  /** A document to PDF bytes. */
  renderToBuffer(input: Input, opts?: RenderToBufferOptions): Promise<Uint8Array>;
  /** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
  renderToFile(input: Input, path: string, opts?: RenderToBufferOptions): Promise<void>;
  /**
   * `renderToBuffer` as a Node stream, to pipe into an HTTP response. The engine writes the whole
   * file at once, so the stream starts when the PDF is ready, and a render that fails rejects the
   * promise instead of erroring the stream.
   */
  renderToStream(input: Input, opts?: RenderToBufferOptions): Promise<Readable>;
  /** Checks `data` against the template's schema and renders it, with per-stage timings. */
  generate<S extends z.ZodType>(
    t: TemplateLike<S, C>,
    data: unknown,
    opts?: GenerateOptions,
  ): Promise<RenderResult>;
  /** Template + data → tree, without the engine (after `await init()`). */
  renderTree<S extends z.ZodType>(t: TemplateLike<S, C>, data: unknown, opts?: RenderOpts): TreeResult;
}

/** The Node functions of the front end `fe`: `@inkoxide/react` exports React's, and so on. */
export function createRenderer<Input, C>(fe: FrontEnd<Input, C>): NodeRenderer<Input, C> {
  const toBuffer = (input: Input, opts: RenderToBufferOptions = {}) => renderToBuffer(fe, input, opts);
  return {
    renderToBuffer: toBuffer,
    async renderToFile(input, path, opts = {}) {
      const pdf = await toBuffer(input, opts);
      try {
        await writeFile(path, pdf);
      } catch (e) {
        throw new RenderError('INTERNAL', `Cannot write the PDF to ${path}: ${(e as Error).message}`, {
          cause: e,
        });
      }
    },
    async renderToStream(input, opts = {}) {
      const pdf = await toBuffer(input, opts);
      // Buffer chunks, as from a file stream; the view shares the PDF's memory.
      return Readable.from([Buffer.from(pdf.buffer, pdf.byteOffset, pdf.byteLength)], { objectMode: false });
    },
    generate: (t, data, opts) => generate(fe, t, data, opts),
    renderTree: (t, data, opts) => renderTree(fe, t, data, opts),
  };
}
