/**
 * The public API of @inkoxide/tree, which every front end builds on: the names it exports, and the
 * props types `@inkoxide/tree/props` gives the front ends. A name added here is API under semver,
 * so it needs its place in the README first.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, expectTypeOf, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
import type { FIELD_KINDS, HOST, PropsByComponent } from '../src/index.ts';
import * as main from '../src/index.ts';
import { declared, sorted } from './entries.ts';

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
  'assignProp',
  'buildDocument',
  'checkElement',
  'checkText',
  'checkTree',
  'childContext',
  'classNames',
  'componentNames',
  'componentStack',
  'dedupe',
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
  'namesInTrace',
  'nodeLinks',
  'parsePath',
  'remoteAllowed',
  'remoteRefused',
  'renderWithTrace',
  'rootContext',
  'tagName',
  'templateError',
  'templateFunctions',
  'trimTemplateText',
  'validateTree',
];

/** The types of the main entry, but the props types (`@inkoxide/tree/props`). */
const TYPES = [
  'BuildHooks',
  'CanvasGradient',
  'CanvasTextOptions',
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
  const names: string[] = [];
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
  for (const m of code.matchAll(/^export \* from '(\.\/[\w/.-]+)';$/gm))
    names.push(...exported(new URL(m[1] ?? '', file)));
  for (const m of code.matchAll(/^export (?:const|function|class|interface|type) (\w+)/gm))
    names.push(m[1] ?? '');
  return sorted(new Set(names));
}

describe('exports', () => {
  it('@inkoxide/tree exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(VALUES));
  });

  it('@inkoxide/tree exports exactly the documented types, and the props types', () => {
    const props = declared(SRC, 'props.ts');
    expect(exported(new URL('index.ts', SRC))).toEqual(sorted(new Set([...VALUES, ...TYPES, ...props])));
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
    expect(pkg.exports).toEqual({ '.': './src/index.ts', './props': './src/props.ts' });
  });
});
