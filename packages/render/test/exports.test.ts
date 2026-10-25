/**
 * The public API of the engine package, which every front end builds on: `@inkoxide/render`,
 * `@inkoxide/render/browser` and `@inkoxide/render/templates`. A name added here is API under
 * semver, so it needs its place in README.md first.
 */
import { describe, expect, it } from 'vitest';

import { declared, sorted, undocumented } from '../../tree/test/entries.ts';
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
const BROWSER_VALUES = [...MAIN_VALUES, 'clearCaches', 'PdfRun', 'sameDocument', 'viewerUrl'];
const BROWSER_TYPES = [
  ...MAIN_TYPES.filter((n) => n !== 'LocalImageOptions' && n !== 'NodeRenderer'),
  'BlobRender',
  'BrowserOptions',
  'BrowserRenderOptions',
  'BrowserRenderer',
  'PdfSnapshot',
  'PdfState',
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

const SRC = new URL('../src/', import.meta.url);
const declaredIn = (file: string) => declared(SRC, file);

/** Where the names are documented: the package's own docs, and those of the names it shares. */
const OWN_DOCS = [new URL('../README.md', import.meta.url)];
const SHARED_DOCS = [
  new URL('../../react/GUIDE.md', import.meta.url),
  new URL('../../render/README.md', import.meta.url),
  new URL('../../tree/README.md', import.meta.url),
];

describe('exports', () => {
  it('@inkoxide/render exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(MAIN_VALUES));
    expect(declaredIn('index.ts')).toEqual(sorted([...MAIN_VALUES, ...MAIN_TYPES]));
  });

  it('@inkoxide/render/browser exports the names that do not need Node', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declaredIn('browser.ts')).toEqual(sorted([...BROWSER_VALUES, ...BROWSER_TYPES]));
  });

  it('@inkoxide/render/templates exports what templates share', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declaredIn('templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('has an entry in package.json for each', () => {
    expect(Object.keys(pkg.exports).sort()).toEqual(['.', './browser', './templates']);
  });

  it('has each name in its docs, or in those of the package it comes from', () => {
    const names = [
      ...MAIN_VALUES,
      ...MAIN_TYPES,
      ...BROWSER_VALUES,
      ...BROWSER_TYPES,
      ...TEMPLATES_VALUES,
      ...TEMPLATES_TYPES,
    ];
    expect(undocumented(names, [...OWN_DOCS, ...SHARED_DOCS])).toEqual([]);
  });
});
