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
export { generate, renderTree, type TemplateLike } from './template-render.ts';
