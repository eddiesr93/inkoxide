import { DEFAULT_THEME_CSS, TailwindCompiler as Compiler } from './compile.ts';
import * as supported from './supported.ts';
import { initSource, type TailwindOptions } from './tw.ts';

export * from './exports.ts';

// Exported before, but not documented: they will be removed from this entry.

/**
 * @deprecated Wraps Tailwind's unstable design-system API, and `tw()` cannot use one you load:
 * set a theme with `initTailwind({ css })`.
 */
export const TailwindCompiler = Compiler;
/** @deprecated Wraps Tailwind's unstable design-system API: set a theme with `initTailwind({ css })`. */
export type TailwindCompiler = Compiler;
/** @deprecated The data of docs/TAILWIND_SUPPORT.md. */
export const SUPPORTED = supported.SUPPORTED;
/** @deprecated The data of docs/TAILWIND_SUPPORT.md. */
export const UNSUPPORTED = supported.UNSUPPORTED;
/** @deprecated The data of docs/TAILWIND_SUPPORT.md. */
export type SupportedFamily = supported.SupportedFamily;

/**
 * Loads Tailwind's design system once per process. Safe to call repeatedly. A call without `css`
 * or `base` keeps the theme loaded or being loaded, so it can run alongside one that sets a theme.
 */
export async function initTailwind(opts: TailwindOptions = {}): Promise<void> {
  const themed = opts.css !== undefined || opts.base !== undefined;
  await initSource(opts, () => Compiler.load(opts.css ?? DEFAULT_THEME_CSS, opts.base), themed);
}
