// Angular's render functions in Node.
import { createRenderer } from '@inkoxide/render';

import { angular } from './front-end.ts';

const renderer = createRenderer(angular);

/**
 * A document straight to PDF bytes: no template registry, schema or sample data. A template is
 * any component and its data are its props.
 *
 * ```ts
 * const pdf = await renderToBuffer({ component: InvoiceComponent, inputs: { invoice } });
 * ```
 */
export const renderToBuffer = renderer.renderToBuffer;
/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export const renderToFile = renderer.renderToFile;
/**
 * `renderToBuffer` as a Node stream, to pipe into an HTTP response. The engine writes the whole
 * file at once, so the stream starts when the PDF is ready.
 */
export const renderToStream = renderer.renderToStream;
