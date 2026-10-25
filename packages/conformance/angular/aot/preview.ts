// The browser directives in an application compiled ahead of time: its build reads them from the
// package's declaration files, as those of any library (test/angular-aot.test.ts).
import { Component } from '@angular/core';
import { InkBlobProvider, InkPdfDownload, InkPdfViewer } from '@inkoxide/angular/browser';

import { Receipt } from './receipt.ts';

@Component({
  selector: 'app-preview',
  imports: [InkPdfViewer, InkPdfDownload, InkBlobProvider],
  template: `
    <iframe
      [inkPdfViewer]="receipt"
      [inkPdfViewerToolbar]="false"
      #viewer="inkPdfViewer"
      title="Receipt"
    ></iframe>
    <p>{{ viewer.pdf.loading() }}</p>
    <a [inkPdfDownload]="receipt" inkPdfDownloadFile="receipt.pdf" #link="inkPdfDownload">
      {{ link.pdf.loading() ? 'Rendering' : 'Download' }}
    </a>
    <p *inkBlobProvider="receipt; let pdf">{{ pdf.url() }}</p>
  `,
})
export class Preview {
  readonly receipt = { component: Receipt, inputs: { customer: 'Ada Lovelace', lines: [] } };
}
