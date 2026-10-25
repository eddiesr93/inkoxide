// The invoice, written as Svelte components, in the browser's PDF viewer and behind a download
// link, rendered in the engine's Web Worker.
import { engineStatus, renderToBuffer } from '@inkoxide/svelte/browser';
import { mount } from 'svelte';

import Invoice from '../../../packages/conformance/svelte/Invoice.svelte';
import { expose, OPTIONS } from './hooks.ts';
import Page from './Page.svelte';
import { sample } from './sample.ts';

expose(() => renderToBuffer({ component: Invoice, props: sample }, OPTIONS), engineStatus);
mount(Page, { target: document.querySelector('#root') as HTMLElement });
