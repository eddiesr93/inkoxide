// `@inkoxide/tree/common`: the names every front end exports as they are (`export * from
// '@inkoxide/tree/common'`), so that their lists cannot drift apart. Each front end's export test
// pins them with the rest.
export type { CanvasGradient, CanvasTextOptions, Painter } from './canvas.ts';
export { PAGE_SIZES } from './tree/page-sizes.ts';
export { StyleSheet } from './components/index.ts';
export { TemplateError, type TemplateErrorKind } from './errors.ts';
export { facturX, type FacturXOptions, type FacturXProfile, facturXRelationship } from './facturx.ts';
export type { EmojiSource } from './fonts/index.ts';
export type { Located, TreeOptions, TreeResult } from './document.ts';
