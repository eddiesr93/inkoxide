/**
 * What the browser helpers of every front end share (`usePdf`, `createPdf`, `injectPdf` and the
 * components built on them): a document rendered to a PDF blob and its object URL, again on
 * request, each render stopping the one still running, and each URL revoked when another
 * replaces it. Each front end keeps the state in its own reactivity.
 */
import type { BrowserRenderOptions } from './browser-api.ts';

/** The state of a `PdfRun`, as the helpers show it. */
export interface PdfSnapshot {
  /** The PDF of the last render that finished, or null. */
  blob: Blob | null;
  /** An object URL of `blob`, or null while stopped. */
  url: string | null;
  /** Whether a render is running. */
  loading: boolean;
  /** Why the last render failed (a `RenderError`, usually), or null. */
  error: unknown;
}

/** Renders the document a helper holds, as `renderToBlob` does. */
export type BlobRender<Input> = (input: Input, opts: BrowserRenderOptions) => Promise<Blob>;

/**
 * A document rendered to a PDF in the engine's worker. `update()` renders (the document given, or
 * the last one), stopping the render still running, whose PDF is then not shown; `stop()` stops
 * it, revokes the URL and shows none, and `update()` starts again. `onChange` gets each new state;
 * `options` are read when a render starts.
 */
export class PdfRun<Input> {
  private state: PdfSnapshot = { blob: null, url: null, loading: false, error: null };
  private last: Input | undefined;
  private running: AbortController | undefined;

  constructor(
    private readonly render: BlobRender<Input>,
    private readonly onChange: (state: PdfSnapshot) => void,
    private readonly options: () => BrowserRenderOptions | undefined = () => undefined,
  ) {}

  /** The state now. */
  get snapshot(): PdfSnapshot {
    return this.state;
  }

  /** The document of the last render, if any. */
  get document(): Input | undefined {
    return this.last;
  }

  update(input?: Input): void {
    if (input !== undefined) this.last = input;
    if (this.last === undefined) return;
    this.running?.abort();
    const controller = new AbortController();
    this.running = controller;
    const { signal } = controller;
    this.set({ loading: true });
    this.render(this.last, { ...this.options(), signal }).then(
      (blob) => {
        if (signal.aborted) return;
        if (this.state.url) URL.revokeObjectURL(this.state.url);
        this.set({ blob, url: URL.createObjectURL(blob), loading: false, error: null });
      },
      (error: unknown) => {
        if (signal.aborted) return;
        this.set({ loading: false, error });
      },
    );
  }

  stop(): void {
    this.running?.abort();
    this.running = undefined;
    if (this.state.url) URL.revokeObjectURL(this.state.url);
    this.set({ url: null });
  }

  private set(change: Partial<PdfSnapshot>): void {
    this.state = { ...this.state, ...change };
    this.onChange(this.state);
  }
}

/**
 * Whether two sets of props are the same values: a document made again with the same props
 * renders the same PDF, so the helpers do not render it again.
 */
export function sameProps(
  a: Readonly<Record<string, unknown>> = {},
  b: Readonly<Record<string, unknown>> = {},
): boolean {
  if (a === b) return true;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => Object.is(a[k], b[k]));
}
