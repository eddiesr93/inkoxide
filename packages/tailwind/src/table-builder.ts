/**
 * Builds the table of classes the browser reads in place of Tailwind's compiler
 * (`precompiled.ts`): for each class a build saw that Tailwind knows, its CSS and its place in
 * Tailwind's order, and the theme values those can reach. Runs in Node, at build time.
 */
import './node.ts';
import type { CandidateResult } from './candidate.ts';
import { DEFAULT_THEME_CSS, TailwindCompiler } from './compile.ts';
import { variantOf } from './parse.ts';
import { TABLE_FORMAT, type Table, type TableEntry } from './table-source.ts';
import type { ThemeOptions } from './tw.ts';

/** The class every table has: the default family of a document that sets none. */
const ALWAYS = ['font-sans'];

/** The custom properties a value reads (`var(--name`). */
const VAR = /var\(\s*(--[\w-]+)/g;

/** A table of classes that grows as a build sees more (in a dev server, as files change). */
export class TableBuilder {
  private readonly seen = new Set<string>();
  private readonly known = new Map<string, CandidateResult>();
  private readonly compiler: TailwindCompiler;
  private readonly css: string;

  private constructor(compiler: TailwindCompiler, css: string) {
    this.compiler = compiler;
    this.css = css;
    this.add(ALWAYS);
  }

  static async create(theme: ThemeOptions = {}): Promise<TableBuilder> {
    const css = theme.css ?? DEFAULT_THEME_CSS;
    return new TableBuilder(await TailwindCompiler.load(css, theme.base), css);
  }

  /** Adds the classes Tailwind knows among `candidates`; true when there was one not seen. */
  add(candidates: Iterable<string>): boolean {
    let added = false;
    for (const c of candidates) {
      if (this.seen.has(c)) continue;
      this.seen.add(c);
      let r: CandidateResult;
      try {
        r = this.compiler.candidate(c);
      } catch {
        // Not a class Tailwind can write: a token of code, not of a class string.
        continue;
      }
      if (!r.ok && r.problem.kind === 'unknown') continue;
      this.known.set(c, r);
      added = true;
      // The class without its variants tells whose problem it is (`md:space-x-2`).
      if (!r.ok) this.add([c.slice(variantOf(c).length)]);
    }
    return added;
  }

  /** The table of the classes so far, the same bytes for the same classes and theme. */
  table(): Table {
    const names = [...this.known.keys()].sort();
    const usable = names.filter((n) => this.known.get(n)?.ok);
    // Ranks over the whole set order any subset of it as Tailwind does.
    const ranks = this.compiler.ranks(usable);
    const classes: Record<string, TableEntry> = {};
    const reached = new Set<string>();
    const reach = (value: string) => {
      for (const m of value.matchAll(VAR)) {
        const name = m[1] as string;
        if (reached.has(name)) continue;
        reached.add(name);
        const theme = this.compiler.themeValue(name);
        if (theme !== undefined) reach(theme);
      }
    };
    for (const name of names) {
      const r = this.known.get(name) as CandidateResult;
      if (!r.ok) {
        classes[name] = { problem: r.problem };
        continue;
      }
      const { decls, initials } = r.css;
      for (const [, v] of [...decls, ...initials]) reach(v);
      classes[name] = {
        decls: decls.map(([p, v, media]) => [p, v, media.map((q) => q.source)]),
        initials,
        rank: ranks.get(name)?.toString() ?? null,
      };
    }
    const theme: Record<string, string> = {};
    for (const name of [...reached].sort()) {
      const value = this.compiler.themeValue(name);
      if (value !== undefined) theme[name] = value;
    }
    return { format: TABLE_FORMAT, css: this.css, classes, theme };
  }
}
