import * as react from '@inkoxide/react';
/**
 * Every front end exports the same names, from the same entries: what the guide documents for
 * one is there in each.
 */
// Angular's libraries are partially compiled; outside an Angular build they need the JIT compiler.
import '@angular/compiler';
import * as reactBrowser from '@inkoxide/react/browser';
import * as reactTemplates from '@inkoxide/react/templates';
import * as vue from '@inkoxide/vue';
import * as vueBrowser from '@inkoxide/vue/browser';
import * as vueTemplates from '@inkoxide/vue/templates';
import { describe, expect, it } from 'vitest';

const names = (m: object) => Object.keys(m).sort();

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
    expect(names(browser)).toEqual(angularNames(names(reactBrowser)));
    expect(names(templates)).toEqual(names(reactTemplates));
  });
});
