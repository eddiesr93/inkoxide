// @vitest-environment happy-dom
/**
 * The browser directives, compiled just in time from their partial declarations, with the render
 * mocked: each render waits until the test settles it.
 */
import '@angular/compiler';
import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { BrowserTestingModule, platformBrowserTesting } from '@angular/platform-browser/testing';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { InkBlobProvider, InkPdfDownload, InkPdfViewer } from '../src/directives.ts';

interface Started {
  resolve(blob: Blob): void;
}

const renders = vi.hoisted((): Started[] => []);

vi.mock('../src/browser-render.ts', () => ({
  browserRenderer: {
    renderToBlob: () => new Promise<Blob>((resolve) => renders.push({ resolve })),
  },
}));

class Invoice {}
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeAll(() => {
  TestBed.initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});

beforeEach(() => {
  renders.length = 0;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:pdf-${++made}`);
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
});

afterEach(() => {
  TestBed.resetTestingModule();
  vi.restoreAllMocks();
});

@Component({
  selector: 'app-preview',
  imports: [InkPdfViewer, InkPdfDownload, InkBlobProvider],
  template: `
    <iframe [inkPdfViewer]="doc" [inkPdfViewerToolbar]="false" title="PDF"></iframe>
    <a [inkPdfDownload]="doc" inkPdfDownloadFile="invoice.pdf">Download</a>
    <p *inkBlobProvider="doc; let pdf">{{ pdf.loading() ? 'Rendering' : 'Ready' }}</p>
  `,
})
class Preview {
  doc = { component: Invoice, inputs: {} };
}

describe('the browser directives', () => {
  it('show the PDF in the frame, link to it, and give templates the state', async () => {
    const fixture = TestBed.createComponent(Preview);
    fixture.detectChanges();
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('iframe')?.getAttribute('src')).toBeNull();
    expect(el.querySelector('a')?.getAttribute('aria-disabled')).toBe('true');
    expect(el.querySelector('p')?.textContent).toBe('Rendering');
    expect(renders).toHaveLength(3);
    for (const r of renders) r.resolve(new Blob(['pdf']));
    await settle();
    fixture.detectChanges();
    await fixture.whenStable();
    expect(el.querySelector('iframe')?.getAttribute('src')).toMatch(/^blob:pdf-\d#toolbar=0$/);
    expect(el.querySelector('a')?.getAttribute('href')).toMatch(/^blob:pdf-\d$/);
    expect(el.querySelector('a')?.getAttribute('download')).toBe('invoice.pdf');
    expect(el.querySelector('p')?.textContent).toBe('Ready');
  });
});
