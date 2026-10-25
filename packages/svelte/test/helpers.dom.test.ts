/**
 * The browser helpers, with the render mocked: each render they start waits until the test
 * settles it.
 */
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import PdfViewer from '../src/browser/PdfViewer.svelte';
import type { PdfState } from '../src/helpers.svelte.ts';
import Probe from './fixtures/Probe.svelte';

interface Started {
  opts: { signal?: AbortSignal };
  resolve(blob: Blob): void;
  reject(e: unknown): void;
}

const renders = vi.hoisted((): Started[] => []);

vi.mock('../src/browser-render.ts', () => ({
  browserRenderer: {
    renderToBlob: (_doc: unknown, opts: Started['opts']) =>
      new Promise<Blob>((resolve, reject) => renders.push({ opts, resolve, reject })),
  },
}));

const Doc = () => {};
const started = (n: number): Started => renders[n] as Started;
const settle = async () => {
  await new Promise((r) => setTimeout(r, 0));
  flushSync();
};

let revoked: MockInstance<typeof URL.revokeObjectURL>;
let app: ReturnType<typeof mount> | undefined;
let target: HTMLElement;

beforeEach(() => {
  renders.length = 0;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:pdf-${++made}`);
  revoked = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  target = document.createElement('div');
});

afterEach(() => {
  if (app) void unmount(app);
  app = undefined;
  vi.restoreAllMocks();
});

describe('createPdf', () => {
  it('renders, shows the last render that started, and revokes what it replaces', async () => {
    let pdf!: PdfState;
    app = mount(Probe, {
      target,
      props: { document: { component: Doc as never }, seen: (p: PdfState) => (pdf = p) },
    });
    flushSync();
    expect(renders).toHaveLength(1);
    expect(pdf.loading).toBe(true);
    pdf.update();
    expect(started(0).opts.signal?.aborted).toBe(true);
    const fresh = new Blob(['fresh']);
    started(0).resolve(new Blob(['stale']));
    started(1).resolve(fresh);
    await settle();
    expect(pdf.blob).toBe(fresh);
    expect(pdf.url).toBe('blob:pdf-1');
    pdf.update();
    started(2).resolve(new Blob(['newer']));
    await settle();
    expect(revoked).toHaveBeenCalledWith('blob:pdf-1');
    pdf.update();
    void unmount(app);
    app = undefined;
    expect(started(3).opts.signal?.aborted).toBe(true);
    expect(revoked).toHaveBeenCalledWith('blob:pdf-2');
  });

  it('gives the error of a render that fails', async () => {
    let pdf!: PdfState;
    app = mount(Probe, {
      target,
      props: { document: { component: Doc as never }, seen: (p: PdfState) => (pdf = p) },
    });
    flushSync();
    const error = new Error('the render failed');
    started(0).reject(error);
    await settle();
    expect(pdf.error).toBe(error);
    expect(pdf.loading).toBe(false);
  });
});

describe('PdfViewer', () => {
  it('shows the PDF in a frame once it is rendered', async () => {
    app = mount(PdfViewer, { target, props: { document: { component: Doc as never }, showToolbar: false } });
    flushSync();
    expect(target.querySelector('iframe')).toBeNull();
    started(0).resolve(new Blob(['pdf']));
    await settle();
    expect(target.querySelector('iframe')?.getAttribute('src')).toBe('blob:pdf-1#toolbar=0');
  });
});
