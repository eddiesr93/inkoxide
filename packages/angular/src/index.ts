// `@inkoxide/angular`: Angular templates to PDF in Node, with the native engine. One import for templates
// and rendering: the components come with the renderer. test/exports.test.ts pins the list. In a
// browser: `@inkoxide/angular/browser`; templates served by id: `@inkoxide/angular/templates`.
export * from './core.ts';
export {
  countPages,
  type DataIssue,
  type EngineStatus,
  type EngineVersion,
  engineStatus,
  Font,
  type FontRegistration,
  type FontSource,
  type FontSrc,
  type FontWeight,
  init,
  type LocalImageOptions,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
  type RenderToBufferOptions,
  version,
} from '@inkoxide/render';
export { renderToBuffer, renderToFile, renderToStream } from './node-render.ts';
