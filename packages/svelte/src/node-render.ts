// Svelte's render functions in Node: the templates render with `svelte/server`.
import { createRenderer, type NodeRenderer } from '@inkoxide/render';
import type { Component } from 'svelte';

import { svelteFrontEnd } from './front-end.ts';
import { renderTemplate } from './server.ts';
import type { Renderable } from './template.ts';

// oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
type Renderer = NodeRenderer<Renderable, Component<any>>;

const renderer: Renderer = createRenderer(svelteFrontEnd(renderTemplate));

/**
 * A document straight to PDF bytes: no template registry, schema or sample data. A template is
 * any component and its data are its props.
 *
 * ```ts
 * const pdf = await renderToBuffer({ component: Invoice, props: { invoice } });
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
