// `@inkoxide/react/templates`: React templates served by id, with a zod schema for their data
// (GUIDE.md, "Serving templates"). test/exports.test.ts pins the list.
export {
  type GenerateOptions,
  parseData,
  renderDocument,
  renderSettled,
  type RenderOpts,
  type RenderResult,
  type Timings,
  type TreeResult,
} from '@inkoxide/render/templates';
export type { TemplateLike } from './front-end.ts';
export { generate, renderTree } from './node-render.ts';
