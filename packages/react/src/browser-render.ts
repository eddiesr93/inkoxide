// React's functions in a browser: the engine in a Web Worker. Each is documented on
// `BrowserRenderer` (@inkoxide/render/browser), so that the front ends say the same.
import { createRenderer, type GenerateOptions, type RenderResult } from '@inkoxide/render/browser';
import type { z } from 'zod';

import { react, type TemplateLike } from './front-end.ts';

export const browserRenderer = createRenderer(react);

export const { renderToBuffer, renderToBlob, renderToStream } = browserRenderer;

// A template's component takes the data its schema gives (`TemplateLike<S>`).
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = browserRenderer.generate;
