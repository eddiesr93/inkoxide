import { createRenderer, type GenerateOptions, type RenderResult } from '@inkoxide/render/browser';
import type { z } from 'zod';

import { vue, type TemplateLike } from './front-end.ts';

/** Vue's functions in a browser: the engine in a Web Worker. */
export const browserRenderer = createRenderer(vue);

/**
 * A document to PDF bytes, rendered in a Web Worker. A `signal` that aborts, or `timeoutMs` that
 * passes, ends the worker (the next render starts a new one) and rejects with the signal's reason.
 */
export const renderToBuffer = browserRenderer.renderToBuffer;
/** `renderToBuffer`, as a `Blob` of type `application/pdf`, for a link or a viewer. */
export const renderToBlob = browserRenderer.renderToBlob;
/** `renderToBuffer`, as a `ReadableStream` of the PDF's bytes. */
export const renderToStream = browserRenderer.renderToStream;
/** Checks `data` against the template's schema and renders it, in a Web Worker. */
export const generate: <S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts?: GenerateOptions,
) => Promise<RenderResult> = browserRenderer.generate;
