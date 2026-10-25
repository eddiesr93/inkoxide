/**
 * Vue helpers of the browser entry: a composable that renders a document to a PDF blob and its
 * URL, a link that downloads it, a frame that shows it in the browser's own viewer, and a
 * component that hands the render's state to its slot.
 */
import {
  type BrowserRenderOptions,
  PdfRun,
  type PdfState as RenderState,
  viewerUrl,
} from '@inkoxide/render/browser';
import {
  defineComponent,
  h,
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
 * `document` now: a ref's value, a getter's, or itself. A function that takes props is a functional
 * component, which `toValue` would call as a getter; one that takes none renders the same either way.
 */
const current = (document: MaybeRefOrGetter<Renderable | undefined>): Renderable | undefined =>
  typeof document === 'function' && document.length > 0 ? (document as Renderable) : toValue(document);

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
  const run = new PdfRun<Renderable>(
    browserRenderer.renderToBlob,
    (state) => {
      blob.value = state.blob;
      url.value = state.url;
      loading.value = state.loading;
      error.value = state.error;
    },
    () => toValue(opts),
  );

  watch(
    () => current(document),
    (next) => run.follow(next),
    { immediate: true },
  );
  onScopeDispose(() => run.stop());

  return { blob, url, loading, error, update: (next) => run.update(next) };
}

/** The state `usePdf` gives, unwrapped, as the slots of the components receive it. */
export type PdfState = RenderState<Renderable>;

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
      return h('iframe', { title: props.title, src: viewerUrl(url, props.showToolbar) });
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
