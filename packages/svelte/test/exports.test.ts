/**
 * The public API: every name @inkoxide/svelte exports, from each entry. A name added here is API under
 * semver, so it needs its place in the package's README or GUIDE.md first.
 */
import { describe, expect, it } from 'vitest';

import { declared, sorted, undocumented } from '../../tree/test/entries.ts';
import pkg from '../package.json' with { type: 'json' };
import * as browser from '../src/browser.ts';
import * as main from '../src/index.ts';
import * as templates from '../src/templates.ts';

const SRC = new URL('../src/', import.meta.url);
/** The props and style types, which every front end exports from `@inkoxide/tree/props`. */
const PROPS = declared(new URL('../../tree/src/', import.meta.url), 'props.ts');

const MAIN_VALUES = [
  'Attachment',
  'Canvas',
  'Cell',
  'Checkbox',
  'Circle',
  'ClipPath',
  'countPages',
  'Defs',
  'Document',
  'Ellipse',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'Field',
  'FieldSet',
  'Font',
  'Footer',
  'G',
  'Header',
  'Image',
  'ImageBackground',
  'init',
  'Line',
  'LinearGradient',
  'Link',
  'List',
  'Marker',
  'Note',
  'Page',
  'PAGE_SIZES',
  'PageBreak',
  'PageNumber',
  'Path',
  'Polygon',
  'Polyline',
  'RadialGradient',
  'Radio',
  'Rect',
  'RenderError',
  'renderTemplate',
  'renderToBuffer',
  'renderToFile',
  'renderToStream',
  'renderToTree',
  'Row',
  'Select',
  'Spacer',
  'Stop',
  'StyleSheet',
  'Svg',
  'Table',
  'TemplateError',
  'Text',
  'TextField',
  'TextInput',
  'TextPath',
  'TotalPages',
  'Tspan',
  'usePage',
  'version',
  'View',
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
  'Attachment',
  'BlobProvider',
  'Canvas',
  'Cell',
  'Checkbox',
  'Circle',
  'clearCaches',
  'ClipPath',
  'countPages',
  'createPdf',
  'Defs',
  'Document',
  'Ellipse',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'Field',
  'FieldSet',
  'Font',
  'Footer',
  'G',
  'generate',
  'Header',
  'Image',
  'ImageBackground',
  'init',
  'Line',
  'LinearGradient',
  'Link',
  'List',
  'Marker',
  'Note',
  'Page',
  'PAGE_SIZES',
  'PageBreak',
  'PageNumber',
  'Path',
  'PdfDownloadLink',
  'PdfViewer',
  'Polygon',
  'Polyline',
  'RadialGradient',
  'Radio',
  'Rect',
  'RenderError',
  'renderTemplate',
  'renderToBlob',
  'renderToBuffer',
  'renderToStream',
  'renderToTree',
  'Row',
  'Select',
  'Spacer',
  'Stop',
  'StyleSheet',
  'Svg',
  'Table',
  'TemplateError',
  'Text',
  'TextField',
  'TextInput',
  'TextPath',
  'TotalPages',
  'Tspan',
  'usePage',
  'version',
  'View',
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
  'Located',
  'Painter',
  'PdfState',
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

/** Where the names are documented: the package's own docs, and those of the names it shares. */
const OWN_DOCS = [new URL('../README.md', import.meta.url), new URL('../GUIDE.md', import.meta.url)];
const SHARED_DOCS = [
  new URL('../../react/GUIDE.md', import.meta.url),
  new URL('../../render/README.md', import.meta.url),
  new URL('../../tree/README.md', import.meta.url),
];

describe('exports', () => {
  it('@inkoxide/svelte exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(MAIN_VALUES));
    expect(declared(SRC, 'index.ts')).toEqual(sorted(new Set([...MAIN_VALUES, ...MAIN_TYPES])));
  });

  it('@inkoxide/svelte/browser exports the names that do not need Node, and the browser helpers', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declared(SRC, 'browser.ts')).toEqual(sorted(new Set([...BROWSER_VALUES, ...BROWSER_TYPES])));
  });

  it('@inkoxide/svelte/templates exports the template pipeline', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declared(SRC, 'templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('has an entry in package.json for each, with the svelte condition first', () => {
    expect(Object.keys(pkg.exports).sort()).toEqual(['.', './browser', './templates']);
    for (const entry of Object.values(pkg.exports))
      expect(Object.keys(entry as object)).toEqual(['svelte', 'default']);
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
