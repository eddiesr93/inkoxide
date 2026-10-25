/**
 * The public API of the engine package, which every front end builds on: `@inkoxide/render`,
 * `@inkoxide/render/browser` and `@inkoxide/render/templates`. A name added here is API under
 * semver, so it needs its place in README.md first.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
import * as browser from '../src/browser.ts';
import * as main from '../src/index.ts';
import * as templates from '../src/templates.ts';

const MAIN_VALUES = [
  'Font',
  'RenderError',
  'countPages',
  'createRenderer',
  'engineStatus',
  'init',
  'renderDocument',
  'version',
];
const MAIN_TYPES = [
  'DataIssue',
  'EngineStatus',
  'EngineVersion',
  'FontRegistration',
  'FontSource',
  'FontSrc',
  'FontWeight',
  'FrontEnd',
  'GenerateOptions',
  'LocalImageOptions',
  'NodeRenderer',
  'RemoteImageOptions',
  'RenderErrorCode',
  'RenderOpts',
  'RenderResult',
  'RenderToBufferOptions',
  'TemplateLike',
  'Timings',
];
const BROWSER_VALUES = [...MAIN_VALUES, 'clearCaches'];
const BROWSER_TYPES = [
  ...MAIN_TYPES.filter((n) => n !== 'LocalImageOptions' && n !== 'NodeRenderer'),
  'BrowserOptions',
  'BrowserRenderOptions',
  'BrowserRenderer',
];
const TEMPLATES_VALUES = ['parseData', 'renderDocument', 'renderSettled'];
const TEMPLATES_TYPES = [
  'GenerateOptions',
  'RenderOpts',
  'RenderResult',
  'TemplateLike',
  'Timings',
  'TreeResult',
];

const sorted = (names: Iterable<string>) => [...names].sort();

/**
 * The names an entry file exports, values and types: it may only re-export, with
 * `export { … } from`, `export type { … } from` and `export * from` a file of this package, and
 * import its engine host (`import './node-host.ts';`).
 */
function declared(file: string): string[] {
  const src = readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8');
  const code = src.replace(/^\s*\/\/.*$/gm, '');
  const names: string[] = [];
  for (const m of code.matchAll(/^export \* from '\.\/([a-z-]+\.tsx?)';$/gm))
    names.push(...declared(m[1] ?? ''));
  for (const m of code.matchAll(/export\s+(?:type\s+)?\{([^}]*)\}\s*from\s*'[^']+';/g)) {
    for (const part of (m[1] ?? '').split(',')) {
      const name = part
        .trim()
        .replace(/^type\s+/, '')
        .split(/\s+as\s+/)
        .pop();
      if (name) names.push(name);
    }
  }
  const rest = code
    .replace(/export\s+(?:type\s+)?\{[^}]*\}\s*from\s*'[^']+';/g, '')
    .replace(/^import '\.\/[a-z-]+-host\.ts';$/m, '')
    .replace(/^export \* from '\.\/[a-z-]+\.ts';$/gm, '')
    .trim();
  expect(rest, `${file} only re-exports`).toBe('');
  return sorted(names);
}

describe('exports', () => {
  it('@inkoxide/render exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(MAIN_VALUES));
    expect(declared('index.ts')).toEqual(sorted([...MAIN_VALUES, ...MAIN_TYPES]));
  });

  it('@inkoxide/render/browser exports the names that do not need Node', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declared('browser.ts')).toEqual(sorted([...BROWSER_VALUES, ...BROWSER_TYPES]));
  });

  it('@inkoxide/render/templates exports what templates share', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declared('templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('has an entry in package.json for each', () => {
    expect(Object.keys(pkg.exports).sort()).toEqual(['.', './browser', './templates']);
  });
});
