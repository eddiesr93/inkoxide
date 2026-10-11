import { FontRegistry } from './registry.ts';

export { type FontFace, FontRegistry, type ResolvedFont } from './registry.ts';

/** The process-wide registry used when no other is passed. */
export const fonts = new FontRegistry();
