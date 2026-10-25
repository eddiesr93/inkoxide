import * as i0 from '@angular/core';
import { effect, ElementRef, inject, input, Renderer2, TemplateRef, ViewContainerRef } from '@angular/core';
// The Angular directives of the browser entry: a frame that shows a document in the browser's own
// PDF viewer, a link that downloads it, and a template that gets the render's state.
//
// They are declared as Angular's partial compilation writes a library (`ɵɵngDeclareDirective`):
// the application's build links them like those of any library, and a template compiled just in
// time compiles them too. So this package needs no Angular compiler of its own.
import type { BrowserRenderOptions } from '@inkoxide/render/browser';

import { injectPdf, type InjectPdfResult } from './helpers.ts';
import type { Renderable } from './template.ts';

// The declarations as the linker reads them; Angular's types leave out `minVersion`, which it needs.
type FactoryDeclaration = Parameters<typeof i0.ɵɵngDeclareFactory>[0];
type DirectiveDeclaration = Parameters<typeof i0.ɵɵngDeclareDirective>[0];

/**
 * `<iframe [inkPdfViewer]="document">`: the document rendered and shown in the frame, in the
 * browser's own PDF viewer. It renders at once and when `document` changes, and keeps the last PDF
 * until the next one is ready. `inkPdfViewerOptions` are the render's options; `[inkPdfViewerToolbar]="false"`
 * asks the viewer to hide its toolbar (`#toolbar=0`).
 */
export class InkPdfViewer {
  readonly document = input.required<Renderable>({ alias: 'inkPdfViewer' });
  readonly options = input<BrowserRenderOptions | undefined>(undefined, { alias: 'inkPdfViewerOptions' });
  readonly toolbar = input<boolean>(true, { alias: 'inkPdfViewerToolbar' });
  /** The render's state. */
  readonly pdf: InjectPdfResult = injectPdf(
    () => this.document(),
    () => this.options(),
  );

  constructor() {
    const frame = inject(ElementRef).nativeElement as Element;
    const renderer = inject(Renderer2);
    effect(() => {
      const url = this.pdf.url();
      if (url === null) renderer.removeAttribute(frame, 'src');
      else renderer.setAttribute(frame, 'src', this.toolbar() ? url : `${url}#toolbar=0`);
    });
  }

  static ɵfac = i0.ɵɵngDeclareFactory({
    minVersion: '12.0.0',
    version: '21.0.0',
    ngImport: i0,
    type: InkPdfViewer,
    deps: [],
    target: i0.ɵɵFactoryTarget.Directive,
  } as FactoryDeclaration);

  static ɵdir = i0.ɵɵngDeclareDirective({
    minVersion: '17.1.0',
    version: '21.0.0',
    type: InkPdfViewer,
    isStandalone: true,
    selector: 'iframe[inkPdfViewer]',
    inputs: {
      document: {
        classPropertyName: 'document',
        publicName: 'inkPdfViewer',
        isSignal: true,
        isRequired: true,
        transformFunction: null,
      },
      options: {
        classPropertyName: 'options',
        publicName: 'inkPdfViewerOptions',
        isSignal: true,
        isRequired: false,
        transformFunction: null,
      },
      toolbar: {
        classPropertyName: 'toolbar',
        publicName: 'inkPdfViewerToolbar',
        isSignal: true,
        isRequired: false,
        transformFunction: null,
      },
    },
    exportAs: ['inkPdfViewer'],
    ngImport: i0,
  } as DirectiveDeclaration);
}

/**
 * `<a [inkPdfDownload]="document" inkPdfDownloadFile="invoice.pdf">`: a link that downloads the
 * document as a PDF once it has rendered (`aria-disabled` until then). It renders at once and when
 * `document` changes. `#link="inkPdfDownload"` gives the render's state, `link.pdf`.
 */
