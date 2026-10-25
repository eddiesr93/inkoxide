import { FontRegistry } from '@inkoxide/tree';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import View from '../src/components/View.svelte';
import { renderTemplate, TemplateError } from '../src/index.ts';
import Holder from './fixtures/Holder.svelte';
import Misplaced from './fixtures/Misplaced.svelte';
import Paged from './fixtures/Paged.svelte';
import Raw from './fixtures/Raw.svelte';
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

  it('reports what a template throws, with the components it is in', () => {
    const e = thrown(() => renderTemplate(Holder, { fonts }));
    expect(e.message).toBe('The template threw: boom');
    expect(e.componentStack).toBe(
      '\n    in Throws\n    in Text\n    in Page\n    in Document\n    in Holder',
    );
  });

  it('refuses an Inkoxide element its component did not render', () => {
    const e = thrown(() => renderTemplate(Raw, { fonts }));
    expect(e.message).toBe(
      '<ink-view> is rendered by the View component: write <View>, not the element, and no Inkoxide markup in {@html}.',
    );
    expect(e.componentStack).toBe('\n    in Page\n    in Document');
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
