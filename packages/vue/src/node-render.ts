// Vue's render functions in Node.
import { createRenderer, type NodeRenderer } from '@inkoxide/render';
import type { Component } from 'vue';

import { vue } from './front-end.ts';
import type { Renderable } from './template.ts';

type Renderer = NodeRenderer<Renderable, Component>;

const renderer: Renderer = createRenderer(vue);

/**
 * A document straight to PDF bytes: no template registry, schema or sample data. A template is
 * any component and its data are its props.
 *
 * ```ts
 * const pdf = await renderToBuffer(h(Invoice, { invoice }));
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
