/**
 * The table the Tailwind build plugin writes (`table-builder.ts`), and the class source that reads it
 * in place of Tailwind's compiler (`precompiled.ts`). Imports neither the compiler nor postcss.
 */
import { type MediaQuery, parseMedia } from '@inkoxide/style/internal';

import { memo } from './cache.ts';
import type { CandidateCss, CandidateProblem, CandidateResult } from './candidate.ts';
import { type ClassSource, sortByRank } from './parse.ts';

/** The version of the table's format: a table of another fails `initTailwind()`. */
export const TABLE_FORMAT = 1;

/** One class as the table keeps it: its CSS and its rank in Tailwind's order, or its problem. */
export type TableEntry =
  | {
      /** `[property, value, media]`, the page media as written in their `@media` rules. */
      decls: Array<[string, string, string[]]>;
      initials: Array<[string, string]>;
      /** Its place in Tailwind's stylesheet, a decimal integer; none for a class without one. */
      rank: string | null;
    }
  | { problem: CandidateProblem };

/** The classes a build saw, with what `mapClasses` needs of each. */
export interface Table {
  format: number;
  /** The theme CSS the table was built with, as given. */
  css: string;
  /** Every class the build saw that Tailwind knows, usable or not, by name. */
  classes: Record<string, TableEntry>;
  /** The theme variables their values can reach, unresolved. */
  theme: Record<string, string>;
}

/** Reads `table` in place of Tailwind's compiler. */
export class TableSource implements ClassSource {
  private readonly results = new Map<string, CandidateResult>();
  private readonly ranks = new Map<string, bigint>();
  /** Parsed media queries, by text: the classes of a table share a few. */
  private readonly queries = new Map<string, MediaQuery>();
  private readonly table: Table;

  constructor(table: Table) {
    this.table = table;
  }

  candidate(cls: string): CandidateResult {
    return memo(this.results, cls, (c) => this.read(c));
  }

  private read(cls: string): CandidateResult {
    const entry = Object.hasOwn(this.table.classes, cls) ? this.table.classes[cls] : undefined;
    if (entry === undefined) return { ok: false, problem: { kind: 'unseen' } };
    if ('problem' in entry) return { ok: false, problem: entry.problem };
    const query = (text: string) => memo(this.queries, text, parseMedia);
    const css: CandidateCss = {
      decls: entry.decls.map(([p, v, media]) => [p, v, media.map(query)]),
      initials: entry.initials,
    };
    if (entry.rank !== null) this.ranks.set(cls, BigInt(entry.rank));
    return { ok: true, css };
  }

  sort(classes: string[]): string[] {
    return sortByRank(classes, (cls) => {
      this.candidate(cls);
      return this.ranks.get(cls);
    });
  }

  themeValue(name: string): string | undefined {
    return Object.hasOwn(this.table.theme, name) ? this.table.theme[name] : undefined;
  }
}

/** `value` as a table, or why it is not one this version reads. */
/**
 * Where a module that Next.js's Turbopack bundles in place of `@inkoxide/tailwind` leaves the
 * table (`withInkoxideTailwind()`): Turbopack has no hook to put it in `table.ts`.
 */
export const TABLE_GLOBAL = Symbol.for('@inkoxide/tailwind table');

export function checkTable(value: unknown): Table {
  if (value == null) {
    throw new Error(
      '@inkoxide/tailwind/precompiled has no table of classes: build with the Tailwind build plugin ' +
        '(`inkoxideTailwind()` from `@inkoxide/tailwind/plugin`), or import `@inkoxide/tailwind`.',
    );
  }
  const table = value as Partial<Table>;
  if (table.format !== TABLE_FORMAT) {
    throw new Error(
      `@inkoxide/tailwind: the table of classes is in format ${String(table.format)}, and this version reads format ${TABLE_FORMAT}: build again with the plugin of the same version.`,
    );
  }
  return value as Table;
}
