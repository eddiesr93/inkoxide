/**
 * The public API of @inkoxide/tree, which every front end builds on: the names it exports, and the
 * props types `@inkoxide/tree/props` gives the front ends. A name added here is API under semver,
 * so it needs its place in the README first.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, expectTypeOf, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
import * as common from '../src/common.ts';
import type { FIELD_KINDS, HOST, PropsByComponent } from '../src/index.ts';
import * as main from '../src/index.ts';
import * as internal from '../src/internal.ts';
import { declared, listed, sorted } from './entries.ts';

const VALUES = [
  'BOOLEAN_PROPS',
  'COMPONENTS',
  'FIELD_KINDS',
  'FontRegistry',
  'HOST',
  'HOST_NAME',
  'PAGE_SIZES',
  'StyleSheet',
  'TAGS',
  'TemplateError',
  'assertValidTree',
  'buildDocument',
  'checkElement',
  'checkText',
  'checkTree',
  'childContext',
  'componentNames',
  'componentStack',
  'facturX',
  'facturXRelationship',
  'familyKey',
  'finishTemplate',
  'fontStretchValue',
  'fontVariationsValue',
  'fontWeightValue',
  'fonts',
  'formatStack',
  'hostName',
  'imageSource',
  'innermostComponent',
  'isHostType',
  'isImagePath',
  'loadTailwind',
  'parsePath',
  'remoteAllowed',
  'remoteRefused',
  'rootContext',
  'tagName',
  'templateError',
  'templateFunctions',
  'trimTemplateText',
  'validateTree',
  'withoutRepeats',
];

/** The types of the main entry, but the props types (`@inkoxide/tree/props`). */
const TYPES = [
  'CanvasGradient',
  'CanvasTextOptions',
  'CommentNode',
  'Common',
  'Container',
  'DrawingFont',
  'EmojiSource',
  'EncryptionOptions',
  'FacturXOptions',
  'FacturXProfile',
  'FieldCommon',
  'Flow',
  'FontFace',
  'HostContext',
  'HostType',
  'ImageSource',
  'Instance',
  'Located',
  'Node',
  'OutputOptions',
  'Painter',
  'PdfALevel',
  'PdfUaPart',
  'PendingImage',
  'PropsByComponent',
  'RemoteImagePolicy',
  'RenderPage',
  'RenderProp',
  'ResolvedFont',
  'SchemaProblem',
  'TemplateErrorInit',
  'TemplateErrorKind',
  'TemplateFunctions',
  'TextInstance',
  'Trace',
  'TreeOptions',
  'TreeResult',
];

const SRC = new URL('../src/', import.meta.url);

/**
 * The names `file` exports, values and types: those it re-exports by name, those of the files it
 * re-exports whole (`export * from`), and those it declares.
 */
function exported(file: URL): string[] {
  const code = readFileSync(file, 'utf8').replace(/^\s*\/\/.*$/gm, '');
  const names = listed(code);
  for (const m of code.matchAll(/^export \* from '(\.\/[\w/.-]+)';$/gm))
    names.push(...exported(new URL(m[1] ?? '', file)));
  for (const m of code.matchAll(/^export (?:const|function|class|interface|type) (\w+)/gm))
    names.push(m[1] ?? '');
  return sorted(new Set(names));
}

/** What Inkoxide's own front ends share; not under semver. */
const INTERNAL = [
  'assignProp',
  'classNames',
  'listenerError',
  'namesInTrace',
  'nodeLinks',
  'renderWithTrace',
  'textInElement',
];

/** The names every front end exports as they are (`export * from '@inkoxide/tree/common'`). */
const COMMON = [
  'CanvasGradient',
  'CanvasTextOptions',
  'EmojiSource',
  'FacturXOptions',
  'FacturXProfile',
  'Located',
  'PAGE_SIZES',
  'Painter',
  'StyleSheet',
  'TemplateError',
  'TemplateErrorKind',
  'TreeOptions',
  'TreeResult',
  'facturX',
  'facturXRelationship',
];

describe('exports', () => {
  it('@inkoxide/tree exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(VALUES));
  });

  it('@inkoxide/tree exports exactly the documented types, and the props types', () => {
    const props = declared(SRC, 'props.ts');
    expect(exported(new URL('index.ts', SRC))).toEqual(sorted(new Set([...VALUES, ...TYPES, ...props])));
  });

  it('@inkoxide/tree/internal exports what the front ends share', () => {
    expect(sorted(Object.keys(internal))).toEqual(sorted(INTERNAL));
    expect(declared(SRC, 'internal.ts')).toEqual(sorted(INTERNAL));
  });

  it('@inkoxide/tree/common exports names of the main entry, which every front end exports', () => {
    expect(declared(SRC, 'common.ts')).toEqual(sorted(COMMON));
    expect(Object.keys(common).filter((name) => !(name in main))).toEqual([]);
  });

  it('has each name in the README', () => {
    const readme = readFileSync(new URL('../README.md', import.meta.url), 'utf8');
    expect([...VALUES, ...TYPES].filter((name) => !readme.includes(`\`${name}\``))).toEqual([]);
  });

  it('types the props of every component, and of nothing else', () => {
    expectTypeOf<keyof PropsByComponent>().toEqualTypeOf<keyof typeof HOST | keyof typeof FIELD_KINDS>();
    expect(main.COMPONENTS).toEqual([...Object.keys(main.HOST), ...Object.keys(main.FIELD_KINDS)]);
  });

  it('@inkoxide/tree/props lists only types the main entry exports', () => {
    const props = declared(SRC, 'props.ts');
    expect(props.length).toBeGreaterThan(50);
    expect(props.filter((name) => name in main)).toEqual([]);
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({
      '.': './src/index.ts',
      './common': './src/common.ts',
      './internal': './src/internal.ts',
      './props': './src/props.ts',
    });
  });
});
