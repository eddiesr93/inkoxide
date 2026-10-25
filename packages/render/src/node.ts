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

/**
 * The functions a front end exports in Node, for what its users render (`Input`). The front ends
 * export them as they are, so their documentation is this.
 */
export interface NodeRenderer<Input, Component> {
  /**
   * A document straight to PDF bytes: no template registry, schema or sample data. A template is
   * any component, and its data are its props.
   */
  renderToBuffer(input: Input, opts?: RenderToBufferOptions): Promise<Uint8Array>;
  /** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
  renderToFile(input: Input, path: string, opts?: RenderToBufferOptions): Promise<void>;
  /**
   * `renderToBuffer` as a Node stream, to pipe into an HTTP response. The engine writes the whole
   * file at once, so the stream starts when the PDF is ready, and a render that fails rejects the
   * promise instead of erroring the stream.
   */
  renderToStream(input: Input, opts?: RenderToBufferOptions): Promise<Readable>;
  /**
   * The whole pipeline: data → zod → the template → tree (+ Tailwind) → ajv (dev) → engine → PDF,
   * with per-stage timings.
   */
  generate<S extends z.ZodType>(
    t: TemplateLike<S, Component>,
    data: unknown,
    opts?: GenerateOptions,
  ): Promise<RenderResult>;
  /**
   * Template + data → tree, without the engine (for tree snapshots and previews). `renderSettled`
   * renders it; pass both the same `opts`. It is synchronous, so it cannot load what `init` loads:
   * before `init` has finished, or while fonts added with `Font.register` are not loaded, it
   * throws an `INTERNAL` RenderError that says to call `init`.
   */
  renderTree<S extends z.ZodType>(
    t: TemplateLike<S, Component>,
    data: unknown,
    opts?: RenderOpts,
  ): TreeResult;
}

/** The Node functions of the front end `fe`: `@inkoxide/react` exports React's, and so on. */
export function createRenderer<Input, Component>(
  fe: FrontEnd<Input, Component>,
): NodeRenderer<Input, Component> {
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
