// React's render functions in Node: documents, and templates served by id.
import {
  createRenderer,
  type GenerateOptions,
  type NodeRenderer,
  type RenderOpts,
  type RenderResult,
} from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { ReactElement } from 'react';
import type { z } from 'zod';

import { react, type TemplateComponent, type TemplateLike } from './front-end.ts';

type Renderer = NodeRenderer<ReactElement, TemplateComponent>;

const renderer: Renderer = createRenderer(react);

/**
 * A `<Document>` element straight to PDF bytes: no template registry, schema or sample data. A
 * template is any component and its data are its props.
 *
 * ```tsx
 * const pdf = await renderToBuffer(<Invoice invoice={invoice} />);
 * ```
 */
export const renderToBuffer: Renderer['renderToBuffer'] = renderer.renderToBuffer;
/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export const renderToFile: Renderer['renderToFile'] = renderer.renderToFile;
/**
 * `renderToBuffer` as a Node stream, to pipe into an HTTP response:
 *
 * ```tsx
 * (await renderToStream(<Invoice invoice={invoice} />)).pipe(res);
 * ```
 *
 * The engine writes the whole file at once, so the stream starts when the PDF is ready, and a
 * render that fails rejects the promise instead of erroring the stream.
 */
export const renderToStream: Renderer['renderToStream'] = renderer.renderToStream;

/**
 * The whole pipeline: data → zod → React → tree (+ Tailwind) → ajv (dev) → engine → PDF, with
 * per-stage timings.
 */
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = renderer.generate;

/**
 * Template + data → tree, without the engine (for tree snapshots and previews). `renderSettled`
 * renders it; pass both the same `opts`. It is synchronous, so it cannot load what `init` loads:
 * before `init` has finished, or while fonts added with `Font.register` are not loaded, it
 * throws an `INTERNAL` RenderError that says to call `init`.
 */
export const renderTree: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: RenderOpts,
) => TreeResult = renderer.renderTree;
