import { type MediaQuery, parseMedia } from '@inkoxide/style/internal';
import postcss, { type AtRule, type Container, type Rule } from 'postcss';

import { memo } from './cache.ts';
import type { CandidateCss, CandidateProblem, CandidateResult } from './candidate.ts';
import { sortByRank } from './parse.ts';
import { DEFAULT_THEME_CSS } from './theme.ts';

/**
 * What is used of Tailwind's design system. `tailwindcss` and `@tailwindcss/node` each declare the
 * whole type, and the two declarations do not match.
 */
export interface DesignSystem {
  theme: { entries(): Iterable<[string, { value: string }]> };
  getClassOrder(classes: string[]): Array<[string, bigint | null]>;
  candidatesToCss(classes: string[]): Array<string | null>;
}

/**
 * Loads Tailwind's design system for theme `css`, its `@import`s resolved from `base` (by default
 * the tailwindcss package). Each entry installs its own: Node's reads the imports from disk
 * (`node.ts`), the browser's has the theme bundled (`browser.ts`).
 */
export type DesignSystemLoader = (css: string, base: string | undefined) => Promise<DesignSystem>;

let loader: DesignSystemLoader | undefined;

export function setDesignSystemLoader(l: DesignSystemLoader): void {
  loader = l;
}

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
  // A field, not a parameter property: a build tool's config may load this file with Node's
  // type stripping (`@inkoxide/tailwind/plugin`).
  private readonly ds: DesignSystem;

  private constructor(ds: DesignSystem) {
    this.ds = ds;
    for (const [key, entry] of ds.theme.entries()) {
      this.theme.set(key, entry.value);
    }
  }

  static async load(css: string = DEFAULT_THEME_CSS, base?: string): Promise<TailwindCompiler> {
    if (!loader)
      throw new Error('@inkoxide/tailwind: no design system loader; import the package through an entry.');
    return new TailwindCompiler(await loader(css, base));
  }

  /** A theme variable (`--spacing`, `--color-red-500`), unresolved. */
  themeValue(name: string): string | undefined {
    return this.theme.get(name);
  }

  /** The place of each class in Tailwind's stylesheet; none for one it does not order. */
  ranks(classes: string[]): Map<string, bigint | null> {
    return new Map(this.ds.getClassOrder(classes));
  }

  /** Sorts classes the way Tailwind orders them in its stylesheet (later wins on conflicts). */
  sort(classes: string[]): string[] {
    if (classes.length < 2) return classes;
    const ranks = this.ranks(classes);
    return sortByRank(classes, (cls) => ranks.get(cls));
  }

  /** The CSS of one class, cached for the life of the compiler. */
  candidate(cls: string): CandidateResult {
    return memo(this.cache, cls, (c) => this.compileOne(c));
  }

  private compileOne(cls: string): CandidateResult {
    let css: string | null | undefined;
    try {
      [css] = this.ds.candidatesToCss([cls]);
    } catch {
      // Tailwind's parser gave up on the class (a value nested past its stack): no class.
      return { ok: false, problem: { kind: 'unknown' } };
    }
    if (css == null) return { ok: false, problem: { kind: 'unknown' } };
    let root;
    try {
      root = postcss.parse(css);
    } catch {
      // Tailwind wrote what is not CSS (an arbitrary value with a stray quote): no class.
      return { ok: false, problem: { kind: 'unknown' } };
    }
    const out: CandidateCss = { decls: [], initials: [] };
    // The declarations of the first rule: a variant repeats them, a utility's own `@media` not.
    let first: string | undefined;
    let problem: CandidateProblem | undefined;
    // Page variants wrap the rule in `@media`, nested for each one (`sm:landscape:`); a custom
    // variant may put it under several, any of which applies its declarations.
    const visit = (container: Container, media: readonly MediaQuery[]) =>
      container.each((node) => {
        if (node.type === 'rule') {
          const rule = node as Rule;
          if (
            rule.selector.trim().startsWith('.') &&
            !/[\s>:+~[\]]/.test(rule.selector.replace(/\\./g, ''))
          ) {
            // `md:container`, whose screen sizes Tailwind lifts out of the rule.
            const decls = rule.nodes.map(String).join(';');
            first ??= decls;
            if (decls !== first) {
              problem ??= { kind: 'screenSizes', rule: `@media ${media.map((q) => q.source).join(' and ')}` };
            }
            rule.each((d) => {
              if (d.type === 'decl') out.decls.push([d.prop, d.value, media]);
              else if (d.type === 'rule') problem ??= { kind: 'selector', selector: (d as Rule).selector };
              else if (d.type === 'atrule') {
                const at = d as AtRule;
                const text = `@${at.name} ${at.params}`;
                // `@supports`: a progressive-enhancement override; the plain declarations are the fallback.
                if (at.name === 'media') problem ??= { kind: 'screenSizes', rule: text };
                else if (at.name !== 'supports') problem ??= { kind: 'atRule', rule: text };
              }
            });
          } else {
            problem ??= { kind: 'selector', selector: rule.selector };
          }
        } else if (node.type === 'atrule') {
          const at = node as AtRule;
          if (at.name === 'property') {
            at.each((d) => {
              if (d.type === 'decl' && d.prop === 'initial-value')
                out.initials.push([at.params.trim(), d.value]);
            });
          } else if (at.name === 'media') {
            let query: MediaQuery;
            try {
              query = parseMedia(at.params);
            } catch (e) {
              problem ??= { kind: 'media', message: (e as Error).message };
              return;
            }
            visit(at, [...media, query]);
          } else if (at.name !== 'supports') {
            problem ??= { kind: 'atRule', rule: `@${at.name} ${at.params}` };
          }
        }
      });
    visit(root, []);
    if (problem) return { ok: false, problem };
    if (first === undefined) return { ok: false, problem: { kind: 'unknown' } };
    return { ok: true, css: out };
  }
}
