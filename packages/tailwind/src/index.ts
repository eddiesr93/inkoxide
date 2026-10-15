import type { FlatStyle } from '@inkoxide/style';

import { Lru } from './cache.ts';
import { DEFAULT_THEME_CSS, TailwindCompiler as Compiler } from './compile.ts';
import { mapClasses, splitClasses, type TwUnits, type UnsupportedUtilityError } from './parse.ts';
import * as supported from './supported.ts';

export { DEFAULT_THEME_CSS } from './compile.ts';
export { type TwUnits, UnsupportedUtilityError } from './parse.ts';
// The style values it shares with @inkoxide/react, from @inkoxide/style.
export {
  BORDER_STYLES,
  type BorderStyle,
  type Color,
  type ColorValue,
  type DecorationStyle,
  type Direction,
  type FlatStyle,
  type FlatStyleKey,
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
  type Units,
  UnsupportedValueError,
  type WhiteSpace,
} from '@inkoxide/style';

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

export interface TailwindOptions {
  /**
   * Theme CSS (`@import "tailwindcss/theme"; @theme { … }`). Defaults to `DEFAULT_THEME_CSS`, the
   * bundled theme; extend that one to keep its font stacks, which name the bundled fonts.
   */
  css?: string;
  /** Directory that `@import`s in `css` resolve from (default: the tailwindcss package). */
  base?: string;
  /**
   * Throw on unsupported classes (default outside production). When false, the class is
   * logged once and ignored.
   */
  strict?: boolean;
  /** LRU size for class strings (default 5000). */
  cacheSize?: number;
}

let compiler: Compiler | undefined;
/** The theme load asked for last, until it settles: a later theme replaces an earlier one. */
let loading: Promise<Compiler> | undefined;
let strict =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.NODE_ENV !==
  'production';
let cache = new Lru<FlatStyle>(5000);
const warned = new Set<string>();
const EMPTY: FlatStyle = Object.freeze({}) as FlatStyle;

/**
 * Loads Tailwind's design system once per process. Safe to call repeatedly. A call without `css`
 * or `base` keeps the theme loaded or being loaded, so it can run alongside one that sets a theme.
 */
export async function initTailwind(opts: TailwindOptions = {}): Promise<void> {
  if (opts.strict !== undefined) setTailwindStrict(opts.strict);
  if (opts.cacheSize !== undefined) cache = new Lru<FlatStyle>(opts.cacheSize);
  if (opts.css !== undefined || opts.base !== undefined) {
    loading = Compiler.load(opts.css ?? DEFAULT_THEME_CSS, opts.base);
  } else if (!compiler) {
    loading ??= Compiler.load();
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
 * The engine style for a class string. Synchronous; call `initTailwind()` once first. `units` are
 * what `em`, `vw` and `vh` in arbitrary values (`w-[2em]`, `h-[10vh]`) resolve against; without
 * them such a value is unsupported. Results are cached by the normalised string (and, for a style
 * that depends on them, by the units), so the hot path is one map lookup. Do not mutate the
 * returned object.
 */
export function tw(className: string | undefined | null | false, units?: TwUnits): FlatStyle {
  if (!className) return EMPTY;
  const hit = cache.get(className);
  if (hit) return hit;
  if (!compiler) throw new Error('Tailwind is not initialised: await initTailwind() at startup');
  const classes = splitClasses(className);
  const key = classes.join(' ');
  let style = key === className ? undefined : cache.get(key);
  if (!style) {
    // A style that depends on the units is cached under them, never under the classes alone.
    const unitsKey = units
      ? `${key}\n${units.parentFontSize} ${units.fontSize} ${units.page.width} ${units.page.height}`
      : `${key}\n`;
    const relative = cache.get(unitsKey);
    if (relative) return relative;
    const mapped =
      classes.length === 0 ? { style: EMPTY, relative: false } : mapClasses(compiler, classes, report, units);
    if (mapped.relative) {
      cache.set(unitsKey, mapped.style);
      return mapped.style;
    }
    style = mapped.style;
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
