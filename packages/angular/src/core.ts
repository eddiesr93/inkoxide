// The Angular front end without the renderer: templates to document trees, and the types of the
// elements' props. Templates use the elements (`<ink-view>`, `<ink-text>`, `<ink-path>`) with
// CUSTOM_ELEMENTS_SCHEMA: Angular passes their attributes and property bindings on as props.
export type * from '@inkoxide/tree/props';
export * from '@inkoxide/tree/common';
export { injectPage } from './page.ts';
export { INK_PAGE } from './renderer.ts';
export { type Renderable, renderTemplate, renderToTree } from './template.ts';
