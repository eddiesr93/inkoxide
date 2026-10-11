// One import for templates and rendering: the components come with the renderer.
export * from '@inkoxide/react';
export { SCHEMA_VERSION } from '@inkoxide/schema';
export {
  assetsDir,
  countPages,
  type EngineStatus,
  type EngineVersion,
  engineStatus,
  initEngine,
  version,
} from './engine.ts';
export { Font, type FontRegistration, type FontSource, type FontSrc, type FontWeight } from './font.ts';
export { DEFAULT_FACES } from './fonts.ts';
export {
  type GenerateOptions,
  generate,
  init,
  parseData,
  type RenderToBufferOptions,
  renderDocument,
  renderSettled,
  renderToBuffer,
  renderToFile,
  renderTree,
  type TemplateLike,
} from './pipeline.ts';
export * from './types.ts';
