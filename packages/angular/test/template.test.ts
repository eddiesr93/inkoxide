import '@angular/compiler';
import { Component, CUSTOM_ELEMENTS_SCHEMA, Input } from '@angular/core';
import { FontRegistry, loadTailwind } from '@inkoxide/tree';
import { beforeAll, describe, expect, it } from 'vitest';

import { injectPage, renderTemplate, renderToBuffer, TemplateError } from '../src/index.ts';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

beforeAll(async () => {
  await loadTailwind();
});

function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

@Component({
  selector: 'app-line',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `<ink-text class="font-bold">{{ label }}</ink-text>`,
})
// Decorated inputs: signal inputs need the AOT compiler, and these tests compile just in time.
class Line {
  @Input({ required: true }) label!: string;
}

@Component({
  selector: 'app-doc',
  imports: [Line],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document title="T">
      <ink-page size="A5" [style]="{ padding: 20 }">
        <ink-view class="flex-row gap-2">
          <ink-text hyphenate lang="en" [hyphenationPenalty]="900"
            >Hello <ink-link href="https://x.y">link</ink-link> end</ink-text
          >
        </ink-view>
        @for (l of lines; track l) {
          <app-line [label]="l" />
        }
        @if (note) {
          <ink-text>{{ note }}</ink-text>
        }
        <ink-text-field name="email" />
        <ink-footer>
          <ink-text [render]="footer" />
          <ng-template #footer let-page>{{ page.pageNumber }} / {{ page.totalPages }}</ng-template>
        </ink-footer>
      </ink-page>
    </ink-document>
  `,
})
class Doc {
  @Input() lines = ['a', 'b'];
  @Input() note?: string;
}

describe('templates', () => {
  it('reads attributes, property bindings, classes, styles, blocks and component hosts', () => {
    const tree = renderTemplate({ component: Doc, inputs: { note: 'Note' } }, { fonts, pageCount: 2 });
    const page = tree.document.pages[0];
    expect(page?.margin).toEqual({ top: 20, right: 20, bottom: 20, left: 20 });
    const json = JSON.stringify(tree.document);
    expect(json).toContain('"hyphenate":true');
    expect(json).toContain('"hyphenationPenalty":900');
    expect(json).toContain('"text":"Hello "');
    expect(json).toContain('"text":"Note"');
    expect(json).toContain('"name":"email","kind":"text"');
    expect(json).toContain('2 / 2');
    const texts = (page?.children ?? []).filter((n) => n.type === 'text');
    expect(texts.map((t) => (t as { runs: { text: string }[] }).runs[0]?.text)).toEqual(['a', 'b', 'Note']);
  });

  it('names the components above a misplaced element', () => {
    @Component({ selector: 'app-inner', schemas: [CUSTOM_ELEMENTS_SCHEMA], template: '<ink-view />' })
    class Inner {}
    @Component({
      selector: 'app-outer',
      imports: [Inner],
      schemas: [CUSTOM_ELEMENTS_SCHEMA],
      template: '<ink-document><ink-page><ink-text><app-inner /></ink-text></ink-page></ink-document>',
    })
    class Outer {}
    const e = thrown(() => renderTemplate(Outer, { fonts }));
    expect(e.message).toMatch(/^<View> cannot be inside <Text>/);
    expect(e.componentStack).toBe('\n    in View\n    in Inner\n    in Outer');
  });

  it('refuses an element that is not a component of Inkoxide', () => {
    @Component({
      selector: 'app-div',
      schemas: [CUSTOM_ELEMENTS_SCHEMA],
      template: '<ink-document><ink-page><div></div></ink-page></ink-document>',
    })
    class WithDiv {}
    expect(thrown(() => renderTemplate(WithDiv, { fonts })).message).toMatch(
      /^<div> is not an Inkoxide component/,
    );
  });

  it('gives injectPage the page inside a render template, and refuses it elsewhere', () => {
    @Component({ selector: 'app-num', template: '{{ text }}' })
    class Num {
      readonly text = `p. ${injectPage().pageNumber}`;
    }
    @Component({
      selector: 'app-paged',
      imports: [Num],
      schemas: [CUSTOM_ELEMENTS_SCHEMA],
      template:
        '<ink-document><ink-page><ink-text [render]="t" /><ng-template #t><app-num /></ng-template></ink-page></ink-document>',
    })
    class Paged {}
    expect(JSON.stringify(renderTemplate(Paged, { fonts, pageCount: 1 }).document)).toContain('p. 1');
    @Component({
      selector: 'app-bad',
      imports: [Num],
      schemas: [CUSTOM_ELEMENTS_SCHEMA],
      template: '<ink-document><ink-page><ink-text><app-num /></ink-text></ink-page></ink-document>',
    })
    class Bad {}
    expect(thrown(() => renderTemplate(Bad, { fonts })).message).toMatch(
      /^injectPage\(\) works only inside a render template/,
    );
  });

  it('reports what a template throws', () => {
    @Component({ selector: 'app-throws', template: '{{ boom() }}' })
    class Throws {
      boom(): string {
        throw new Error('boom');
      }
    }
    expect(thrown(() => renderTemplate(Throws, { fonts })).message).toBe('The template threw: boom');
  });
});

describe('renderToBuffer', () => {
  it('renders a component with its inputs', async () => {
    const pdf = await renderToBuffer(
      { component: Doc, inputs: { lines: ['x'] } },
      { creationDate: '2026-10-25T00:00:00Z' },
    );
    expect(new TextDecoder().decode(pdf.subarray(0, 5))).toBe('%PDF-');
  });
});
