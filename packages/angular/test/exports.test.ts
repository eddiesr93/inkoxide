/**
 * The public API: every name @inkoxide/angular exports, from each entry. A name added here is API under
 * semver, so it needs its place in the package's README or GUIDE.md first.
 */
// Angular's libraries are partially compiled; outside an Angular build they need the JIT compiler.
import '@angular/compiler';
import { describe, expect, it } from 'vitest';

import { declared, sorted } from '../../tree/test/entries.ts';
import pkg from '../package.json' with { type: 'json' };
import * as browser from '../src/browser.ts';
import * as main from '../src/index.ts';
import * as templates from '../src/templates.ts';

const SRC = new URL('../src/', import.meta.url);
/** The props and style types, which every front end exports from `@inkoxide/tree/props`. */
const PROPS = declared(new URL('../../tree/src/', import.meta.url), 'props.ts');

const MAIN_VALUES = [
  'countPages',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'Font',
  'init',
  'injectPage',
  'INK_PAGE',
  'PAGE_SIZES',
  'RenderError',
  'renderTemplate',
  'renderToBuffer',
  'renderToFile',
  'renderToStream',
  'renderToTree',
  'StyleSheet',
  'TemplateError',
  'version',
];
const MAIN_TYPES = [
  ...PROPS,
  'CanvasGradient',
  'CanvasTextOptions',
  'DataIssue',
  'EmojiSource',
  'EngineStatus',
  'EngineVersion',
  'FacturXOptions',
  'FacturXProfile',
  'FontRegistration',
  'FontSource',
  'FontSrc',
  'FontWeight',
  'LocalImageOptions',
  'Located',
  'Painter',
  'RemoteImageOptions',
  'Renderable',
  'RenderErrorCode',
  'RenderToBufferOptions',
  'TemplateErrorKind',
  'TreeOptions',
  'TreeResult',
];
const BROWSER_VALUES = [
  'InkBlobProvider',
  'InkPdfDownload',
  'InkPdfViewer',
  'clearCaches',
  'countPages',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'Font',
  'generate',
  'init',
  'injectPage',
  'injectPdf',
  'INK_PAGE',
  'PAGE_SIZES',
  'RenderError',
  'renderTemplate',
  'renderToBlob',
  'renderToBuffer',
  'renderToStream',
  'renderToTree',
  'StyleSheet',
  'TemplateError',
  'version',
];
const BROWSER_TYPES = [
  ...PROPS,
  'BrowserOptions',
  'BrowserRenderOptions',
  'CanvasGradient',
  'CanvasTextOptions',
  'DataIssue',
  'EmojiSource',
  'EngineStatus',
  'EngineVersion',
  'FacturXOptions',
  'FacturXProfile',
  'FontRegistration',
  'FontSource',
  'FontSrc',
  'FontWeight',
  'GenerateOptions',
  'BlobProviderContext',
  'InjectPdfResult',
  'Located',
  'Painter',
  'RemoteImageOptions',
  'Renderable',
  'RenderErrorCode',
  'RenderOpts',
  'RenderResult',
  'RenderToBufferOptions',
  'TemplateErrorKind',
  'TemplateLike',
  'Timings',
  'TreeOptions',
  'TreeResult',
];
const TEMPLATES_VALUES = ['generate', 'parseData', 'renderDocument', 'renderSettled', 'renderTree'];
const TEMPLATES_TYPES = [
  'GenerateOptions',
  'RenderOpts',
  'RenderResult',
  'TemplateLike',
  'Timings',
  'TreeResult',
];

describe('exports', () => {
  it('@inkoxide/angular exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(MAIN_VALUES));
    expect(declared(SRC, 'index.ts')).toEqual(sorted(new Set([...MAIN_VALUES, ...MAIN_TYPES])));
  });

  it('@inkoxide/angular/browser exports the names that do not need Node, and the browser helpers', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declared(SRC, 'browser.ts')).toEqual(sorted(new Set([...BROWSER_VALUES, ...BROWSER_TYPES])));
  });

  it('@inkoxide/angular/templates exports the template pipeline', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declared(SRC, 'templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({
      '.': './src/index.ts',
      './browser': './src/browser.ts',
      './templates': './src/templates.ts',
    });
  });
});
