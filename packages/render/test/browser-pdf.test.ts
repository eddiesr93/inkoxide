import { describe, expect, it, vi } from 'vitest';

import { PdfRun, sameDocument } from '../src/browser-pdf.ts';

const Invoice = () => null;
const Receipt = () => null;
const invoice = { number: 1 };

// What React's `createElement` and Vue's `h` make, as far as the comparison reads them.
const createElement = (type: unknown, { key = null, ...props }: Record<string, unknown> = {}) => ({
  $$typeof: Symbol.for('react.transitional.element'),
  type,
  key,
  props,
});
const h = (type: unknown, props: Record<string, unknown> | null = null) => ({
  __v_isVNode: true,
  type,
  key: null,
  props,
});

describe('sameDocument', () => {
  it('compares React elements and Vue VNodes by type, key and props', () => {
    expect(sameDocument(createElement(Invoice, { invoice }), createElement(Invoice, { invoice }))).toBe(true);
    expect(
      sameDocument(createElement(Invoice, { invoice }), createElement(Invoice, { invoice: { number: 1 } })),
    ).toBe(false);
    expect(sameDocument(createElement(Invoice, { invoice }), createElement(Receipt, { invoice }))).toBe(
      false,
    );
    expect(
      sameDocument(createElement(Invoice, { invoice, key: 'a' }), createElement(Invoice, { invoice })),
    ).toBe(false);
    expect(sameDocument(h(Invoice, { invoice }), h(Invoice, { invoice }))).toBe(true);
    expect(sameDocument(h(Invoice), h(Invoice, {}))).toBe(true);
    expect(sameDocument(h(Invoice, { invoice }), h(Invoice, { invoice, page: 2 }))).toBe(false);
  });

  it('compares components with their props or inputs', () => {
    expect(
      sameDocument({ component: Invoice, props: { invoice } }, { component: Invoice, props: { invoice } }),
    ).toBe(true);
    expect(
      sameDocument({ component: Invoice, inputs: { invoice } }, { component: Invoice, inputs: { invoice } }),
    ).toBe(true);
    expect(sameDocument({ component: Invoice }, { component: Invoice, props: {} })).toBe(true);
    expect(
      sameDocument({ component: Invoice, props: { invoice } }, { component: Receipt, props: { invoice } }),
    ).toBe(false);
    expect(sameDocument({ component: Invoice, props: { invoice } }, { component: Invoice, props: {} })).toBe(
      false,
    );
    expect(
      sameDocument({ component: Invoice, props: { a: undefined } }, { component: Invoice, props: { b: 1 } }),
    ).toBe(false);
    expect(sameDocument(Invoice, Invoice)).toBe(true);
    expect(sameDocument(Invoice, { component: Invoice })).toBe(false);
  });
});

describe('PdfRun.follow', () => {
  it('renders a document only when it is another one than the last', async () => {
    const render = vi.fn(async () => new Blob(['%PDF']));
    globalThis.URL.createObjectURL ??= () => 'blob:x';
    globalThis.URL.revokeObjectURL ??= () => {};
    const run = new PdfRun(render, () => {});
    run.follow(undefined);
    run.follow({ component: Invoice, props: { invoice } });
    run.follow({ component: Invoice, props: { invoice } });
    expect(render).toHaveBeenCalledTimes(1);
    run.follow({ component: Invoice, props: { invoice: { number: 2 } } });
    expect(render).toHaveBeenCalledTimes(2);
    run.stop();
  });
});