export class InkPdfDownload {
  readonly document = input.required<Renderable>({ alias: 'inkPdfDownload' });
  readonly fileName = input<string>('document.pdf', { alias: 'inkPdfDownloadFile' });
  readonly options = input<BrowserRenderOptions | undefined>(undefined, { alias: 'inkPdfDownloadOptions' });
  /** The render's state. */
  readonly pdf: InjectPdfResult = injectPdf(
    () => this.document(),
    () => this.options(),
  );

  constructor() {
    const link = inject(ElementRef).nativeElement as Element;
    const renderer = inject(Renderer2);
    effect(() => {
      const url = this.pdf.url();
      renderer.setAttribute(link, 'download', this.fileName());
      renderer.setAttribute(link, 'aria-disabled', String(url === null));
      if (url === null) renderer.removeAttribute(link, 'href');
      else renderer.setAttribute(link, 'href', url);
    });
  }

  static ɵfac = i0.ɵɵngDeclareFactory({
    minVersion: '12.0.0',
    version: '21.0.0',
    ngImport: i0,
    type: InkPdfDownload,
    deps: [],
    target: i0.ɵɵFactoryTarget.Directive,
  } as FactoryDeclaration);

  static ɵdir = i0.ɵɵngDeclareDirective({
    minVersion: '17.1.0',
    version: '21.0.0',
    type: InkPdfDownload,
    isStandalone: true,
    selector: 'a[inkPdfDownload]',
    inputs: {
      document: {
        classPropertyName: 'document',
        publicName: 'inkPdfDownload',
        isSignal: true,
        isRequired: true,
        transformFunction: null,
      },
      fileName: {
        classPropertyName: 'fileName',
        publicName: 'inkPdfDownloadFile',
        isSignal: true,
        isRequired: false,
        transformFunction: null,
      },
      options: {
        classPropertyName: 'options',
        publicName: 'inkPdfDownloadOptions',
        isSignal: true,
        isRequired: false,
        transformFunction: null,
      },
    },
    exportAs: ['inkPdfDownload'],
    ngImport: i0,
  } as DirectiveDeclaration);
}

/** The context of `*inkBlobProvider`: the render's state. */
export interface BlobProviderContext {
  $implicit: InjectPdfResult;
}

/**
 * `<ng-container *inkBlobProvider="document; let pdf">…</ng-container>`: renders the document and
 * shows its content with the render's state (`pdf.url()`, `pdf.loading()`, …). It renders at once
 * and when `document` changes.
 */
export class InkBlobProvider {
  readonly document = input.required<Renderable>({ alias: 'inkBlobProvider' });
  readonly options = input<BrowserRenderOptions | undefined>(undefined, { alias: 'inkBlobProviderOptions' });
  readonly pdf: InjectPdfResult = injectPdf(
    () => this.document(),
    () => this.options(),
  );

  constructor() {
    const template = inject(TemplateRef) as TemplateRef<BlobProviderContext>;
    inject(ViewContainerRef).createEmbeddedView(template, { $implicit: this.pdf });
  }

  static ngTemplateContextGuard(_dir: InkBlobProvider, _context: unknown): _context is BlobProviderContext {
    return true;
  }

  static ɵfac = i0.ɵɵngDeclareFactory({
    minVersion: '12.0.0',
    version: '21.0.0',
    ngImport: i0,
    type: InkBlobProvider,
    deps: [],
    target: i0.ɵɵFactoryTarget.Directive,
  } as FactoryDeclaration);

  static ɵdir = i0.ɵɵngDeclareDirective({
    minVersion: '17.1.0',
    version: '21.0.0',
    type: InkBlobProvider,
    isStandalone: true,
    selector: '[inkBlobProvider]',
    inputs: {
      document: {
        classPropertyName: 'document',
        publicName: 'inkBlobProvider',
        isSignal: true,
        isRequired: true,
        transformFunction: null,
      },
      options: {
        classPropertyName: 'options',
        publicName: 'inkBlobProviderOptions',
        isSignal: true,
        isRequired: false,
        transformFunction: null,
      },
    },
    ngImport: i0,
  } as DirectiveDeclaration);
}
