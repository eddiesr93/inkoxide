// @vitest-environment happy-dom
/**
 * `injectPdf`, with the render mocked: each render it starts waits until the test settles it.
 */
import '@angular/compiler';
import { Injector, runInInjectionContext, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { BrowserTestingModule, platformBrowserTesting } from '@angular/platform-browser/testing';
import { afterEach, beforeAll, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import { injectPdf } from '../src/helpers.ts';

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

class Invoice {}
const started = (n: number): Started => renders[n] as Started;
const settle = () => new Promise((r) => setTimeout(r, 0));

let revoked: MockInstance<typeof URL.revokeObjectURL>;

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});

beforeEach(() => {
  renders.length = 0;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:pdf-${++made}`);
  revoked = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
});

afterEach(() => {
  TestBed.resetTestingModule();
  vi.restoreAllMocks();
});

describe('injectPdf', () => {
  it('renders, again when its document changes, and revokes the URLs it replaces', async () => {
    const n = signal(1);
    const pdf = runInInjectionContext(TestBed.inject(Injector), () =>
      injectPdf(() => ({ component: Invoice, inputs: { n: n() } })),
    );
    TestBed.tick();
    expect(renders).toHaveLength(1);
    expect(pdf.loading()).toBe(true);
    n.set(2);
    TestBed.tick();
    expect(renders).toHaveLength(2);
    expect(started(0).opts.signal?.aborted).toBe(true);
    const fresh = new Blob(['fresh']);
    started(0).resolve(new Blob(['stale']));
    started(1).resolve(fresh);
    await settle();
    expect(pdf.blob()).toBe(fresh);
    expect(pdf.url()).toBe('blob:pdf-1');
    expect(pdf.safeUrl()).not.toBeNull();
    pdf.update();
    started(2).resolve(new Blob(['newer']));
    await settle();
    expect(revoked).toHaveBeenCalledWith('blob:pdf-1');
  });

  it('does not render again for the same component and inputs', () => {
    const tick = signal(0);
    const inputs = { a: 1 };
    runInInjectionContext(TestBed.inject(Injector), () =>
      injectPdf(() => (tick(), { component: Invoice, inputs })),
    );
    TestBed.tick();
    tick.set(1);
    TestBed.tick();
    expect(renders).toHaveLength(1);
  });

  it('gives the error of a render that fails', async () => {
    const pdf = runInInjectionContext(TestBed.inject(Injector), () => injectPdf({ component: Invoice }));
    TestBed.tick();
    const error = new Error('the render failed');
    started(0).reject(error);
    await settle();
    expect(pdf.error()).toBe(error);
    expect(pdf.loading()).toBe(false);
  });
});
