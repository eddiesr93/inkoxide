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
const h = (type: unknown, props: Record<string, unknown> | null = null, children: unknown = null) => ({
  __v_isVNode: true,
  type,
  key: null,
  props,
  children,
  // The instance it mounts: null until then.
  component: null,
});

describe('sameDocument', () => {
  it('compares React elements and Vue VNodes by type, key and props', () => {
    expect(sameDocument(createElement(Invoice, { invoice }), createElement(Invoice, { invoice }))).toBe(true);
    expect(
      sameDocument(createElement(Invoice, { invoice }), createElement(Invoice, { invoice: { number: 2 } })),
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
    expect(sameDocument(h(Invoice, { invoice }), h(Receipt, { invoice }))).toBe(false);
    expect(
      sameDocument(
        h(Invoice, null, () => []),
        h(Invoice, null, () => []),
      ),
    ).toBe(false);
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

  it('compares data made again by its contents, and anything else by identity', () => {
    const data = () => ({ number: 'A-1', lines: [{ name: 'Tea', price: 3 }], notes: null });
    const doc = (invoice: unknown) => ({ component: Invoice, inputs: { invoice } });
    // Data a template builds as it renders: the same document, or a helper renders without end.
    expect(sameDocument(doc(data()), doc(data()))).toBe(true);
    expect(
      sameDocument(createElement(Invoice, { invoice: data() }), createElement(Invoice, { invoice: data() })),
    ).toBe(true);
    expect(sameDocument(doc(data()), doc({ ...data(), lines: [{ name: 'Tea', price: 4 }] }))).toBe(false);
    expect(sameDocument(doc(data()), doc({ ...data(), lines: [] }))).toBe(false);
    expect(sameDocument(doc([1, 2]), doc({ 0: 1, 1: 2 }))).toBe(false);
    // Not data: each is the same only as itself.
    expect(sameDocument(doc(new Date(0)), doc(new Date(0)))).toBe(false);
    expect(sameDocument(doc(new Uint8Array(2)), doc(new Uint8Array(2)))).toBe(false);
    expect(
      sameDocument(
        doc(() => 1),
        doc(() => 1),
      ),
    ).toBe(false);
    const bytes = new Uint8Array(2);
    expect(sameDocument(doc({ logo: bytes }), doc({ logo: bytes }))).toBe(true);
    // Data that holds itself ends: it is another document.
    const loop: Record<string, unknown> = {};
    loop.self = loop;
    const other: Record<string, unknown> = {};
    other.self = other;
    expect(sameDocument(doc(loop), doc(other))).toBe(false);
  });

  it('compares components given alone by identity', () => {
    // Vue components are objects, which two components declaring the same props must not equal.
    const props = { invoice: Object };
    const A = { name: 'A', props, setup: () => null };
    const B = { name: 'B', props, setup: () => null };
    expect(sameDocument(A, A)).toBe(true);
    expect(sameDocument(A, B)).toBe(false);
    expect(sameDocument({}, {})).toBe(false);
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
