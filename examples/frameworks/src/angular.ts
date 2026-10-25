// The invoice, written as an Angular component, in the browser's PDF viewer and behind a download
// link, rendered in the engine's Web Worker. The components compile just in time here; an
// application built with Angular's compiler needs no `@angular/compiler` and no 'unsafe-eval'.
import '@angular/compiler';
import { Component, provideZonelessChangeDetection } from '@angular/core';
import { bootstrapApplication } from '@angular/platform-browser';
import { InkPdfDownload, InkPdfViewer, renderToBuffer } from '@inkoxide/angular/browser';

import { Invoice } from '../../../packages/conformance/angular/examples.ts';
import { expose, OPTIONS } from './hooks.ts';
import { sample } from './sample.ts';

const invoice = { component: Invoice, inputs: sample };
expose(() => renderToBuffer(invoice, OPTIONS));

class App {
  readonly invoice = invoice;
  readonly options = OPTIONS;
}
Component({
  selector: 'app-root',
  imports: [InkPdfViewer, InkPdfDownload],
  template: `
    <header>
      <h1>Inkoxide with Angular</h1>
      <a id="download" [inkPdfDownload]="invoice" inkPdfDownloadFile="invoice.pdf" [inkPdfDownloadOptions]="options" #link="inkPdfDownload">
        <span [attr.data-state]="link.pdf.error() ? 'failed' : link.pdf.loading() || !link.pdf.url() ? 'loading' : 'ready'">Download</span>
      </a>
    </header>
    <iframe id="viewer" [inkPdfViewer]="invoice" [inkPdfViewerOptions]="options" title="The invoice"></iframe>
  `,
})(App);

const root = document.querySelector('#root') as HTMLElement;
root.append(document.createElement('app-root'));
void bootstrapApplication(App, { providers: [provideZonelessChangeDetection()] });
