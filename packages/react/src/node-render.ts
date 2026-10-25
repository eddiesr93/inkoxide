// React's render functions in Node: documents, and templates served by id. Each is documented on
// `NodeRenderer` (@inkoxide/render), so that the front ends say the same.
import { createRenderer, type GenerateOptions, type RenderOpts, type RenderResult } from '@inkoxide/render';
import type { TreeResult } from '@inkoxide/tree';
import type { z } from 'zod';

import { react, type TemplateLike } from './front-end.ts';

const renderer = createRenderer(react);

export const { renderToBuffer, renderToFile, renderToStream } = renderer;

// A template's component takes the data its schema gives (`TemplateLike<S>`).
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = renderer.generate;
export const renderTree: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: RenderOpts,
) => TreeResult = renderer.renderTree;
