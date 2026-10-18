/**
 * The sources a theme names, for the Tailwind build plugin's scan (`plugin.ts`): its `@source`
 * directives, as Tailwind reads them. `@source "../lib"` scans a directory or a file, and a glob
 * (`@source "../src/**\/*.tsx"`) the files it matches; `@source not` leaves them out;
 * `@source inline("…")` keeps classes, braces expanded (`bg-red-{100..900..100}`), and
 * `@source not inline("…")` never keeps them. `source(none)` on `@import "tailwindcss"` scans only
 * what the directives name, and `source("../app")` starts the scan there in place of the project
 * root. Paths are relative to the CSS file that has them, an imported one too. Runs in Node.
 */
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve, sep } from 'node:path';

import { compile } from '@tailwindcss/node';

import type { SourceGlob } from './scan.ts';

/** What a theme's directives add to the scan. */
export interface ThemeSources {
  /**
   * Where the scan starts in place of the project root: none for `source(none)`, a directory for
   * `source("…")`; undefined when the theme says neither.
   */
  root: string[] | undefined;
  /** Directories, files and globs to scan besides. */
  include: (string | SourceGlob)[];
  /** Directories and files, and globs, to leave out. */
  exclude: string[];
  excludeGlobs: SourceGlob[];
  /** Classes to keep, and classes never to keep. */
  inline: string[];
  notInline: Set<string>;
  /** The CSS files the theme is made of, besides its own: what it imports. */
  files: string[];
}

/** This package's `tailwindcss`. */
const TAILWIND_DIR = dirname(createRequire(import.meta.url).resolve('tailwindcss/package.json'));

/** Tailwind's own stylesheets, as its package's `style` exports name them. */
async function tailwindStylesheet(id: string): Promise<string | undefined> {
  if (id === 'tailwindcss') return join(TAILWIND_DIR, 'index.css');
  if (!id.startsWith('tailwindcss/')) return undefined;
  const file = id.slice('tailwindcss/'.length);
  return join(TAILWIND_DIR, file.endsWith('.css') ? file : `${file}.css`);
}

/** The sources theme `css` names, with its paths relative to `base`. */
export async function themeSources(css: string, base: string): Promise<ThemeSources> {
  const files: string[] = [];
  const compiled = await compile(css, {
    base,
    onDependency: (path) => files.push(path),
    // Tailwind's stylesheets name no sources: this package's will do, wherever the theme is.
    customCssResolver: (id) => tailwindStylesheet(id),
  });
  const sources: ThemeSources = {
    root: undefined,
    include: [],
    exclude: [],
    excludeGlobs: [],
    inline: [],
    notInline: new Set(),
    files,
  };
  if (compiled.root === 'none') sources.root = [];
  else if (compiled.root !== null) sources.root = [resolve(compiled.root.base, compiled.root.pattern)];
  for (const { base: from, pattern, negated } of compiled.sources) {
    for (const p of expand(pattern)) {
      const glob = globOf(from, p);
      if (negated) {
        if (typeof glob === 'string') sources.exclude.push(glob);
        else sources.excludeGlobs.push(glob);
      } else {
        sources.include.push(glob);
      }
    }
  }
  // Tailwind keeps `inline` to itself: the theme's text and that of what it imports, without
  // comments.
  for (const text of [css, ...files.map((f) => readFileSync(f, 'utf8'))]) {
    for (const m of text.replace(COMMENT, '').matchAll(INLINE)) {
      const classes = (m[3] as string).split(/\s+/).filter(Boolean).flatMap(expand);
      if (m[1] === undefined) sources.inline.push(...classes);
      else for (const c of classes) sources.notInline.add(c);
    }
  }
  return sources;
}

/** A CSS comment. */
const COMMENT = /\/\*[\s\S]*?\*\//g;

/** `@source inline("…")` and `@source not inline("…")`. */
const INLINE = /@source\s+(not\s+)?inline\(\s*(["'])(.*?)\2\s*\)/gs;

/** Characters that make a part of a path a glob's. */
const GLOB = /[*?[{]/;

/**
 * A directory or file at `pattern` from `base`; a glob, the directory before its first part with
 * a wildcard and what it matches under it.
 */
function globOf(base: string, pattern: string): string | SourceGlob {
  const parts = pattern.split('/');
  const fixed = parts.findIndex((p) => GLOB.test(p));
  if (fixed < 0) return resolve(base, pattern);
  // `/abs/**` starts at the root: its first part is empty.
  const dir = resolve(base, parts.slice(0, fixed).join('/') || '.');
  return { dir, match: globRegExp(parts.slice(fixed).join('/')) };
}

/** A regular expression for glob `glob` (without braces): `**`, `*`, `?` and `[…]`. */
export function globRegExp(glob: string): RegExp {
  let re = '';
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i] as string;
    if (c === '*' && glob[i + 1] === '*') {
      // `**/` is any number of directories, none too; `**` at the end, anything.
      const slash = glob[i + 2] === '/';
      re += slash ? '(?:.*/)?' : '.*';
      i += slash ? 2 : 1;
    } else if (c === '*') {
      re += '[^/]*';
    } else if (c === '?') {
      re += '[^/]';
    } else if (c === '[') {
      const end = glob.indexOf(']', i + 2);
      if (end < 0) {
        re += '\\[';
        continue;
      }
      const set = glob
        .slice(i + 1, end)
        .replace(/^!/, '^')
        .replace(/\\/g, '\\\\');
      re += `[${set}]`;
      i = end;
    } else {
      re += c.replace(/[.+^${}()|\\]/g, '\\$&');
    }
  }
  return new RegExp(`^${re}$`, sep === '\\' ? 'i' : '');
}

/** At most this many strings from one pattern: braces multiply. */
const MAX_EXPANDED = 10_000;

/**
 * `pattern` with its braces expanded, as Tailwind expands them: `{a,b}` each, `{1..5}` and
 * `{100..900..100}` each number, nested too.
 */
export function expand(pattern: string): string[] {
  const open = pattern.indexOf('{');
  if (open < 0) return [pattern];
  let depth = 0;
  let close = -1;
  const commas: number[] = [];
  for (let i = open; i < pattern.length; i++) {
    const c = pattern[i];
    if (c === '{') depth++;
    else if (c === '}' && --depth === 0) {
      close = i;
      break;
    } else if (c === ',' && depth === 1) commas.push(i);
  }
  if (close < 0) return [pattern];
  const head = pattern.slice(0, open);
  const inner = pattern.slice(open + 1, close);
  const tails = expand(pattern.slice(close + 1));
  let alternatives: string[];
  const range = /^(-?\d+)\.\.(-?\d+)(?:\.\.(-?\d+))?$/.exec(inner);
  if (range !== null) {
    const [from, to] = [Number(range[1]), Number(range[2])];
    const step = Math.abs(Number(range[3] ?? 1)) || 1;
    alternatives = [];
    for (let n = from; from <= to ? n <= to : n >= to; n += from <= to ? step : -step) {
      alternatives.push(String(n));
      if (alternatives.length > MAX_EXPANDED) break;
    }
  } else if (commas.length === 0) {
    // Not a list: the braces stay.
    alternatives = expand(inner).map((a) => `{${a}}`);
  } else {
    const bounds = [open, ...commas, close];
    alternatives = bounds.slice(1).flatMap((end, k) => expand(pattern.slice((bounds[k] as number) + 1, end)));
  }
  const out: string[] = [];
  for (const a of alternatives) {
    for (const t of tails) {
      out.push(head + a + t);
      if (out.length >= MAX_EXPANDED) return out;
    }
  }
  return out;
}
