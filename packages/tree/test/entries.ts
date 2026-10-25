/**
 * What the export tests of every package share: the names an entry file declares, values and
 * types. An entry may only re-export: `export { … } from`, `export type { … } from`, `export *
 * from` a file of its package, `export type * from '@inkoxide/tree/props'` (the props types every
 * front end shares), and the import of an engine host (`import './node-host.ts';`).
 */
import { readFileSync } from 'node:fs';

import { expect } from 'vitest';

export const sorted = (names: Iterable<string>): string[] => [...names].sort();

const PROPS = new URL('../src/props.ts', import.meta.url);

/** The names of `export { … } from` and `export type { … } from` in `code`. */
function listed(code: string): string[] {
  const names: string[] = [];
  for (const m of code.matchAll(/export\s+(?:type\s+)?\{([^}]*)\}\s*from\s*'[^']+';/g)) {
    for (const part of (m[1] ?? '').split(',')) {
      const name = part
        .trim()
        .replace(/^type\s+/, '')
        .split(/\s+as\s+/)
        .pop();
      if (name) names.push(name);
    }
  }
  return names;
}

/** The names `file` in the `src` folder declares, sorted; fails when it does more than re-export. */
export function declared(src: URL, file: string): string[] {
  const code = readFileSync(new URL(file, src), 'utf8').replace(/^\s*\/\/.*$/gm, '');
  const names = listed(code);
  for (const m of code.matchAll(/^export \* from '\.\/([\w/-]+\.tsx?)';$/gm))
    names.push(...declared(src, m[1] ?? ''));
  if (/^export type \* from '@inkoxide\/tree\/props';$/m.test(code)) {
    names.push(...listed(readFileSync(PROPS, 'utf8')));
  }
  const rest = code
    .replace(/export\s+(?:type\s+)?\{[^}]*\}\s*from\s*'[^']+';/g, '')
    .replace(/^import '\.\/[a-z-]+-host\.ts';$/m, '')
    .replace(/^export \* from '\.\/[\w/-]+\.tsx?';$/gm, '')
    .replace(/^export type \* from '@inkoxide\/tree\/props';$/m, '')
    .trim();
  expect(rest, `${file} only re-exports`).toBe('');
  return sorted(new Set(names));
}
