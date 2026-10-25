import * as react from '@inkoxide/react';
/**
 * Every front end exports the same names, from the same entries: what the guide documents for
 * one is there in each.
 */
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
