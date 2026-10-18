/**
 * The precompiled entry maps classes as the compiler does: a table built from the classes of the
 * mapper's tests, the documented subset and every class string of the examples, benchmarks and the
 * React and render tests gives the same styles and the same errors, and orders any set of classes
 * as Tailwind does.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { beforeAll, describe, expect, it, vi } from 'vitest';

import { TailwindCompiler } from '../src/compile.ts';
import * as compiler from '../src/index.ts';
import { candidatesIn, scanFiles, sourceFiles } from '../src/scan.ts';
import { A4_UNITS, SUPPORTED, UNSUPPORTED } from '../src/supported.ts';
import { TableBuilder } from '../src/table-builder.ts';
import { type Table, TableSource } from '../src/table-source.ts';

const ROOT = fileURLToPath(new URL('../../..', import.meta.url));

/** The files whose class strings the table must cover. */
const SOURCES = [
  'examples',
  'benchmarks',
  'packages/react/test',
  'packages/render/test',
  'packages/tailwind/test',
].map((d) => `${ROOT}${d}`);

/** The class strings of `files`: `className="…"`, `className={'…'}`, template parts, `tw('…')`. */
function classStrings(files: string[]): string[] {
  const out = new Set<string>();
  const patterns = [
    /className=(?:\{\s*)?(["'])([^"'\n]*)\1/g,
    /className=\{`([^`]*)`\}/g,
    /\btw\((["'])([^"'\n]*)\1/g,
  ];
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    for (const re of patterns) {
      for (const m of text.matchAll(re)) {
        const value = (m[2] ?? m[1]) as string;
        // A template's literal parts.
        for (const part of value.split(/\$\{[^}]*\}/)) if (part.trim()) out.add(part.trim());
      }
    }
  }
  return [...out].sort();
}

/** The table of the classes Tailwind knows among `candidates`, for the bundled theme. */
async function tableOf(candidates: Iterable<string>): Promise<Table> {
  const builder = await TableBuilder.create();
  builder.add(candidates);
  return builder.table();
}

type Entry = typeof compiler;
let table: Table;
let precompiled: Entry;
let strings: string[];

/** `@inkoxide/tailwind/precompiled` with `t` as its table. */
async function withTable(t: unknown): Promise<Entry> {
  vi.resetModules();
  vi.doMock('../src/table.ts', () => ({ TABLE: t }));
  try {
    return (await import('../src/precompiled.ts')) as unknown as Entry;
  } finally {
    vi.doUnmock('../src/table.ts');
  }
}

beforeAll(async () => {
  const files = sourceFiles(SOURCES);
  strings = classStrings(files);
  const candidates = scanFiles(files);
  for (const f of SUPPORTED) for (const cls of Object.keys(f.examples)) candidatesIn(cls, candidates);
  for (const { example } of UNSUPPORTED) candidatesIn(example, candidates);
  table = await tableOf(candidates);
  await compiler.initTailwind();
  compiler.setTailwindStrict(true);
  precompiled = await withTable(table);
  await precompiled.initTailwind();
  precompiled.setTailwindStrict(true);
}, 60_000);

/**
 * The style of `cls`, or the message of its error. The table cannot tell a class Tailwind does
 * not know from one the build did not see, and says so: it reads here as the compiler's message.
 */
function outcome(entry: Entry, cls: string, units: typeof A4_UNITS | undefined = A4_UNITS): unknown {
  try {
    return entry.tw(cls, units);
  } catch (e) {
    return (e as Error).message.replace(
      /not a Tailwind utility, or one the build did not see: .*$/,
      'not a Tailwind utility (check the spelling, or see docs/TAILWIND_SUPPORT.md)',
    );
  }
}

describe('the precompiled entry', () => {
  it('maps every documented example and refuses every unsupported one as the compiler does', () => {
    for (const family of SUPPORTED) {
      for (const [cls, expected] of Object.entries(family.examples)) {
        expect(precompiled.tw(cls, family.units), cls).toEqual(expected);
      }
    }
    for (const { example } of UNSUPPORTED) {
      const message = outcome(compiler, `p-2 ${example}`);
      expect(typeof message, example).toBe('string');
      expect(outcome(precompiled, `p-2 ${example}`), example).toEqual(message);
    }
  });

  it('maps every class string of the examples, benchmarks and tests as the compiler does', () => {
    expect(strings.length).toBeGreaterThan(200);
    for (const s of strings) expect(outcome(precompiled, s), s).toEqual(outcome(compiler, s));
    // Without units too, where a relative value is an error.
    for (const s of strings) {
      expect(outcome(precompiled, s, undefined), s).toEqual(outcome(compiler, s, undefined));
    }
  });

  it('orders any set of its classes as Tailwind does', async () => {
    const tailwind = await TailwindCompiler.load();
    const source = new TableSource(table);
    const usable = Object.keys(table.classes).filter((c) => !('problem' in (table.classes[c] ?? {})));
    expect(usable.length).toBeGreaterThan(300);
    // A fixed sequence, so that a failure repeats.
    let seed = 42;
    const next = (n: number) => {
      seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648;
      return seed % n;
    };
    for (let i = 0; i < 5000; i++) {
      const pick = Array.from({ length: 2 + next(6) }, () => usable[next(usable.length)] as string);
      expect(source.sort(pick), pick.join(' ')).toEqual(tailwind.sort(pick));
    }
  });

  it('says that a class the build did not see may have been put together at run time', () => {
    // Put together here, as a template would, so that the scan does not see it.
    const unseen = ['bg', 'lime', '950'].join('-');
    let message = '';
    try {
      precompiled.tw(`p-2 ${unseen}`);
    } catch (e) {
      message = (e as Error).message;
    }
    expect(message).toContain(`Unsupported Tailwind class "${unseen}"`);
    expect(message).toContain('the build did not see');
    // The compiler knows it.
    expect(outcome(compiler, unseen)).toEqual({ backgroundColor: expect.any(Array) });
  });

  it('keeps the default family and only the theme values its classes reach', () => {
    expect(precompiled.tw('font-sans').fontFamily).toEqual(compiler.tw('font-sans').fontFamily);
    expect(table.theme['--font-sans']).toBeDefined();
    expect(table.theme[['--color', 'lime', '950'].join('-')]).toBeUndefined();
    expect(Object.keys(table.theme).length).toBeLessThan(400);
  });

  it('is the same bytes for the same classes and theme', async () => {
    const again = await tableOf(Object.keys(table.classes).reverse());
    expect(JSON.stringify(again)).toBe(JSON.stringify(table));
  });

  it('takes only the theme the build used, and needs a table', async () => {
    const t = await withTable(table);
    await expect(t.initTailwind({ css: '@import "tailwindcss/theme";' })).rejects.toThrow(
      /another theme than the one the build used/,
    );
    await t.initTailwind({ css: table.css });
    await expect((await withTable(null)).initTailwind()).rejects.toThrow(
      /no table of classes: build with the Tailwind build plugin/,
    );
    await expect((await withTable({ ...table, format: 99 })).initTailwind()).rejects.toThrow(
      /in format 99, and this version reads format 1/,
    );
  });
});
