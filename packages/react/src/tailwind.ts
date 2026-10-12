// `@inkoxide/tailwind` is optional: templates that style with `style` alone do not need it, and
// it is loaded only when it is installed.
import type { TwStyle } from '@inkoxide/style';

/**
 * What is used of `@inkoxide/tailwind`. Declared here, not imported, so that the published types
 * do not name a package that may not be installed.
 */
interface Tailwind {
  initTailwind(): Promise<void>;
  isTailwindReady(): boolean;
  tw(className: string): TwStyle;
  UnsupportedUtilityError: abstract new (...args: never[]) => Error & { className: string };
}

/** The loaded module, `null` when it is not installed, `undefined` before `loadTailwind`. */
let tailwind: Tailwind | null | undefined;

/** The font families of a document that sets none, Tailwind's default `font-sans`. */
const SANS = ['Inter', 'Noto Sans'];

/**
 * Loads and starts `@inkoxide/tailwind` when it is installed, so that `className` works, and
 * resolves to whether it is. `renderToTree` and `@inkoxide/render` call it; call it once before
 * `renderTemplate`, which is synchronous, when templates use `className`. A theme set up with
 * `initTailwind` beforehand is kept.
 */
export function loadTailwind(): Promise<boolean> {
  return loadWith(() => import('@inkoxide/tailwind'));
}

/** `loadTailwind` with the module from `load` (tests stand in a missing package). */
export async function loadWith(load: () => Promise<Tailwind>): Promise<boolean> {
  if (tailwind === undefined) {
    try {
      tailwind = await load();
    } catch (e) {
      if (!isMissing(e)) throw e;
      tailwind = null;
    }
  }
  await tailwind?.initTailwind();
  return tailwind != null;
}

/** Whether `e` says that `@inkoxide/tailwind` itself is not installed (not one of its imports). */
function isMissing(e: unknown): boolean {
  const { code, message } = (e ?? {}) as { code?: unknown; message?: unknown };
  return (
    code === 'ERR_MODULE_NOT_FOUND' &&
    typeof message === 'string' &&
    /['"]@inkoxide\/tailwind['"]/.test(message)
  );
}

/**
 * The style of `className`, or why there is none: the reason, to end "needs Tailwind: …". The
 * Tailwind error for an unsupported class is thrown, to report with the element.
 */
export function classStyle(className: string): TwStyle | string {
  if (tailwind === null) return 'install it with `npm install @inkoxide/tailwind`';
  if (!tailwind?.isTailwindReady()) {
    return 'call `await loadTailwind()` before `renderTemplate`, or use `renderToTree`';
  }
  return tailwind.tw(className);
}

/** Whether `e` is Tailwind's error for a class it cannot map. */
export function isUnsupportedClass(e: unknown): e is Error & { className: string } {
  return tailwind != null && e instanceof tailwind.UnsupportedUtilityError;
}

/** The font families of a document that sets none: the Tailwind theme's `font-sans`, if loaded. */
export function defaultFamily(): string[] {
  return (tailwind?.isTailwindReady() && tailwind.tw('font-sans').fontFamily) || SANS;
}
