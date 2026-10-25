// `@inkoxide/tree`: what every front end shares. The components' host types and props, where each
// may go, and the conversion of the host nodes a front end renders (React, Vue, Svelte, Angular)
// into the engine's document tree, with fonts, images, Tailwind classes and errors that name the
// component at fault.

export type { CanvasGradient, CanvasTextOptions, DrawingFont, Painter } from './canvas.ts';
export * from './components/index.ts';
export { TemplateError, type TemplateErrorInit, type TemplateErrorKind } from './errors.ts';
export { facturX, type FacturXOptions, type FacturXProfile, facturXRelationship } from './facturx.ts';
export * from './fonts/index.ts';
export { familyKey } from './fonts/registry.ts';
export {
  type CommentNode,
  type Container,
  componentNames,
  componentStack,
  formatStack,
  hostName,
  innermostComponent,
  type Instance,
  isHostType,
  type Node,
  type TextInstance,
  type Trace,
  trimTemplateText,
  withoutRepeats,
} from './nodes.ts';
export {
  checkElement,
  checkText,
  checkTree,
  childContext,
  type HostContext,
  rootContext,
} from './placement.ts';
export { loadTailwind } from './tailwind.ts';
export { finishTemplate, templateError, templateFunctions, type TemplateFunctions } from './template.ts';
export type { EncryptionOptions, OutputOptions, PdfALevel, PdfUaPart } from './tree/build.ts';
export {
  type ImageSource,
  imageSource,
  isImagePath,
  type PendingImage,
  type RemoteImagePolicy,
  remoteAllowed,
  remoteRefused,
} from './tree/images.ts';
export { parsePath } from './tree/locate.ts';
export { PAGE_SIZES } from './tree/page-sizes.ts';
export {
  stretchOf as fontStretchValue,
  variationsOf as fontVariationsValue,
  weightOf as fontWeightValue,
} from './tree/style.ts';
export { type SchemaProblem, validateTree } from './tree/validate.ts';
export {
  assertValidTree,
  buildDocument,
  type Located,
  type RenderPage,
  type TreeOptions,
  type TreeResult,
} from './document.ts';
