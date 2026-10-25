// React's render functions in Node.
import { createRenderer, type NodeRenderer } from '@inkoxide/render';
import type { ComponentType, ReactElement } from 'react';

import { react } from './front-end.ts';

type Renderer = NodeRenderer<ReactElement, ComponentType<{ data: never }>>;

const renderer: Renderer = createRenderer(react);

/**
 * A `<Document>` element straight to PDF bytes: no template registry, schema or sample data. A
 * template is any component and its data are its props.
 *
 * ```tsx
 * const pdf = await renderToBuffer(<Invoice invoice={invoice} />);
 * ```
 */
export const renderToBuffer: Renderer['renderToBuffer'] = renderer.renderToBuffer;
/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export const renderToFile: Renderer['renderToFile'] = renderer.renderToFile;
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
export const renderToStream: Renderer['renderToStream'] = renderer.renderToStream;
