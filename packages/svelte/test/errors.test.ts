import { FontRegistry } from '@inkoxide/tree';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import View from '../src/components/View.svelte';
import { renderTemplate, TemplateError } from '../src/index.ts';
import Misplaced from './fixtures/Misplaced.svelte';
import Paged from './fixtures/Paged.svelte';
import Throws from './fixtures/Throws.svelte';
import Unpaged from './fixtures/Unpaged.svelte';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

describe('errors', () => {
  it('refuses an element in the wrong place, with the elements above it', () => {
    const e = thrown(() => renderTemplate(Misplaced, { fonts }));
    expect(e.message).toMatch(/^<View> cannot be inside <Text>/);
    expect(e.componentStack).toBe('\n    in View\n    in Text\n    in Page\n    in Document');
  });

  it('reports what a template throws', () => {
    expect(thrown(() => renderTemplate(Throws, { fonts })).message).toBe('The template threw: boom');
  });

  it('says where usePage() works', () => {
    expect(thrown(() => renderTemplate(Unpaged, { fonts })).message).toMatch(
      /^usePage\(\) works only inside a render snippet/,
    );
  });

  it('says that a component renders only inside an Inkoxide render', () => {
    expect(() => render(View).body).toThrow(/<view> renders only inside an Inkoxide render/);
  });
});

describe('render snippet', () => {
  it('gives usePage the page, and the content the contexts around the element', () => {
    expect(JSON.stringify(renderTemplate(Paged, { fonts, pageCount: 1 }).document)).toContain('p. 1');
  });
});
