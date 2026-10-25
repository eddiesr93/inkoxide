// @vitest-environment happy-dom
/**
 * The browser helpers, with the render mocked: each render the helpers start waits until the test
 * settles it.
 */
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';
import { createApp, defineComponent, h, nextTick, ref, shallowRef, type VNode } from 'vue';

import { BlobProvider, type PdfState, PdfViewer, usePdf, type UsePdfResult } from '../src/helpers.ts';

interface Started {
  doc: unknown;
  opts: { signal?: AbortSignal };
  resolve(blob: Blob): void;
  reject(e: unknown): void;
}

const renders = vi.hoisted((): Started[] => []);

vi.mock('../src/browser-render.ts', () => ({
  browserRenderer: {
    renderToBlob: (doc: unknown, opts: Started['opts']) =>
      new Promise<Blob>((resolve, reject) => renders.push({ doc, opts, resolve, reject })),
  },
}));

const started = (n: number): Started => renders[n] as Started;
/** Lets the promise callbacks and Vue's updates run. */
const settle = async () => {
  await new Promise((r) => setTimeout(r, 0));
  await nextTick();
};

let host: HTMLElement;
let revoked: MockInstance<typeof URL.revokeObjectURL>;
let unmount: (() => void) | undefined;

function mountWith(render: () => VNode | VNode[] | null | undefined, setup?: () => void) {
  const app = createApp(defineComponent({ setup: () => (setup?.(), render) }));
  app.mount(host);
  unmount = () => app.unmount();
}

beforeEach(() => {
  renders.length = 0;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:pdf-${++made}`);
  revoked = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  host = document.createElement('div');
});

afterEach(() => {
  unmount?.();
  unmount = undefined;
  vi.restoreAllMocks();
});

describe('usePdf', () => {
  it('renders at once and when its document changes, shows the last render that started, and revokes what it replaces', async () => {
    const n = ref(1);
    let pdf!: UsePdfResult;
    mountWith(
      () => null,
      () => {
        pdf = usePdf(() => h('span', { n: n.value }));
      },
    );
    expect(renders).toHaveLength(1);
    expect(pdf.loading.value).toBe(true);
    n.value = 2;
    await nextTick();
    expect(renders).toHaveLength(2);
    expect(started(0).opts.signal?.aborted).toBe(true);
    const fresh = new Blob(['fresh']);
    started(0).resolve(new Blob(['stale']));
    started(1).resolve(fresh);
    await settle();
    expect(pdf.blob.value).toBe(fresh);
    expect(pdf.url.value).toBe('blob:pdf-1');
    expect(pdf.loading.value).toBe(false);
    pdf.update();
    started(2).resolve(new Blob(['newer']));
    await settle();
    expect(revoked).toHaveBeenCalledWith('blob:pdf-1');
    pdf.update();
    unmount?.();
    unmount = undefined;
    expect(started(3).opts.signal?.aborted).toBe(true);
    expect(revoked).toHaveBeenCalledWith('blob:pdf-2');
  });

  it('does not render again for the same component and props', async () => {
    const data = { a: 1 };
    const tick = ref(0);
    mountWith(
      () => null,
      () => {
        usePdf(() => (tick.value, h('span', data)));
      },
    );
    tick.value++;
    await nextTick();
    expect(renders).toHaveLength(1);
  });

  it('renders a functional component, and the components a getter gives', async () => {
    const Invoice = (props: { n?: number }) => h('span', props.n);
    const Receipt = { name: 'Receipt', props: { n: Number }, render: () => h('span') };
    const Other = { name: 'Other', props: { n: Number }, render: () => h('span') };
    const which = shallowRef<object>(Receipt);
    mountWith(
      () => null,
      () => {
        usePdf(Invoice);
        usePdf(() => which.value);
      },
    );
    expect(started(0).doc).toBe(Invoice);
    expect(started(1).doc).toBe(Receipt);
    which.value = Other;
    await nextTick();
    expect(started(2).doc).toBe(Other);
  });

  it('keeps the last PDF and gives the error of a render that fails', async () => {
    let pdf!: UsePdfResult;
    mountWith(
      () => null,
      () => {
        pdf = usePdf(h('span'));
      },
    );
    const blob = new Blob(['pdf']);
    started(0).resolve(blob);
    await settle();
    pdf.update();
    const error = new Error('the render failed');
    started(1).reject(error);
    await settle();
    expect(pdf.blob.value).toBe(blob);
    expect(pdf.error.value).toBe(error);
  });
});

describe('PdfViewer', () => {
  it('shows its fallback, then the PDF in a frame', async () => {
    mountWith(() =>
      h(PdfViewer, { document: h('span'), showToolbar: false }, { fallback: () => h('p', 'Rendering…') }),
    );
    expect(host.innerHTML).toBe('<p>Rendering…</p>');
    started(0).resolve(new Blob(['pdf']));
    await settle();
    expect(host.innerHTML).toBe('<iframe title="PDF" src="blob:pdf-1#toolbar=0"></iframe>');
  });
});

describe('BlobProvider', () => {
  it('hands the render state to its slot', async () => {
    mountWith(() =>
      h(
        BlobProvider,
        { document: h('span') },
        { default: (pdf: PdfState) => h('a', { href: pdf.url ?? '' }, pdf.loading ? 'wait' : 'get') },
      ),
    );
    expect(host.textContent).toBe('wait');
    started(0).resolve(new Blob(['pdf']));
    await settle();
    expect(host.innerHTML).toBe('<a href="blob:pdf-1">get</a>');
  });
});
