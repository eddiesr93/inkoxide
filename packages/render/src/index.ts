// One import for templates and rendering: the components come with the renderer. Every name
// here is documented in GUIDE.md; test/exports.test.ts pins the list. The template registry
// is a separate entry, `@inkoxide/render/templates` (templates.ts).
// In Node: the native binding, and the bundled assets from disk.
import './node-host.ts';

export * from './components.ts';
export { countPages, type EngineStatus, type EngineVersion, engineStatus, version } from './engine.ts';
export { Font, type FontRegistration, type FontSource, type FontSrc, type FontWeight } from './font.ts';
export { renderToFile, renderToStream } from './node.ts';
export { init, type RenderToBufferOptions, renderToBuffer } from './pipeline.ts';
export {
  type DataIssue,
  type LocalImageOptions,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
} from './types.ts';
