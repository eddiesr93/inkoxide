/** A theme's `@source` directives: paths, globs, exclusions and inline classes. */
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterAll, describe, expect, it } from 'vitest';

import { sourceFiles } from '../src/scan.ts';
import { expand, globRegExp, themeSources } from '../src/sources.ts';

const dir = realpathSync(mkdtempSync(join(tmpdir(), 'inkoxide-sources-')));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('brace expansion', () => {
  it('expands lists, ranges and steps, nested', () => {
    expect(expand('p-4')).toEqual(['p-4']);
    expect(expand('{hover:,}bg-red-{50,{100..300..100},950}')).toEqual([
      'hover:bg-red-50',
      'hover:bg-red-100',
      'hover:bg-red-200',
      'hover:bg-red-300',
      'hover:bg-red-950',
      'bg-red-50',
      'bg-red-100',
      'bg-red-200',
      'bg-red-300',
      'bg-red-950',
    ]);
    expect(expand('m-{3..1}')).toEqual(['m-3', 'm-2', 'm-1']);
    // Not a list: the braces stay, as do braces that never close.
    expect(expand('a{b}c')).toEqual(['a{b}c']);
    expect(expand('a{b,c')).toEqual(['a{b,c']);
  });

  it('stops at a bound', () => {
    expect(expand('w-{1..100000}').length).toBeLessThanOrEqual(10_001);
  });
});

describe('globs', () => {
  it('match as Tailwind globs do', () => {
    const re = globRegExp('**/*.tsx');
    expect(re.test('a.tsx')).toBe(true);
    expect(re.test('x/y/a.tsx')).toBe(true);
    expect(re.test('a.ts')).toBe(false);
    expect(globRegExp('*.md').test('x/a.md')).toBe(false);
    expect(globRegExp('a?.[jt]s').test('ab.ts')).toBe(true);
    expect(globRegExp('a.(x)+').test('a.(x)+')).toBe(true);
  });
});

describe("a theme's sources", () => {
  for (const d of ['styles', 'lib/ui', 'legacy', 'src/deep', 'app'])
    mkdirSync(join(dir, d), { recursive: true });
  for (const f of [
    'lib/ui/a.ts',
    'lib/ui/b.txt',
    'legacy/c.ts',
    'src/d.tsx',
    'src/deep/e.tsx',
    'src/f.ts',
    'app/g.ts',
  ]) {
    writeFileSync(join(dir, f), 'p-4');
  }
  writeFileSync(join(dir, 'styles/more.css'), '@source "../app";\n@source not inline("p-9");\n');

  it('reads paths, globs, exclusions and inline classes, from imports too', async () => {
    const css = `@import "tailwindcss";
@import "./more.css";
@source "../lib";
@source not "../legacy";
@source "../src/**/*.tsx";
@source not "../src/deep/*.tsx";
@source inline("{md:,}p-{1..2}");`;
    const s = await themeSources(css, join(dir, 'styles'));
    expect(s.root).toBeUndefined();
    expect(s.inline).toEqual(['md:p-1', 'md:p-2', 'p-1', 'p-2']);
    expect([...s.notInline]).toEqual(['p-9']);
    expect(s.files).toContain(join(dir, 'styles/more.css'));
    const files = sourceFiles([dir, ...s.include], new Set(s.exclude), s.excludeGlobs).map((f) =>
      f.slice(dir.length + 1),
    );
    expect(files.sort()).toEqual(['app/g.ts', 'lib/ui/a.ts', 'src/d.tsx', 'src/f.ts']);
  });

  it('takes an absolute glob from the root, and no inline class from a comment', async () => {
    const css = `@import "tailwindcss" source(none);
@source "${dir}/src/**/*.tsx";
/* @source inline("m-7"); */
@source inline("m-8");`;
    const s = await themeSources(css, join(dir, 'styles'));
    expect(s.include).toEqual([{ dir: join(dir, 'src'), match: expect.any(RegExp) }]);
    expect(s.inline).toEqual(['m-8']);
  });

  it('starts where `source()` says, or nowhere', async () => {
    const none = await themeSources('@import "tailwindcss" source(none);', join(dir, 'styles'));
    expect(none.root).toEqual([]);
    const app = await themeSources('@import "tailwindcss" source("../app");', join(dir, 'styles'));
    expect(app.root).toEqual([join(dir, 'app')]);
  });
});
