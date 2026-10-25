import {
  createRenderer,
  type GenerateOptions,
  type NodeRenderer,
  type RenderOpts,
  type RenderResult,
} from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
// Vue's render functions in Node: documents, and templates served by id.
import type { Component } from 'vue';
import type { z } from 'zod';

import { vue, type TemplateLike } from './front-end.ts';
import type { Renderable } from './template.ts';

type Renderer = NodeRenderer<Renderable, Component>;

const renderer: Renderer = createRenderer(vue);

/**
 * A document straight to PDF bytes: no template registry, schema or sample data. A template is
 * any component and its data are its props.
 *
 * ```ts
 * const pdf = await renderToBuffer(h(Invoice, { invoice }));
 * ```
 */
export const renderToBuffer: Renderer['renderToBuffer'] = renderer.renderToBuffer;
/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export const renderToFile: Renderer['renderToFile'] = renderer.renderToFile;
/**
 * `renderToBuffer` as a Node stream, to pipe into an HTTP response. The engine writes the whole
 * file at once, so the stream starts when the PDF is ready.
 */
export const renderToStream: Renderer['renderToStream'] = renderer.renderToStream;

/**
 * The whole pipeline: data → zod → Vue → tree (+ Tailwind) → ajv (dev) → engine → PDF, with
 * per-stage timings.
 */
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = renderer.generate;

/** Template + data → tree, without the engine (after `await init()`). */
export const renderTree: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: RenderOpts,
) => TreeResult = renderer.renderTree;
