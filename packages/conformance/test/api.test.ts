// Every front end exports the same names, from the same entries: what the guide documents for
// one is there in each.
import * as react from '@inkoxide/react';
// Angular's libraries are partially compiled; outside an Angular build they need the JIT compiler.
import '@angular/compiler';
import * as reactBrowser from '@inkoxide/react/browser';
import * as reactTemplates from '@inkoxide/react/templates';
import * as vue from '@inkoxide/vue';
import * as vueBrowser from '@inkoxide/vue/browser';
import * as vueTemplates from '@inkoxide/vue/templates';
import { describe, expect, it } from 'vitest';

/** What only @inkoxide/react exports: what it exported before 1.0, for the code that uses it. */
const REACT_COMPAT = new Set([
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
]);

const sorted = (list: string[]) => [...list].sort();

const names = (m: object) =>
  Object.keys(m)
    .filter((n) => !REACT_COMPAT.has(n))
    .sort();

describe.each([['Vue', vue, vueBrowser, vueTemplates]] as const)('%s', (_, main, browser, templates) => {
  it('exports the names of @inkoxide/react', () => {
    expect(names(main)).toEqual(names(react));
  });

  it('exports the names of @inkoxide/react/browser from its browser entry', () => {
    expect(names(browser)).toEqual(names(reactBrowser));
  });

  it('exports the names of @inkoxide/react/templates from its templates entry', () => {
    expect(names(templates)).toEqual(names(reactTemplates));
  });
});

/** React's names that Angular writes its own way: components are elements, hooks are inject functions. */
const ANGULAR_OWN = { usePage: 'injectPage', usePdf: 'injectPdf' } as const;
const REACT_ONLY_IN_ANGULAR = new Set(['BlobProvider', 'PdfDownloadLink', 'PdfViewer']);
/** Angular's browser components are directives on the elements they drive. */
const ANGULAR_DIRECTIVES = ['InkBlobProvider', 'InkPdfDownload', 'InkPdfViewer'];

describe('Angular', () => {
  const components = new Set(
    Object.keys(react).filter(
      (n) =>
        /^[A-Z]([a-z]|$)/.test(n) &&
        n !== 'Font' &&
        n !== 'RenderError' &&
        n !== 'StyleSheet' &&
        n !== 'TemplateError',
    ),
  );
  const angularNames = (names: string[]) =>
    names
      .filter((n) => !components.has(n) && !REACT_ONLY_IN_ANGULAR.has(n))
      .map((n): string => ANGULAR_OWN[n as keyof typeof ANGULAR_OWN] ?? n)
      .concat(['INK_PAGE'])
      .sort();

  it('exports the names of @inkoxide/react but the components, with inject functions for hooks', async () => {
    const angular = await import('@inkoxide/angular');
    expect(names(angular)).toEqual(angularNames(names(react)));
  });

  it('does the same in its browser and templates entries', async () => {
    const browser = await import('@inkoxide/angular/browser');
    const templates = await import('@inkoxide/angular/templates');
    expect(names(browser)).toEqual(sorted([...angularNames(names(reactBrowser)), ...ANGULAR_DIRECTIVES]));
    expect(names(templates)).toEqual(names(reactTemplates));
  });
});

describe('Svelte', () => {
  // Svelte's way to keep state in a function: `createPdf`, with runes, for React's hook.
  const svelteNames = (list: string[]) => list.map((n) => (n === 'usePdf' ? 'createPdf' : n)).sort();

  it('exports the names of @inkoxide/react, from each entry', async () => {
    const [main, browser, templates] = await Promise.all([
      import('@inkoxide/svelte'),
      import('@inkoxide/svelte/browser'),
      import('@inkoxide/svelte/templates'),
    ]);
    expect(names(main)).toEqual(names(react));
    expect(names(browser)).toEqual(svelteNames(names(reactBrowser)));
    expect(names(templates)).toEqual(names(reactTemplates));
  });
});

describe('components', () => {
  it('every front end has every component of @inkoxide/tree', async () => {
    const { COMPONENTS, TAGS } = await import('@inkoxide/tree');
    const svelte = await import('@inkoxide/svelte');
    for (const entry of [react, vue, svelte]) {
      expect(COMPONENTS.filter((name) => !(name in entry))).toEqual([]);
    }
    // Angular writes them as elements.
    const { tagName } = await import('@inkoxide/tree');
    expect(COMPONENTS.filter((name) => !TAGS.has(tagName(name)))).toEqual([]);
  });
});
