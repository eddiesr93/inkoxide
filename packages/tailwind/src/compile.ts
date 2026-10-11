import { createRequire } from 'node:module';
import { dirname } from 'node:path';

import { __unstable__loadDesignSystem } from '@tailwindcss/node';
import postcss, { type AtRule, type Rule } from 'postcss';

import { DEFAULT_THEME_CSS } from './theme.ts';

/** What Tailwind emits for one class, before any value is resolved. */
export interface CandidateCss {
  /** Declarations in source order, `[property, value]`; custom properties included. */
  decls: Array<[string, string]>;
  /** `@property --tw-*` initial values the rule depends on. */
  initials: Array<[string, string]>;
}

/** Why a class cannot be used, when Tailwind's output already says so. */
export type CandidateProblem =
  | { kind: 'unknown' }
  | { kind: 'variant'; detail: string }
  | { kind: 'selector'; detail: string };

export type CandidateResult = { ok: true; css: CandidateCss } | { ok: false; problem: CandidateProblem };

type DesignSystem = Awaited<ReturnType<typeof __unstable__loadDesignSystem>>;

/** Where `@import "tailwindcss/…"` resolves from: the tailwindcss package itself. */
const TAILWIND_DIR = dirname(createRequire(import.meta.url).resolve('tailwindcss/package.json'));

export { DEFAULT_THEME_CSS };

/**
 * Wraps Tailwind's design system: per-class CSS (`candidatesToCss`), class ordering and theme
 * values. Loading is async and slow (tens of ms); everything after that is synchronous.
 *
 * We use `__unstable__loadDesignSystem` (exported by `@tailwindcss/node`, used by Tailwind's
 * IntelliSense) rather than `compile().build()`: it returns the CSS of each class separately and
 * synchronously, whereas `build()` accumulates every class seen so far into one stylesheet.
 * The pinned Tailwind version guards against the "unstable" prefix.
 */
export class TailwindCompiler {
  private readonly cache = new Map<string, CandidateResult>();
  private readonly theme = new Map<string, string>();

  private constructor(private readonly ds: DesignSystem) {
    for (const [key, entry] of ds.theme.entries()) {
      this.theme.set(key, entry.value);
    }
  }

  static async load(css: string = DEFAULT_THEME_CSS, base: string = TAILWIND_DIR): Promise<TailwindCompiler> {
    const ds = await __unstable__loadDesignSystem(css, { base });
    return new TailwindCompiler(ds);
  }

  /** A theme variable (`--spacing`, `--color-red-500`), unresolved. */
  themeValue(name: string): string | undefined {
    return this.theme.get(name);
  }

  /** Sorts classes the way Tailwind orders them in its stylesheet (later wins on conflicts). */
  sort(classes: string[]): string[] {
    if (classes.length < 2) return classes;
    const order = this.ds.getClassOrder(classes);
    return order
      .map(([cls, rank], i) => ({ cls, rank: rank ?? -1n, i }))
      .sort((a, b) => (a.rank < b.rank ? -1 : a.rank > b.rank ? 1 : a.i - b.i))
      .map((x) => x.cls);
  }

  /** The CSS of one class, cached for the life of the process. */
  candidate(cls: string): CandidateResult {
    let hit = this.cache.get(cls);
    if (!hit) {
      hit = this.compileOne(cls);
      this.cache.set(cls, hit);
    }
    return hit;
  }

  private compileOne(cls: string): CandidateResult {
    const [css] = this.ds.candidatesToCss([cls]);
    if (css == null) return { ok: false, problem: { kind: 'unknown' } };
    const root = postcss.parse(css);
    const out: CandidateCss = { decls: [], initials: [] };
    let found = false;
    let problem: CandidateProblem | undefined;
    root.each((node) => {
      if (node.type === 'rule') {
        const rule = node as Rule;
        if (rule.selector.trim().startsWith('.') && !/[\s>:+~[\]]/.test(rule.selector.replace(/\\./g, ''))) {
          found = true;
          rule.each((d) => {
            if (d.type === 'decl') out.decls.push([d.prop, d.value]);
            else if (d.type === 'atrule' && (d as AtRule).name === 'supports') {
              // A progressive-enhancement override; the plain declarations are the fallback.
            } else if (d.type === 'rule' || d.type === 'atrule') {
              problem ??= { kind: 'selector', detail: d.toString().split('{')[0]?.trim() ?? '' };
            }
          });
        } else {
          problem ??= { kind: 'selector', detail: rule.selector };
        }
      } else if (node.type === 'atrule') {
        const at = node as AtRule;
        if (at.name === 'property') {
          at.each((d) => {
            if (d.type === 'decl' && d.prop === 'initial-value')
              out.initials.push([at.params.trim(), d.value]);
          });
        } else if (at.name !== 'supports') {
          problem ??= { kind: 'variant', detail: `@${at.name} ${at.params}` };
        }
      }
    });
    if (problem) return { ok: false, problem };
    if (!found) return { ok: false, problem: { kind: 'unknown' } };
    return { ok: true, css: out };
  }
}
