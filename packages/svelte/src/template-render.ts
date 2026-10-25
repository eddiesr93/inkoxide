// Svelte's template functions in Node.
import {
  createRenderer,
  type GenerateOptions,
  type RenderOpts,
  type RenderResult,
  type TemplateLike as AnyTemplate,
} from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { Component } from 'svelte';
import type { z } from 'zod';

import { svelteFrontEnd } from './front-end.ts';
import { renderTemplate } from './server.ts';

/**
 * A template for `generate`: a component that takes the prop `data`, the zod schema of that data
 * and the static images it refers to by id.
 */
// oxlint-disable-next-line typescript/no-explicit-any -- the component takes the data in `data`.
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<S, Component<any>>;

const renderer = createRenderer(svelteFrontEnd(renderTemplate));

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
