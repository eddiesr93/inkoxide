/**
 * Finds the tokens of source files that may be Tailwind classes, for the table of classes a build
 * makes (`plugin.ts`). It reads more than classes, which costs only a lookup each: a token Tailwind
 * does not know is not kept. Arbitrary values (`text-[9pt]`, `font-features-['ss01']`,
 * `bg-(--brand)`) stay whole, quotes and commas inside them included.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, join } from 'node:path';

/** The files it reads, by extension. */
export const SCANNED = new Set([
  '.js',
  '.jsx',
  '.ts',
  '.tsx',
  '.mjs',
  '.cjs',
  '.mts',
  '.cts',
  '.vue',
  '.svelte',
  '.astro',
  '.html',
  '.md',
  '.mdx',
]);

/** Directories it does not go into anywhere, besides hidden ones (`.git`, `.next`). */
const SKIPPED = new Set(['node_modules']);
/** Build output, which it does not go into directly under a root. */
const OUTPUT = new Set(['dist', 'build', 'out', 'coverage', 'target', 'tmp']);

/** Files larger than this are not read: bundles and data, not sources. */
const MAX_FILE = 1 << 20;
/** Tokens longer than this are not classes; one that grows past it is dropped. */
const MAX_TOKEN = 256;

/** Characters that end a token outside brackets: a backslash too, of an escaped quote. */
const DELIMITERS = new Set(' \t\r\n"\'`<>{};,=)\\');

/**
 * Adds the tokens of `text` that may be classes to `out`: once with brackets holding values
 * (`text-[9pt]`), and once with brackets as delimiters, for classes inside code's own brackets
 * (`[tw('p-4')]`).
 */
export function candidatesIn(text: string, out: Set<string>): void {
  tokens(text, out, true);
  tokens(text, out, false);
}

function tokens(text: string, out: Set<string>, values: boolean): void {
  let start = -1;
  let depth = 0;
  const end = (i: number) => {
    if (start >= 0 && depth === 0) add(text.slice(start, i), out);
    start = -1;
    depth = 0;
  };
  for (let i = 0; i < text.length; i++) {
    const c = text[i] as string;
    if (start >= 0 && i - start > MAX_TOKEN) {
      start = -1;
      depth = 0;
    }
    if (c === '\n' || c === '\r') {
      end(i);
      continue;
    }
    if (depth > 0) {
      if (c === '[' || c === '(') depth++;
      else if (c === ']' || c === ')') depth--;
      continue;
    }
    // `(` opens a value only right after `-` (`bg-(--brand)`); elsewhere it is a call's.
    const opens = values && (c === '[' || (c === '(' && text[i - 1] === '-' && start >= 0));
    if (DELIMITERS.has(c) || (!opens && (c === '(' || c === '[' || c === ']'))) {
      end(i);
      continue;
    }
    if (start < 0) start = i;
    if (opens) depth = 1;
  }
  end(text.length);
}

/** A class starts as Tailwind's do: a letter, a digit, or `-`, `!`, `@`, `[`, `*`. */
const START = /^[a-z0-9\-!@[*]/i;

function add(token: string, out: Set<string>): void {
  if (token.length === 0 || token.length > MAX_TOKEN || !START.test(token)) return;
  out.add(token);
  // A Svelte class directive (`class:p-4={on}`).
  if (token.startsWith('class:')) add(token.slice('class:'.length), out);
  // At the end of a sentence or a label.
  const trimmed = token.replace(/[.:]+$/, '');
  if (trimmed !== token && trimmed.length > 0) out.add(trimmed);
}

/**
 * The files under `roots` (directories or files) that it reads, without `node_modules`, build
 * output and hidden directories, nor `exclude` (absolute paths): in a directory, those of the
 * types in `SCANNED`; a root that is a file, whatever its type.
 */
export function sourceFiles(roots: readonly string[], exclude: ReadonlySet<string> = new Set()): string[] {
  const files: string[] = [];
  const walk = (path: string, level: number) => {
    if (exclude.has(path)) return;
    let stat;
    try {
      stat = statSync(path);
    } catch {
      return;
    }
    if (stat.isDirectory()) {
      const name = path.split(/[\\/]/).pop() ?? '';
      if (level > 0 && (SKIPPED.has(name) || name.startsWith('.'))) return;
      if (level === 1 && OUTPUT.has(name)) return;
      for (const entry of readdirSync(path).sort()) walk(join(path, entry), level + 1);
    } else if (stat.isFile() && (level === 0 || SCANNED.has(extname(path))) && stat.size <= MAX_FILE) {
      files.push(path);
    }
  };
  for (const root of roots) walk(root, 0);
  return files;
}

/** The tokens of `files` that may be classes. */
export function scanFiles(files: readonly string[]): Set<string> {
  const out = new Set<string>();
  for (const file of files) {
    try {
      candidatesIn(readFileSync(file, 'utf8'), out);
    } catch {
      // Gone since it was listed.
    }
  }
  return out;
}
