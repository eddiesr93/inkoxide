/**
 * @vitest-environment happy-dom
 *
 * `usePdf`, the hook of `PdfDownloadLink`, `PdfViewer` and `BlobProvider`, with a render the test
 * settles itself: which render's PDF it shows, and when it stops renders and revokes the object
 * URLs it made. Then the components: when they render again, and what they show.
 */
import { Activity, act, type ReactElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { BrowserRenderOptions } from '../src/browser-api.ts';
import { BlobProvider, PdfViewer, usePdf, type UsePdfResult } from '../src/helpers.tsx';

interface Started {
  opts: BrowserRenderOptions;
  resolve(blob: Blob): void;
  reject(e: unknown): void;
}

/** The renders the hook started, oldest first. */
const renders = vi.hoisted((): Started[] => []);

vi.mock('../src/browser-api.ts', () => ({
  renderToBlob: (_element: ReactElement, opts: BrowserRenderOptions) =>
    new Promise<Blob>((resolve, reject) => renders.push({ opts, resolve, reject })),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** What the hook returned, render by render. */
const seen: UsePdfResult[] = [];
const last = (): UsePdfResult => seen[seen.length - 1] as UsePdfResult;
const started = (n: number): Started => renders[n] as Started;

function Probe({ element }: { element?: ReactElement }) {
  seen.push(usePdf(element));
  return null;
}

const doc = <span />;
let root: Root;
let host: HTMLElement;
let revoked: MockInstance<typeof URL.revokeObjectURL>;

beforeEach(() => {
  renders.length = 0;
  seen.length = 0;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:pdf-${++made}`);
  revoked = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  host = document.createElement('div');
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  vi.restoreAllMocks();
});

describe('usePdf', () => {
  it('shows the PDF of the last render that started, and revokes each URL it replaces', async () => {
    await act(async () => root.render(<Probe element={doc} />));
    expect(renders).toHaveLength(1);
    expect(last()).toMatchObject({ blob: null, url: null, loading: true, error: null });
    // A render that starts stops the one still running, whose PDF is then not shown.
    await act(async () => last().update());
    expect(started(0).opts.signal?.aborted).toBe(true);
    const fresh = new Blob(['fresh']);
    await act(async () => {
      started(0).resolve(new Blob(['stale']));
      started(1).resolve(fresh);
    });
    expect(last()).toMatchObject({ blob: fresh, loading: false, error: null });
    const first = last().url;
    expect(first).toBe('blob:pdf-1');
    await act(async () => last().update());
    await act(async () => started(2).resolve(new Blob(['newer'])));
    expect(revoked).toHaveBeenCalledWith(first);
    const second = last().url;
    expect(second).toBe('blob:pdf-2');
    // Unmounting stops the running render and revokes the URL.
    await act(async () => last().update());
    await act(async () => root.unmount());
    expect(started(3).opts.signal?.aborted).toBe(true);
    expect(revoked).toHaveBeenCalledWith(second);
    root = createRoot(document.createElement('div'));
  });

  it('keeps the last PDF and gives the error of a render that fails', async () => {
    await act(async () => root.render(<Probe element={doc} />));
    const blob = new Blob(['pdf']);
    await act(async () => started(0).resolve(blob));
    const { url } = last();
    await act(async () => last().update());
    const error = new Error('the render failed');
    await act(async () => started(1).reject(error));
    expect(last()).toMatchObject({ blob, url, loading: false, error });
  });

  it('renders nothing without an element, until update(element)', async () => {
    await act(async () => root.render(<Probe />));
    expect(renders).toHaveLength(0);
    expect(last().loading).toBe(false);
    await act(async () => last().update(doc));
    expect(renders).toHaveLength(1);
    expect(last().loading).toBe(true);
  });

  it('returns no revoked URL after its effects are cleaned up without an unmount', async () => {
    const view = (mode: 'visible' | 'hidden') => (
      <Activity mode={mode}>
        <Probe element={doc} />
      </Activity>
    );
    await act(async () => root.render(view('visible')));
    await act(async () => started(0).resolve(new Blob(['pdf'])));
    const { url } = last();
    // A hidden Activity cleans up the effects and keeps the state.
    await act(async () => root.render(view('hidden')));
    expect(revoked).toHaveBeenCalledWith(url);
    await act(async () => root.render(view('visible')));
    expect(renders).toHaveLength(2);
    expect(last()).toMatchObject({ url: null, loading: true });
    await act(async () => started(1).resolve(new Blob(['again'])));
    expect(last().url).toBe('blob:pdf-2');
  });
});

function Invoice({ total }: { total: number; note?: string }) {
  return <span>{total}</span>;
}

describe('the components', () => {
  it('render again when the document changes, and not for an equal one', async () => {
    const view = (total: number, note?: string) => (
      <PdfViewer document={<Invoice total={total} note={note} />} />
    );
    await act(async () => root.render(view(1)));
    await act(async () => started(0).resolve(new Blob(['one'])));
    // Made again with the same props: the same document.
    await act(async () => root.render(view(1)));
    expect(renders).toHaveLength(1);
    await act(async () => root.render(view(2)));
    expect(renders).toHaveLength(2);
    // The last PDF stays until the next one is ready.
    expect(host.querySelector('iframe')?.getAttribute('src')).toBe('blob:pdf-1');
    await act(async () => root.render(view(2, 'paid')));
    expect(renders).toHaveLength(3);
    expect(started(1).opts.signal?.aborted).toBe(true);
    await act(async () => started(2).resolve(new Blob(['two'])));
    expect(host.querySelector('iframe')?.getAttribute('src')).toBe('blob:pdf-2');
  });

  it('hides the toolbar with showToolbar={false}', async () => {
    await act(async () => root.render(<PdfViewer document={doc} showToolbar={false} fallback="…" />));
    expect(host.textContent).toBe('…');
    await act(async () => started(0).resolve(new Blob(['pdf'])));
    expect(host.querySelector('iframe')?.getAttribute('src')).toBe('blob:pdf-1#toolbar=0');
  });

  it('BlobProvider shows what its function makes of the render', async () => {
    const view = (total: number) => (
      <BlobProvider document={<Invoice total={total} />}>
        {({ url, loading }) => (loading ? 'rendering' : url)}
      </BlobProvider>
    );
    await act(async () => root.render(view(1)));
    expect(host.textContent).toBe('rendering');
    await act(async () => started(0).resolve(new Blob(['pdf'])));
    expect(host.textContent).toBe('blob:pdf-1');
    await act(async () => root.render(view(2)));
    expect(host.textContent).toBe('rendering');
  });
});
