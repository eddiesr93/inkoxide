/**
 * The examples of examples/, written again with each front end: each must build the document
 * React builds, and the same PDF.
 */
import '@angular/compiler';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import { renderTemplate as angularTree } from '@inkoxide/angular';
import { renderTemplate as reactTree } from '@inkoxide/react';
import { init, renderDocument } from '@inkoxide/render';
import { renderTemplate as svelteTree } from '@inkoxide/svelte';
import { setTailwindStrict } from '@inkoxide/tailwind';
import { loadTailwind, type TreeResult } from '@inkoxide/tree';
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
import ReactResume, { sample as resume } from '../../../examples/resume.tsx';
import * as Ng from '../angular/examples.ts';
import ReactFeatures, { sample as features } from '../react/Features.tsx';
import SvelteCertificate from '../svelte/Certificate.svelte';
import SvelteFeatures from '../svelte/Features.svelte';
import SvelteForm from '../svelte/Form.svelte';
import SvelteInvoice from '../svelte/Invoice.svelte';
import SvelteLetter from '../svelte/Letter.svelte';
import SvelteReceipt from '../svelte/Receipt.svelte';
import SvelteReport from '../svelte/Report.svelte';
import SvelteResume from '../svelte/Resume.svelte';
import VueCertificate from '../vue/Certificate.vue';
import VueFeatures from '../vue/Features.vue';
import VueForm from '../vue/Form.vue';
import VueInvoice from '../vue/Invoice.vue';
import VueLetter from '../vue/Letter.vue';
import VueReceipt from '../vue/Receipt.vue';
import VueReport from '../vue/Report.vue';
import VueResume from '../vue/Resume.vue';

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
  {
    name: 'resume',
    react: ReactResume,
    data: resume,
    vue: VueResume,
    angular: Ng.Resume,
    svelte: SvelteResume,
  },
  {
    name: 'features',
    react: ReactFeatures,
    data: features,
    vue: VueFeatures,
    angular: Ng.Features,
    svelte: SvelteFeatures,
  },
] as const;

const OPTS = { creationDate: '2026-10-25T00:00:00Z' };
/** The image the features document names by id. */
const IMAGES = {
  logo: new Uint8Array(readFileSync(new URL('../../../fixtures/images/logo.png', import.meta.url))),
};
const TREE = { imageIds: Object.keys(IMAGES) };
const sha = (pdf: Uint8Array) => createHash('sha256').update(pdf).digest('hex');

beforeAll(async () => {
  await init();
  await loadTailwind();
  setTailwindStrict(true);
});

type Example = (typeof EXAMPLES)[number];

/** How each front end builds the tree of an example. */
const FRONT_ENDS: [string, (e: Example) => TreeResult][] = [
  ['Vue', (e) => vueTree(h(e.vue as never, e.data as never), TREE)],
  ['Svelte', (e) => svelteTree({ component: e.svelte, props: e.data as never }, TREE)],
  ['Angular', (e) => angularTree({ component: e.angular, inputs: e.data as never }, TREE)],
];

describe.each(EXAMPLES)('$name', (example) => {
  const expected = () => reactTree(createElement(example.react as never, example.data as never), TREE);

  describe.each(FRONT_ENDS)('%s', (_, tree) => {
    it('builds the document React builds', () => {
      expect(tree(example).document).toEqual(expected().document);
    });

    it('renders the PDF React renders', async () => {
      const a = expected();
      const b = tree(example);
      const [pa, pb] = await Promise.all([
        renderDocument(a.document, { ...IMAGES, ...a.images }, OPTS),
        renderDocument(b.document, { ...IMAGES, ...b.images }, OPTS),
      ]);
      expect(sha(pb.pdf)).toBe(sha(pa.pdf));
    });
  });
});
