/**
 * The public API of @inkoxide/tree, which every front end builds on: the names it exports, and the
 * props types `@inkoxide/tree/props` gives the front ends. A name added here is API under semver,
 * so it needs its place in the README first.
 */
import { describe, expect, it } from 'vitest';

import pkg from '../package.json' with { type: 'json' };
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
  'buildDocument',
  'checkElement',
  'checkText',
  'checkTree',
  'childContext',
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
  'parsePath',
  'remoteAllowed',
  'remoteRefused',
  'rootContext',
  'tagName',
  'templateError',
  'templateFunctions',
  'trimTemplateText',
  'validateTree',
];

describe('exports', () => {
  it('@inkoxide/tree exports exactly the documented names', () => {
    expect(sorted(Object.keys(main))).toEqual(sorted(VALUES));
  });

  it('@inkoxide/tree/props lists only types the main entry exports', () => {
    const props = declared(new URL('../src/', import.meta.url), 'props.ts');
    expect(props.length).toBeGreaterThan(50);
    expect(props.filter((name) => name in main)).toEqual([]);
  });

  it('has an entry in package.json for each', () => {
    expect(pkg.exports).toEqual({ '.': './src/index.ts', './props': './src/props.ts' });
  });
});
