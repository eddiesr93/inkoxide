/**
 * What renders do only in Node: write the PDF to a file, or hand it on as a Node stream.
 */
import { writeFile } from 'node:fs/promises';
import { Readable } from 'node:stream';

import type { ReactElement } from 'react';

import { renderToBuffer, type RenderToBufferOptions } from './pipeline.ts';
import { RenderError } from './types.ts';

/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export async function renderToFile(
  element: ReactElement,
  path: string,
  opts: RenderToBufferOptions = {},
): Promise<void> {
  const pdf = await renderToBuffer(element, opts);
  try {
    await writeFile(path, pdf);
  } catch (e) {
    throw new RenderError('INTERNAL', `Cannot write the PDF to ${path}: ${(e as Error).message}`, {
      cause: e,
    });
  }
}

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
export async function renderToStream(
  element: ReactElement,
  opts: RenderToBufferOptions = {},
): Promise<Readable> {
  const pdf = await renderToBuffer(element, opts);
  // Buffer chunks, as from a file stream; the view shares the PDF's memory.
  return Readable.from([Buffer.from(pdf.buffer, pdf.byteOffset, pdf.byteLength)], { objectMode: false });
}
