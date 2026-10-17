/**
 * The public API: every name the package exports, from `@inkoxide/render` and
 * `@inkoxide/render/templates`. A name added here is API under semver, so it needs its place in
 * GUIDE.md first.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
import * as browser from '../src/browser.ts';
import * as main from '../src/index.ts';
import * as templates from '../src/templates.ts';

const MAIN_VALUES = [
  'Attachment',
  'Canvas',
  'Cell',
  'Checkbox',
  'Circle',
  'ClipPath',
  'Defs',
  'Document',
  'Ellipse',
  'Field',
  'FieldSet',
  'Font',
  'Footer',
  'G',
  'Header',
  'Image',
  'ImageBackground',
  'Line',
  'LinearGradient',
  'Link',
  'List',
  'Marker',
  'Note',
  'PAGE_SIZES',
  'Page',
  'PageBreak',
  'PageNumber',
  'Path',
  'Polygon',
  'Polyline',
  'RadialGradient',
  'Radio',
  'Rect',
  'RenderError',
  'Row',
  'Select',
  'Spacer',
  'Stop',
  'StyleSheet',
  'Svg',
  'Table',
  'Text',
  'TextField',
  'TextInput',
  'TotalPages',
  'Tspan',
  'View',
  'countPages',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'init',
  'renderToBuffer',
  'renderToFile',
  'renderToStream',
  'usePage',
  'version',
];

const MAIN_TYPES = [
  'AbsoluteLength',
  'AttachmentFile',
  'AttachmentProps',
  'BookmarkProp',
  'BreakProp',
  'CanvasGradient',
  'CanvasProps',
  'CanvasTextOptions',
  'CellProps',
  'CheckboxProps',
  'CircleProps',
  'ClipPathProps',
  'DataIssue',
  'DefsProps',
  'DocumentProps',
  'EllipseProps',
  'EmojiSource',
  'EngineStatus',
  'EngineVersion',
  'FacturXOptions',
  'FacturXProfile',
  'FieldProps',
  'FieldSetProps',
  'FontRegistration',
  'FontSource',
  'FontSrc',
  'FontWeight',
  'FontWeightName',
  'FooterProps',
  'GradientProps',
  'HeaderProps',
  'HyphenationCallback',
  'ImageBackgroundProps',
  'ImageProps',
  'ImageRequest',
  'ImageSrc',
  'ImageSrcValue',
  'Length',
  'LengthAuto',
  'LengthPercent',
  'LineProps',
  'LinkProps',
  'ListProps',
  'LocalImageOptions',
  'MarkerProps',
  'MediaStyle',
  'NoteProps',
  'PageBreakProps',
  'PageInfo',
  'PagePreset',
  'PageProps',
  'PageSize',
  'Painter',
  'PathProps',
  'Permissions',
  'PolyProps',
  'RadioProps',
  'RectProps',
  'RemoteImageOptions',
  'RenderErrorCode',
  'RenderProp',
  'RenderToBufferOptions',
  'RowProps',
  'SelectProps',
  'SpacerProps',
  'StopProps',
  'Style',
  'StyleProp',
  'SvgPresentation',
  'SvgProps',
  'SvgTextProps',
  'TableProps',
  'TextFieldProps',
  'TextFormat',
  'TextProps',
  'TspanProps',
  'ViewProps',
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

/**
 * `@inkoxide/render/browser`: the main entry without what needs Node, with `renderToBlob`,
 * `clearCaches`, and `generate` from the templates.
 */
const BROWSER_VALUES = [
  ...MAIN_VALUES.filter((n) => n !== 'renderToFile' && n !== 'renderToStream'),
  'clearCaches',
  'generate',
  'PdfDownloadLink',
  'PdfViewer',
  'renderToBlob',
  'usePdf',
];
const BROWSER_TYPES = [
  ...MAIN_TYPES.filter((n) => n !== 'LocalImageOptions'),
  'BrowserOptions',
  'BrowserRenderOptions',
  'GenerateOptions',
  'PdfDownloadLinkProps',
  'PdfViewerProps',
  'RenderOpts',
  'RenderResult',
  'TemplateLike',
  'Timings',
  'UsePdfResult',
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

  it('@inkoxide/render/templates exports exactly the template pipeline', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declared('templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('@inkoxide/render/browser exports the main names that do not need Node', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declared('browser.ts')).toEqual(sorted([...BROWSER_VALUES, ...BROWSER_TYPES]));
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({
      '.': './src/index.ts',
      './templates': './src/templates.ts',
      './browser': './src/browser.ts',
    });
  });
});
