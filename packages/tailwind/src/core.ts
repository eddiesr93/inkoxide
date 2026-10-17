/**
 * What both entries share: the state of `tw()` (the class source, strict mode, the cache) over a
 * `ClassSource`, Tailwind's compiler in `api.ts` or a table built at build time in
 * `precompiled.ts`. Imports neither the compiler nor postcss.
 */
import type { FlatStyle } from '@inkoxide/style';

import { Lru } from './cache.ts';
import {
  type ClassSource,
  mapClasses,
  splitClasses,
  type TwUnits,
  type UnsupportedUtilityError,
} from './parse.ts';

export interface TailwindOptions {
  /**
   * Theme CSS (`@import "tailwindcss/theme"; @theme { … }`). Defaults to `DEFAULT_THEME_CSS`, the
   * bundled theme; extend that one to keep its font stacks, which name the bundled fonts.
   */
  css?: string;
  /**
   * Directory that `@import`s in `css` resolve from (default: the tailwindcss package). In a
   * browser, `css` can import only `tailwindcss/theme`, and `base` is not used.
   */
  base?: string;
  /**
   * Throw on unsupported classes (default outside production). When false, the class is
   * logged once and ignored.
   */
  strict?: boolean;
  /** LRU size for class strings (default 5000). */
  cacheSize?: number;
}

let source: ClassSource | undefined;
/** The source load asked for last, until it settles: a later theme replaces an earlier one. */
let loading: Promise<ClassSource> | undefined;
let strict =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.NODE_ENV !==
  'production';
let cache = new Lru<FlatStyle>(5000);
const warned = new Set<string>();
const EMPTY: FlatStyle = Object.freeze({}) as FlatStyle;

/**
 * Applies `strict` and `cacheSize`, then loads a class source: `theme()` when the options set a
 * theme, else `fallback()` unless one is loaded or being loaded already. Waits for the source asked
 * for last, including one asked for while it waits.
 */
export async function initSource(
  opts: TailwindOptions,
  theme: (() => Promise<ClassSource>) | undefined,
  fallback: () => Promise<ClassSource>,
): Promise<void> {
  if (opts.strict !== undefined) setTailwindStrict(opts.strict);
  if (opts.cacheSize !== undefined) cache = new Lru<FlatStyle>(opts.cacheSize);
  if (theme) {
    loading = theme();
  } else if (!source) {
    loading ??= fallback();
  }
  while (loading) {
    const load = loading;
    try {
      const loaded = await load;
      if (loading !== load) continue;
      source = loaded;
      loading = undefined;
      cache.clear();
    } catch (e) {
      if (loading === load) loading = undefined;
      throw e;
    }
  }
}

export function isTailwindReady(): boolean {
  return source !== undefined;
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
  if (!source) throw new Error('Tailwind is not initialised: await initTailwind() at startup');
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
      classes.length === 0 ? { style: EMPTY, relative: false } : mapClasses(source, classes, report, units);
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
