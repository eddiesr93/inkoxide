/**
 * Vue helpers of the browser entry: a composable that renders a document to a PDF blob and its
 * URL, a link that downloads it, a frame that shows it in the browser's own viewer, and a
 * component that hands the render's state to its slot.
 */
import type { BrowserRenderOptions } from '@inkoxide/render/browser';
import {
  defineComponent,
  h,
  isVNode,
  type MaybeRefOrGetter,
  onScopeDispose,
  type PropType,
  type Ref,
  ref,
  shallowRef,
  toValue,
  watch,
} from 'vue';

import { browserRenderer } from './browser-render.ts';
import type { Renderable } from './template.ts';

export interface UsePdfResult {
  /** The PDF of the last render that finished, or null. */
  blob: Ref<Blob | null>;
  /** An object URL of `blob`, revoked when a new one replaces it and when the scope ends; or null. */
  url: Ref<string | null>;
  /** Whether a render is running. */
  loading: Ref<boolean>;
  /** Why the last render failed (a `RenderError`, usually), or null. */
  error: Ref<unknown>;
  /** Renders again: `document` when given, otherwise the last one. */
  update(document?: Renderable): void;
}

/**
 * Whether `next` renders as `prev` did: the same document, or a VNode of the same type and key
 * whose props are the same values (`h(Invoice, { invoice })` made again with the same `invoice`).
 */
function sameDocument(prev: Renderable | undefined, next: Renderable | undefined): boolean {
  if (prev === next) return true;
  if (!isVNode(prev) || !isVNode(next) || prev.type !== next.type || prev.key !== next.key) return false;
  const a = (prev.props ?? {}) as Record<string, unknown>;
  const b = (next.props ?? {}) as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => Object.is(a[k], b[k]));
}

/**
 * Renders `document` to a PDF in the engine's worker, at once and whenever it changes (a ref or a
 * getter: `usePdf(() => h(Invoice, { invoice: invoice.value }))`), and again on `update()`. A
 * render that starts stops the one still running; the end of the scope stops it too. `opts` are
 * read when a render starts.
 */
export function usePdf(
  document?: MaybeRefOrGetter<Renderable | undefined>,
  opts: MaybeRefOrGetter<BrowserRenderOptions | undefined> = {},
): UsePdfResult {
  const blob = shallowRef<Blob | null>(null);
  const url = ref<string | null>(null);
  const loading = ref(false);
  const error = shallowRef<unknown>(null);
  let last: Renderable | undefined;
  let running: AbortController | undefined;

  const update = (next?: Renderable): void => {
    if (next !== undefined) last = next;
    if (last === undefined) return;
    running?.abort();
    const controller = new AbortController();
    running = controller;
    const { signal } = controller;
    loading.value = true;
    browserRenderer.renderToBlob(last, { ...toValue(opts), signal }).then(
      (pdf) => {
        if (signal.aborted) return;
        if (url.value) URL.revokeObjectURL(url.value);
        blob.value = pdf;
        url.value = URL.createObjectURL(pdf);
        loading.value = false;
        error.value = null;
      },
      (e: unknown) => {
        if (signal.aborted) return;
        loading.value = false;
        error.value = e;
      },
    );
  };

  watch(
    () => toValue(document),
    (next) => {
      if (next === undefined || (last !== undefined && sameDocument(last, next))) return;
      update(next);
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    running?.abort();
    if (url.value) URL.revokeObjectURL(url.value);
    url.value = null;
  });

  return { blob, url, loading, error, update };
}

/** The state `usePdf` gives, unwrapped, as the slots of the components receive it. */
export interface PdfState {
  blob: Blob | null;
  url: string | null;
  loading: boolean;
  error: unknown;
  update(document?: Renderable): void;
}

const state = (pdf: UsePdfResult): PdfState => ({
  blob: pdf.blob.value,
  url: pdf.url.value,
  loading: pdf.loading.value,
  error: pdf.error.value,
  update: pdf.update,
});

const documentProp = { type: [Object, Function] as PropType<Renderable>, required: true } as const;
const optionsProp = { type: Object as PropType<BrowserRenderOptions>, default: undefined } as const;

/**
 * A link that downloads `document` as a PDF, once it has rendered. It renders when it mounts and
 * when `document` changes. Its default slot gets the render's state.
 */
export const PdfDownloadLink = defineComponent({
  name: 'PdfDownloadLink',
  props: {
    document: documentProp,
    fileName: { type: String, default: 'document.pdf' },
    options: optionsProp,
  },
  setup(props, { slots }) {
    const pdf = usePdf(
      () => props.document,
      () => props.options,
    );
    return () =>
      h(
        'a',
        {
          href: pdf.url.value ?? undefined,
          download: props.fileName,
          'aria-disabled': pdf.url.value === null,
        },
        slots.default?.(state(pdf)),
      );
  },
});

/**
 * `document`, rendered and shown in an `<iframe>`, in the browser's own PDF viewer. It renders
 * when it mounts and when `document` changes, and shows the last PDF until the next one is ready;
 * before the first, its `fallback` slot.
 */
export const PdfViewer = defineComponent({
  name: 'PdfViewer',
  props: {
    document: documentProp,
    options: optionsProp,
    title: { type: String, default: 'PDF' },
    /** `false` asks the viewer to hide its toolbar (`#toolbar=0`). */
    showToolbar: { type: Boolean, default: true },
  },
  setup(props, { slots }) {
    const pdf = usePdf(
      () => props.document,
      () => props.options,
    );
    return () => {
      const url = pdf.url.value;
      if (url === null) return slots.fallback?.();
      return h('iframe', { title: props.title, src: props.showToolbar ? url : `${url}#toolbar=0` });
    };
  },
});

/**
 * Renders `document` and shows what its default slot makes of the render's state (`blob`, `url`,
 * `loading`, `error`). It renders when it mounts and when `document` changes.
 */
export const BlobProvider = defineComponent({
  name: 'BlobProvider',
  props: { document: documentProp, options: optionsProp },
  setup(props, { slots }) {
    const pdf = usePdf(
      () => props.document,
      () => props.options,
    );
    return () => slots.default?.(state(pdf));
  },
});
