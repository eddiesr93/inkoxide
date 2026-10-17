/** The scanner finds classes in source text, arbitrary values whole. */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { candidatesIn, sourceFiles } from '../src/scan.ts';

const tokens = (text: string) => {
  const out = new Set<string>();
  candidatesIn(text, out);
  return out;
};

describe('the scanner', () => {
  it('keeps arbitrary values, variants and important marks whole', () => {
    const found = tokens(
      `<View className="font-features-['ss01'] bg-(--brand) grid-cols-[1fr,2fr] md:p-4 !mt-2 p-1! text-[9pt]">`,
    );
    for (const cls of [
      "font-features-['ss01']",
      'bg-(--brand)',
      'grid-cols-[1fr,2fr]',
      'md:p-4',
      '!mt-2',
      'p-1!',
      'text-[9pt]',
    ]) {
      expect(found.has(cls), cls).toBe(true);
    }
  });

  it('reads strings, template parts and prose', () => {
    const found = tokens("const td = 'px-2 py-1';\nclassName={`${td} border-b`}\nUse p-4.");
    for (const cls of ['px-2', 'py-1', 'border-b', 'p-4']) expect(found.has(cls), cls).toBe(true);
    expect([...found].some((t) => t.includes('$'))).toBe(false);
  });

  it('reads escaped quotes and Svelte class directives', () => {
    const found = tokens('const html = "<div class=\\"m-2 p-3\\">";\n<div class:p-4={on} />');
    for (const cls of ['m-2', 'p-3', 'p-4']) expect(found.has(cls), cls).toBe(true);
  });

  it('drops a bracket that never closes, at the end of its line', () => {
    const found = tokens('a[ b c\np-3');
    expect(found.has('p-3')).toBe(true);
    expect([...found].some((t) => t.startsWith('a['))).toBe(false);
  });

  it('reads sources, not dependencies, build output or hidden directories', () => {
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-scan-'));
    try {
      for (const d of ['src', 'node_modules/x', 'dist', '.cache'])
        mkdirSync(join(dir, d), { recursive: true });
      for (const f of ['src/a.tsx', 'src/b.css', 'node_modules/x/c.js', 'dist/d.js', '.cache/e.ts', 'f.md']) {
        writeFileSync(join(dir, f), 'p-4');
      }
      const files = sourceFiles([dir]).map((f) => f.slice(dir.length + 1));
      expect(files).toEqual(['f.md', 'src/a.tsx']);
      expect(sourceFiles([dir], new Set([join(dir, 'src')])).map((f) => f.slice(dir.length + 1))).toEqual([
        'f.md',
      ]);
      // A folder named like build output is read below the root.
      mkdirSync(join(dir, 'src/build'));
      writeFileSync(join(dir, 'src/build/g.ts'), 'p-4');
      expect(sourceFiles([dir]).map((f) => f.slice(dir.length + 1))).toContain('src/build/g.ts');
      // A file named as a source is read whatever its type.
      expect(sourceFiles([join(dir, 'src/b.css')]).map((f) => f.slice(dir.length + 1))).toEqual([
        'src/b.css',
      ]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
