// A server route's code, as Nuxt or SvelteKit builds it: the invoice rendered with the Vue and the
// Svelte front ends, from templates their Vite plugins compiled in a server build.
import { renderToBuffer as svelteRender } from '@inkoxide/svelte';
import { renderToBuffer as vueRender } from '@inkoxide/vue';
import { h } from 'vue';

import SvelteInvoice from '../svelte/Invoice.svelte';
import VueInvoice from '../vue/Invoice.vue';

const OPTIONS = { creationDate: '2026-10-25T00:00:00Z' };

export const vue = (data: Record<string, unknown>) => vueRender(h(VueInvoice, data), OPTIONS);
export const svelte = (data: Record<string, unknown>) =>
  svelteRender({ component: SvelteInvoice, props: data }, OPTIONS);
