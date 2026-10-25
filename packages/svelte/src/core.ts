// The Svelte front end without the renderer: the components, and the types of their props.
export * from './components/index.ts';
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
export { usePage } from './page.ts';
export type { Renderable } from './template.ts';
