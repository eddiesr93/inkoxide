// React's template functions in Node.
import { createRenderer, type GenerateOptions, type RenderOpts, type RenderResult } from '@inkoxide/render';
import type { TemplateLike as AnyTemplate } from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { ComponentType } from 'react';
import type { z } from 'zod';

import { react } from './front-end.ts';

/**
 * A template for `generate`: a component that takes `{ data }`, the zod schema of that data and
 * the static images it refers to by id.
 */
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<
  S,
  ComponentType<{ data: z.output<S> }>
>;

const renderer = createRenderer(react);

/**
 * The whole pipeline: data → zod → React → tree (+ Tailwind) → ajv (dev) → engine → PDF, with
 * per-stage timings.
 */
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = renderer.generate as never;

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
) => TreeResult = renderer.renderTree as never;
