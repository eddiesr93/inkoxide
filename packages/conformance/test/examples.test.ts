/**
 * The examples of examples/, written again with each front end: each must build the document
 * React builds, and the same PDF.
 */
import '@angular/compiler';
import { createHash } from 'node:crypto';

import { renderTemplate as angularTree } from '@inkoxide/angular';
import { renderTemplate as reactTree } from '@inkoxide/react';
import { init, renderDocument } from '@inkoxide/render';
import { renderTemplate as svelteTree } from '@inkoxide/svelte';
import { setTailwindStrict } from '@inkoxide/tailwind';
import { loadTailwind } from '@inkoxide/tree';
import { renderTemplate as vueTree } from '@inkoxide/vue';
import { createElement } from 'react';
import { beforeAll, describe, expect, it } from 'vitest';
import { h } from 'vue';

import ReactCertificate, { sample as certificate } from '../../../examples/certificate.tsx';
import ReactForm, { sample as form } from '../../../examples/form.tsx';
import ReactInvoice, { sample as invoice } from '../../../examples/invoice.tsx';
import ReactLetter, { sample as letter } from '../../../examples/letter.tsx';
import ReactReceipt, { sample as receipt } from '../../../examples/receipt.tsx';
import ReactReport, { sample as report } from '../../../examples/report.tsx';
import * as Ng from '../angular/examples.ts';
import SvelteCertificate from '../svelte/Certificate.svelte';
import SvelteForm from '../svelte/Form.svelte';
import SvelteInvoice from '../svelte/Invoice.svelte';
import SvelteLetter from '../svelte/Letter.svelte';
import SvelteReceipt from '../svelte/Receipt.svelte';
import SvelteReport from '../svelte/Report.svelte';
import VueCertificate from '../vue/Certificate.vue';
import VueForm from '../vue/Form.vue';
import VueInvoice from '../vue/Invoice.vue';
import VueLetter from '../vue/Letter.vue';
import VueReceipt from '../vue/Receipt.vue';
import VueReport from '../vue/Report.vue';

const EXAMPLES = [
  {
    name: 'receipt',
    react: ReactReceipt,
    data: receipt,
    vue: VueReceipt,
    angular: Ng.Receipt,
    svelte: SvelteReceipt,
  },
  {
    name: 'invoice',
    react: ReactInvoice,
    data: invoice,
    vue: VueInvoice,
    angular: Ng.Invoice,
    svelte: SvelteInvoice,
  },
  {
    name: 'letter',
    react: ReactLetter,
    data: letter,
    vue: VueLetter,
    angular: Ng.Letter,
    svelte: SvelteLetter,
  },
  { name: 'form', react: ReactForm, data: form, vue: VueForm, angular: Ng.Form, svelte: SvelteForm },
  {
    name: 'certificate',
    react: ReactCertificate,
    data: certificate,
    vue: VueCertificate,
    angular: Ng.Certificate,
    svelte: SvelteCertificate,
  },
  {
    name: 'report',
    react: ReactReport,
    data: report,
    vue: VueReport,
    angular: Ng.Report,
    svelte: SvelteReport,
  },
] as const;

const OPTS = { creationDate: '2026-10-25T00:00:00Z' };
const sha = (pdf: Uint8Array) => createHash('sha256').update(pdf).digest('hex');

beforeAll(async () => {
  await init();
  await loadTailwind();
  setTailwindStrict(true);
});

describe.each(EXAMPLES)('$name', ({ react, data, vue, angular, svelte }) => {
  const expected = () => reactTree(createElement(react as never, data as never));

  it('Vue builds the document React builds', () => {
    expect(vueTree(h(vue as never, data as never)).document).toEqual(expected().document);
  });

  it('Angular builds the document React builds', () => {
    expect(angularTree({ component: angular, inputs: data as never }).document).toEqual(expected().document);
  });

  it('Angular renders the PDF React renders', async () => {
    const a = expected();
    const b = angularTree({ component: angular, inputs: data as never });
    const [pa, pb] = await Promise.all([
      renderDocument(a.document, a.images, OPTS),
      renderDocument(b.document, b.images, OPTS),
    ]);
    expect(sha(pb.pdf)).toBe(sha(pa.pdf));
  });

  it('Svelte builds the document React builds', () => {
    expect(svelteTree({ component: svelte, props: data as never }).document).toEqual(expected().document);
  });

  it('Svelte renders the PDF React renders', async () => {
    const a = expected();
    const b = svelteTree({ component: svelte, props: data as never });
    const [pa, pb] = await Promise.all([
      renderDocument(a.document, a.images, OPTS),
      renderDocument(b.document, b.images, OPTS),
    ]);
    expect(sha(pb.pdf)).toBe(sha(pa.pdf));
  });

  it('Vue renders the PDF React renders', async () => {
    const a = expected();
    const b = vueTree(h(vue as never, data as never));
    const [pa, pb] = await Promise.all([
      renderDocument(a.document, a.images, OPTS),
      renderDocument(b.document, b.images, OPTS),
    ]);
    expect(sha(pb.pdf)).toBe(sha(pa.pdf));
  });
});
