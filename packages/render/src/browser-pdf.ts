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
  /**
   * Why the last render failed, or null: a `RenderError`, or the `DOMException` of a `timeoutMs`
   * that passed. What is thrown that is not an `Error` (the reason of a `signal`) is wrapped in one.
   */
  error: Error | null;
}

/** What a helper gives besides the state: `update()` renders again, `document` or the last one. */
export interface PdfState<Input> extends PdfSnapshot {
  update(document?: Input): void;
}

/**
 * The URL a frame shows a PDF at: `url`, and `#toolbar=0` when the viewer's toolbar is not wanted
 * (Chromium's viewer follows it).
 */
export const viewerUrl = (url: string, toolbar: boolean): string => (toolbar ? url : `${url}#toolbar=0`);

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

  private readonly render: BlobRender<Input>;
  private readonly onChange: (state: PdfSnapshot) => void;
  private readonly options: () => BrowserRenderOptions | undefined;
  constructor(
    render: BlobRender<Input>,
    onChange: (state: PdfSnapshot) => void,
    options: () => BrowserRenderOptions | undefined = () => undefined,
  ) {
    this.render = render;
    this.onChange = onChange;
    this.options = options;
  }

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
        this.set({ loading: false, error: error instanceof Error ? error : new Error(String(error)) });
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
 * again: the same value; an element of the same type and key whose props and children are the
 * same values (a React element or a Vue VNode made again with the same props); or a component
 * with the same props or inputs (`{ component, props }` in Svelte, `{ component, inputs }` in
 * Angular). Elements first: a VNode has a `component` too, the instance it mounts, null until then.
 * A component alone is the same only as itself. Props that are arrays or plain objects are
 * compared by their contents: a template that builds its data as it renders (`{ ...invoice }`)
 * would otherwise be another document each time, and a helper whose state the template reads
 * would render without end.
 */
export function sameDocument(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (!isObject(a) || !isObject(b)) return false;
  if ('type' in a || 'type' in b) {
    return a.type === b.type && a.key === b.key && a.children === b.children && sameProps(a.props, b.props);
  }
  // Any other object (a Vue component's options) is the same document only as itself.
  if (!('component' in a && 'component' in b)) return false;
  return a.component === b.component && sameProps(a.props, b.props) && sameProps(a.inputs, b.inputs);
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/** Whether two sets of props are the same values (none and `{}` are). */
function sameProps(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  return sameValue(isObject(a) ? a : {}, isObject(b) ? b : {}, 0);
}

/** How deep data is compared by its contents; deeper, it is the same only as itself. */
const MAX_DEPTH = 32;

const isPlain = (value: object): boolean => {
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

/**
 * Whether two values are the same data: the same value, or arrays and plain objects with the same
 * contents, so that data made again with the same contents (`{ ...invoice }`, a list filtered
 * again) is not another document. Anything else (a function, bytes, a `Date`, an instance of a
 * class) is the same only as itself.
 */
function sameValue(a: unknown, b: unknown, depth: number): boolean {
  if (Object.is(a, b)) return true;
  if (!isObject(a) || !isObject(b) || depth > MAX_DEPTH) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, i) => sameValue(item, b[i], depth + 1));
  }
  if (!isPlain(a) || !isPlain(b)) return false;
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length &&
    keys.every((k) => Object.hasOwn(b, k) && sameValue(a[k], b[k], depth + 1))
  );
}
