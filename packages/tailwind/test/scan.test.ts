/** The scanner finds classes in source text, arbitrary values whole. */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { candidatesIn, isSource, sourceFiles } from '../src/scan.ts';

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

  it('reads a file named as a source at any size, and the files of a directory up to 1 MiB', () => {
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-scan-'));
    try {
      const big = join(dir, 'big.md');
      writeFileSync(big, 'p-4 '.repeat(300_000));
      writeFileSync(join(dir, 'small.md'), 'p-4');
      expect(sourceFiles([dir]).map((f) => f.slice(dir.length + 1))).toEqual(['small.md']);
      expect(sourceFiles([big])).toEqual([big]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('tells in isSource the files sourceFiles lists', () => {
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-scan-'));
    try {
      const files = [
        'src/a.tsx',
        'src/b.css',
        'src/build/g.ts',
        'src/deep/dist/h.ts',
        'src/.hidden/i.ts',
        'node_modules/x/c.js',
        'dist/d.js',
        'tmp/t.tsx',
        'coverage/c.js',
        '.cache/e.ts',
        '.env.ts',
        'f.md',
        'big.md',
      ];
      for (const f of files) {
        mkdirSync(dirname(join(dir, f)), { recursive: true });
        writeFileSync(join(dir, f), f === 'big.md' ? 'p-4 '.repeat(300_000) : 'p-4');
      }
      const exclude = new Set([join(dir, 'src/deep')]);
      for (const [roots, skip] of [
        [[dir], new Set<string>()],
        [[dir], exclude],
        [[join(dir, 'src'), join(dir, 'big.md')], new Set<string>()],
      ] as const) {
        const listed = new Set(sourceFiles(roots, skip));
        for (const f of files) {
          expect(isSource(join(dir, f), roots, skip), f).toBe(listed.has(join(dir, f)));
        }
      }
      expect(sourceFiles([dir]).map((f) => f.slice(dir.length + 1))).toEqual([
        '.env.ts',
        'f.md',
        'src/a.tsx',
        'src/build/g.ts',
        'src/deep/dist/h.ts',
      ]);
      // A file that is not there yet is judged by its path alone.
      expect(isSource(join(dir, 'src/new.tsx'), [dir])).toBe(true);
      expect(isSource(join(dir, 'tmp/new.tsx'), [dir])).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
