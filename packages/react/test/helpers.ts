import { initTailwind, setTailwindStrict } from '@inkoxide/tailwind';

import { FontRegistry } from '../src/index.ts';

export const FACES = [
  { id: 'Inter-Regular', family: 'Inter', file: 'Inter-Regular.ttf' },
  { id: 'Inter-Bold', family: 'Inter', weight: 700, file: 'Inter-Bold.ttf' },
  { id: 'Inter-Italic', family: 'Inter', style: 'italic' as const, file: 'Inter-Italic.ttf' },
  { id: 'NotoSans-Regular', family: 'Noto Sans', file: 'NotoSans-Regular.ttf' },
  { id: 'RobotoMono-Regular', family: 'Roboto Mono', file: 'RobotoMono-Regular.ttf' },
];

export function registry(): FontRegistry {
  const r = new FontRegistry();
  for (const f of FACES) r.register(f);
  return r;
}

export async function setup(): Promise<void> {
  await initTailwind();
  setTailwindStrict(true);
}
