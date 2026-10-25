// Vue's template functions in Node.
import {
  createRenderer,
  type GenerateOptions,
  type RenderOpts,
  type RenderResult,
  type TemplateLike as AnyTemplate,
} from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { Component } from 'vue';
import type { z } from 'zod';

import { vue } from './front-end.ts';

/**
 * A template for `generate`: a component that takes the prop `data`, the zod schema of that data
 * and the static images it refers to by id.
 */
// oxlint-disable-next-line no-unused-vars -- S types the data the component takes.
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<S, Component>;

const renderer = createRenderer(vue);

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
