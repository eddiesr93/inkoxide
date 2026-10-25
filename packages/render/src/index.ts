// `@inkoxide/render`: the engine with its bundled fonts and assets, for every front end. It renders
// documents (`renderDocument`), and `createRenderer` gives a front end (`@inkoxide/react`,
// `@inkoxide/vue`, …) the functions it exports. Every name here is documented in README.md;
// test/exports.test.ts pins the list. Templates served by id: `@inkoxide/render/templates`.
// In Node: the native binding, and the bundled assets from disk.
import './node-host.ts';

export { countPages, type EngineStatus, type EngineVersion, engineStatus, version } from './engine.ts';
export { Font, type FontRegistration, type FontSource, type FontSrc, type FontWeight } from './font.ts';
export { createRenderer, type NodeRenderer } from './node.ts';
export {
  type FrontEnd,
  type GenerateOptions,
  init,
  renderDocument,
  type RenderToBufferOptions,
  type TemplateLike,
} from './pipeline.ts';
export {
  type DataIssue,
  type LocalImageOptions,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
  type RenderOpts,
  type RenderResult,
  type Timings,
} from './types.ts';
