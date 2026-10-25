/**
 * The README examples are meant to be copied into a new project and run with `npx tsx`: a
 * CommonJS project with no tsconfig, where esbuild rejects a top-level await and JSX needs its
 * runtime named. examples/render.ts writes them from examples/; this checks what it wrote.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

const READMES = ['../README.md', '../../../README.md'];

/** The ```tsx blocks that render a file: complete programs, not fragments. */
function programs(readme: string): [string, string][] {
  const text = readFileSync(new URL(readme, import.meta.url), 'utf8');
  return [...text.matchAll(/```tsx\n([\s\S]*?)\n```/g)]
    .map((m) => m[1] as string)
    .filter((code) => /^(await )?renderToFile\(/m.test(code))
    .map((code) => [/'([\w-]+\.pdf)'/.exec(code)?.[1] ?? '?', code]);
}

describe.each(READMES)('the programs in %s', (readme) => {
  const blocks = programs(readme);

  it('are there', () => {
    expect(blocks.length).toBeGreaterThan(0);
  });

  it.each(blocks)('%s runs in a CommonJS project with no tsconfig', (_, code) => {
    expect(code.startsWith('/** @jsxRuntime automatic */\n')).toBe(true);
    expect(code).not.toMatch(/^await /m);
    expect(code).not.toMatch(/import\.meta/);
  });
});
