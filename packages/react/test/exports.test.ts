/**
 * The public API: every name the package exports, from `@inkoxide/react` and
 * `@inkoxide/react/templates`. A name added here is API under semver, so it needs its place in
 * GUIDE.md first.
 */
import { describe, expect, it } from 'vitest';

import { declared, sorted } from '../../tree/test/entries.ts';
import pkg from '../package.json' with { type: 'json' };
import * as browser from '../src/browser.ts';
import * as main from '../src/index.ts';
import * as templates from '../src/templates.ts';

/** What @inkoxide/react exported before 1.0, kept for the code that uses it (core.ts). */
const COMPAT_VALUES = [
  'FontRegistry',
  'HOST',
  'HOST_NAME',
  'assertValidTree',
  'familyKey',
  'fontStretchValue',
  'fontVariationsValue',
  'fontWeightValue',
  'fonts',
  'imageSource',
  'innermostComponent',
  'isImagePath',
  'loadTailwind',
  'parsePath',
  'remoteAllowed',
  'remoteRefused',
  'validateTree',
];
const COMPAT_TYPES = [
  'EncryptionOptions',
  'FontFace',
  'HostType',
  'ImageSource',
  'OutputOptions',
  'PdfALevel',
  'PdfUaPart',
  'PendingImage',
  'RemoteImagePolicy',
  'ResolvedFont',
  'SchemaProblem',
];

const MAIN_VALUES = [
  ...COMPAT_VALUES,
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
  'TemplateError',
  'Text',
  'TextField',
  'TextInput',
  'TextPath',
  'TotalPages',
  'Tspan',
  'View',
  'countPages',
  'engineStatus',
  'facturX',
  'facturXRelationship',
  'init',
  'renderTemplate',
  'renderToBuffer',
  'renderToFile',
  'renderToStream',
  'renderToTree',
  'usePage',
  'version',
];

const MAIN_TYPES = [
  ...COMPAT_TYPES,
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
  'Conformance',
  'DataIssue',
  'DefsProps',
  'DocumentProps',
  'Edges',
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
  'FontStretch',
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
  'Located',
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
  'ReactProps',
  'RectProps',
  'RemoteImageOptions',
  'RenderErrorCode',
  'RenderProp',
  'RenderToBufferOptions',
  'Role',
  'RowProps',
  'SelectProps',
  'SpacerProps',
  'StopProps',
  'Style',
  'StyleProp',
  'SvgElementProps',
  'SvgPresentation',
  'SvgProps',
  'SvgStyle',
  'SvgTextProps',
  'TableProps',
  'TemplateErrorKind',
  'TextFieldProps',
  'TextFormat',
  'TextPathProps',
  'TextProps',
  'TreeOptions',
  'TreeResult',
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
 * `@inkoxide/react/browser`: the main entry without what needs Node, with `renderToBlob`,
 * `clearCaches`, and `generate` from the templates.
 */
const BROWSER_VALUES = [
  ...MAIN_VALUES.filter((n) => n !== 'renderToFile'),
  'BlobProvider',
  'clearCaches',
  'generate',
  'PdfDownloadLink',
  'PdfViewer',
  'renderToBlob',
  'usePdf',
];
const BROWSER_TYPES = [
  ...MAIN_TYPES.filter((n) => n !== 'LocalImageOptions'),
  'BlobProviderProps',
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

const SRC = new URL('../src/', import.meta.url);
const declaredIn = (file: string) => declared(SRC, file);

describe('exports', () => {
  it('@inkoxide/react exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(MAIN_VALUES));
    expect(declaredIn('index.ts')).toEqual(sorted([...MAIN_VALUES, ...MAIN_TYPES]));
  });

  it('@inkoxide/react/templates exports exactly the template pipeline', () => {
    expect(sorted(Object.keys(templates))).toEqual(sorted(TEMPLATES_VALUES));
    expect(declaredIn('templates.ts')).toEqual(sorted([...TEMPLATES_VALUES, ...TEMPLATES_TYPES]));
  });

  it('@inkoxide/react/browser exports the main names that do not need Node', () => {
    expect(sorted(Object.keys(browser))).toEqual(sorted(BROWSER_VALUES));
    expect(declaredIn('browser.ts')).toEqual(sorted([...BROWSER_VALUES, ...BROWSER_TYPES]));
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({
      '.': './src/index.ts',
      './templates': './src/templates.ts',
      './browser': './src/browser.ts',
    });
  });
});
