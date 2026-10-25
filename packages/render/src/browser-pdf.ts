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

  /**
   * Renders `input` when it is another document than the last one (`sameDocument`): what the
   * helpers do when the document they are given changes.
   */
  follow(input: Input | undefined): void {
    if (input === undefined || (this.last !== undefined && sameDocument(this.last, input))) return;
    this.update(input);
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
 * Whether two documents render the same PDF, so that a helper given one again does not render it
 * again: the same value; a component with the same props or inputs (`{ component, props }` in
 * Svelte, `{ component, inputs }` in Angular); or an element of the same type and key whose props
 * are the same values (a React element or a Vue VNode made again with the same props).
 */
export function sameDocument(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (!isObject(a) || !isObject(b)) return false;
  if ('component' in a || 'component' in b) {
    return a.component === b.component && sameProps(a.props, b.props) && sameProps(a.inputs, b.inputs);
  }
  return 'type' in a && a.type === b.type && a.key === b.key && sameProps(a.props, b.props);
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/** Whether two sets of props are the same values (none and `{}` are). */
function sameProps(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  const x = isObject(a) ? a : {};
  const y = isObject(b) ? b : {};
  const keys = Object.keys(x);
  return (
    keys.length === Object.keys(y).length && keys.every((k) => Object.hasOwn(y, k) && Object.is(x[k], y[k]))
  );
}
