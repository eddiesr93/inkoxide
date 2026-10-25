// The Vue front end without the renderer: the components, and Vue templates to document trees.
export * from './components.ts';
export { usePage } from './page.ts';
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
  TemplateError,
  type TemplateErrorKind,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/tree';
export { type Renderable, renderTemplate, renderToTree } from './template.ts';
