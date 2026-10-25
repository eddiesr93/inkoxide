/**
 * What the export tests of every package share: the names an entry file declares, values and
 * types. An entry may only re-export: `export { … } from`, `export type { … } from`, `export *
 * from` a file of its package, `export type * from '@inkoxide/tree/props'` (the props types every
 * front end shares), `export * from '@inkoxide/tree/common'` (the names every front end exports as
 * they are), and the import of an engine host (`import './node-host.ts';`).
 */
import { readFileSync } from 'node:fs';

import { expect } from 'vitest';

export const sorted = (names: Iterable<string>): string[] => [...names].sort();

const PROPS = new URL('../src/props.ts', import.meta.url);
const COMMON = new URL('../src/', import.meta.url);

/** The names of `export { … } from` and `export type { … } from` in `code`. */
export function listed(code: string): string[] {
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
  if (/^export \* from '@inkoxide\/tree\/common';$/m.test(code)) names.push(...declared(COMMON, 'common.ts'));
  const rest = code
    .replace(/export\s+(?:type\s+)?\{[^}]*\}\s*from\s*'[^']+';/g, '')
    .replace(/^import '\.\/[a-z-]+-host\.ts';$/m, '')
    .replace(/^export \* from '\.\/[\w/-]+\.tsx?';$/gm, '')
    .replace(/^export type \* from '@inkoxide\/tree\/props';$/m, '')
    .replace(/^export \* from '@inkoxide\/tree\/common';$/m, '')
    .trim();
  expect(rest, `${file} only re-exports`).toBe('');
  return sorted(new Set(names));
}

/**
 * The names none of `docs` mention: each public name needs its place in the package's README or
 * guide. The props types are the React guide's, for every front end.
 */
export function undocumented(names: Iterable<string>, docs: readonly URL[]): string[] {
  const text = docs.map((doc) => readFileSync(doc, 'utf8')).join('\n');
  const props = new Set(listed(readFileSync(PROPS, 'utf8')));
  return sorted(new Set(names)).filter((name) => !props.has(name) && !new RegExp(`\\b${name}\\b`).test(text));
}
