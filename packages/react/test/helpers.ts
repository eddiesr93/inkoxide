import { setTailwindStrict } from '@inkoxide/tailwind';

import { FontRegistry, loadTailwind, TemplateError, type TreeResult } from '../src/index.ts';

export const FACES = [
  { id: 'Inter-Regular', family: 'Inter' },
  { id: 'Inter-Bold', family: 'Inter', weight: 700 },
  { id: 'Inter-Italic', family: 'Inter', style: 'italic' as const },
  { id: 'NotoSans-Regular', family: 'Noto Sans' },
  { id: 'RobotoMono-Regular', family: 'Roboto Mono' },
];

export function registry(): FontRegistry {
  const r = new FontRegistry();
  for (const f of FACES) r.register(f);
  return r;
}

export async function setup(): Promise<void> {
  await loadTailwind();
  setTailwindStrict(true);
}

/** The nodes of the first page. */
export const body = (r: TreeResult) => r.document.pages[0].children ?? [];
/** The first node of the first page. */
export const first = <T>(r: TreeResult) => body(r)[0] as T;

/** The TemplateError that `fn` throws. */
export function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}
