// The Angular front end without the renderer: templates to document trees, and the types of the
// elements' props. Templates use the elements (`<ink-view>`, `<ink-text>`, `<ink-path>`) with
// CUSTOM_ELEMENTS_SCHEMA: Angular passes their attributes and property bindings on as props.
export type * from '@inkoxide/tree/props';
export {
  type CanvasGradient,
  type CanvasTextOptions,
  type EmojiSource,
  facturX,
  type FacturXOptions,
  type FacturXProfile,
  facturXRelationship,
  type Located,
  PAGE_SIZES,
  type Painter,
  StyleSheet,
  TemplateError,
  type TemplateErrorKind,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/tree';
export { injectPage } from './page.ts';
export { INK_PAGE } from './renderer.ts';
export { type Renderable, renderTemplate, renderToTree } from './template.ts';
