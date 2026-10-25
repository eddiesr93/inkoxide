// Svelte's render functions in Node: documents, and templates served by id. The templates
// render with `svelte/server`.
import {
  createRenderer,
  type GenerateOptions,
  type NodeRenderer,
  type RenderOpts,
  type RenderResult,
} from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { z } from 'zod';

import { svelteFrontEnd, type TemplateComponent, type TemplateLike } from './front-end.ts';
import { templates } from './server.ts';
import type { Renderable } from './template.ts';

type Renderer = NodeRenderer<Renderable, TemplateComponent>;

const renderer: Renderer = createRenderer(svelteFrontEnd(templates));

/**
 * A document straight to PDF bytes: no template registry, schema or sample data. A template is
 * any component and its data are its props.
 *
 * ```ts
 * const pdf = await renderToBuffer({ component: Invoice, props: { invoice } });
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
 * The whole pipeline: data → zod → Svelte → tree (+ Tailwind) → ajv (dev) → engine → PDF, with
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
