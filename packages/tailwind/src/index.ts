import type { TwStyle } from '@inkoxide/style';

import { Lru } from './cache.ts';
import { DEFAULT_THEME_CSS, TailwindCompiler } from './compile.ts';
import { mapClasses, splitClasses, type UnsupportedUtilityError } from './parse.ts';

export { DEFAULT_THEME_CSS, TailwindCompiler } from './compile.ts';
export { UnsupportedUtilityError } from './parse.ts';
export { SUPPORTED, type SupportedFamily, UNSUPPORTED } from './supported.ts';
// The style values it shares with @inkoxide/react, from @inkoxide/style.
export {
  BORDER_STYLES,
  type BorderStyle,
  type Color,
  type ColorValue,
  type DecorationStyle,
  type Direction,
  type LenPercent,
  type Len,
  type LetterSpacing,
  type LineHeight,
  OBJECT_FITS,
  type ObjectFit,
  type ObjectPosition,
  PT_PER_PX,
  PT_PER_REM,
  PT_PER_UNIT,
  parseColor,
  parseFlex,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  round,
  splitTopLevel,
  TEXT_INHERITED,
  TEXT_ONLY,
  type TextAlign,
  type TextDecoration,
  type TextTransform,
  type TransformOp,
  type TwKey,
  type TwStyle,
  UnsupportedValueError,
  type WhiteSpace,
} from '@inkoxide/style';

export interface TailwindOptions {
  /** Theme CSS (`@import "tailwindcss/theme"; @theme { … }`). Defaults to the bundled theme. */
  css?: string;
  /** Directory that `@import`s in `css` resolve from. */
  base?: string;
  /**
   * Throw on unsupported classes (default outside production). When false, the class is
   * logged once and ignored.
   */
  strict?: boolean;
  /** LRU size for class strings (default 5000). */
  cacheSize?: number;
}

let compiler: TailwindCompiler | undefined;
/** The theme load asked for last, until it settles: a later theme replaces an earlier one. */
let loading: Promise<TailwindCompiler> | undefined;
let strict = process.env.NODE_ENV !== 'production';
let cache = new Lru<TwStyle>(5000);
const warned = new Set<string>();
const EMPTY: TwStyle = Object.freeze({}) as TwStyle;

/**
 * Loads Tailwind's design system once per process. Safe to call repeatedly. A call without `css`
 * or `base` keeps the theme loaded or being loaded, so it can run alongside one that sets a theme.
 */
export async function initTailwind(opts: TailwindOptions = {}): Promise<void> {
  if (opts.strict !== undefined) setTailwindStrict(opts.strict);
  if (opts.cacheSize !== undefined) cache = new Lru<TwStyle>(opts.cacheSize);
  if (opts.css !== undefined || opts.base !== undefined) {
    loading = TailwindCompiler.load(opts.css ?? DEFAULT_THEME_CSS, opts.base);
  } else if (!compiler) {
    loading ??= TailwindCompiler.load();
  }
  // Wait for the theme asked for last, including one asked for while this call waits.
  while (loading) {
    const load = loading;
    try {
      const loaded = await load;
      if (loading !== load) continue;
      compiler = loaded;
      loading = undefined;
      cache.clear();
    } catch (e) {
      if (loading === load) loading = undefined;
      throw e;
    }
  }
}

export function isTailwindReady(): boolean {
  return compiler !== undefined;
}

function report(e: UnsupportedUtilityError): void {
  if (strict) throw e;
  if (!warned.has(e.className)) {
    warned.add(e.className);
    console.warn(`[inkoxide/tailwind] ignoring ${e.message}`);
  }
}

/**
 * The engine style for a class string. Synchronous; call `initTailwind()` once first.
 * Results are cached by the normalised string, so the hot path is one map lookup. Do not
 * mutate the returned object.
 */
export function tw(className: string | undefined | null | false): TwStyle {
  if (!className) return EMPTY;
  const hit = cache.get(className);
  if (hit) return hit;
  if (!compiler) throw new Error('Tailwind is not initialised: await initTailwind() at startup');
  const classes = splitClasses(className);
  const key = classes.join(' ');
  let style = key === className ? undefined : cache.get(key);
  if (!style) {
    style = classes.length === 0 ? EMPTY : mapClasses(compiler, classes, report);
    cache.set(key, style);
  }
  cache.set(className, style);
  return style;
}

/** Pre-computes the styles of known class strings (e.g. at server start). */
tw.warm = (classNames: Iterable<string>): void => {
  for (const c of classNames) tw(c);
};

/** Cache statistics, for diagnostics and tests. */
tw.cacheSize = (): number => cache.size;

/** Switches strict mode at runtime (for tests). */
export function setTailwindStrict(value: boolean): void {
  // Cached styles skipped the classes they could not map, so check them again.
  if (value !== strict) cache.clear();
  strict = value;
}
