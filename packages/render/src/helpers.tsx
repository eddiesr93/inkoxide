/**
 * React helpers of the browser entry: a hook that renders a document to a PDF blob and its URL,
 * a link that downloads it, a frame that shows it in the browser's own viewer, and a component
 * that hands the render's state to a function.
 */
import {
  type AnchorHTMLAttributes,
  type IframeHTMLAttributes,
  type ReactElement,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';

import { type BrowserRenderOptions, renderToBlob } from './browser-api.ts';

export interface UsePdfResult {
  /** The PDF of the last render that finished, or null. */
  blob: Blob | null;
  /**
   * An object URL of `blob`, revoked when a new one replaces it and on unmount; or null, and null
   * again while the component is hidden (`<Activity mode="hidden">`) until it renders again.
   */
  url: string | null;
  /** Whether a render is running. */
  loading: boolean;
  /** Why the last render failed (a `RenderError`, usually), or null. */
  error: unknown;
  /** Renders again: `element` when given, otherwise the last one. */
  update(element?: ReactElement): void;
}

/**
 * Renders `element` to a PDF in the engine's worker when the component mounts, and again on
 * `update()`. A render that starts stops the one still running; unmounting stops it too.
 * `opts` are read when a render starts.
 *
 * ```tsx
 * const { url, loading, update } = usePdf(<Invoice invoice={invoice} />);
 * ```
 */
export function usePdf(element?: ReactElement, opts: BrowserRenderOptions = {}): UsePdfResult {
  const [state, setState] = useState<Omit<UsePdfResult, 'update'>>({
    blob: null,
    url: null,
    loading: element !== undefined,
    error: null,
  });
  const latest = useRef({ element, opts });
  // Before the effect below, which renders with them.
  useEffect(() => {
    latest.current.opts = opts;
  });
  const running = useRef<AbortController | undefined>(undefined);
  const url = useRef<string | null>(null);

  const update = useCallback((next?: ReactElement) => {
    if (next !== undefined) latest.current.element = next;
    const doc = latest.current.element;
    if (doc === undefined) return;
    running.current?.abort();
    const controller = new AbortController();
    running.current = controller;
    const { signal } = controller;
    setState((s) => ({ ...s, loading: true }));
    renderToBlob(doc, { ...latest.current.opts, signal }).then(
      (blob) => {
        if (signal.aborted) return;
        if (url.current) URL.revokeObjectURL(url.current);
        url.current = URL.createObjectURL(blob);
        setState({ blob, url: url.current, loading: false, error: null });
      },
      (error: unknown) => {
        if (signal.aborted) return;
        setState((s) => ({ ...s, loading: false, error }));
      },
    );
  }, []);

  useEffect(() => {
    update();
    return () => {
      running.current?.abort();
      if (url.current) URL.revokeObjectURL(url.current);
      url.current = null;
      // React also cleans up without unmounting (a hidden `<Activity>`): no revoked URL to show.
      setState((s) => ({ ...s, url: null }));
    };
  }, [update]);

  return { ...state, update };
}

/**
 * Whether `next` renders as `prev` did: the same element, or one of the same type and key whose
 * props are the same values (`<Invoice invoice={invoice} />` made again with the same `invoice`).
 */
function sameElement(prev: ReactElement, next: ReactElement): boolean {
  if (prev === next) return true;
  if (prev.type !== next.type || prev.key !== next.key) return false;
  const a = prev.props as Record<string, unknown>;
  const b = next.props as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => Object.is(a[k], b[k]));
}

/**
 * `usePdf` for a component given `document` by its parent: renders again when the parent gives
 * it another one. Not for `usePdf` itself, whose caller makes the element on each of its renders,
 * the ones the hook's own state causes as well.
 */
function useDocumentPdf(document: ReactElement, options?: BrowserRenderOptions): UsePdfResult {
  const pdf = usePdf(document, options);
  const shown = useRef(document);
  const { update } = pdf;
  useEffect(() => {
    if (sameElement(shown.current, document)) return;
    shown.current = document;
    update(document);
  }, [document, update]);
  return pdf;
}

export interface PdfDownloadLinkProps extends Omit<
  AnchorHTMLAttributes<HTMLAnchorElement>,
  'href' | 'download' | 'children'
> {
  /** The `<Document>` element to render. */
  document: ReactElement;
  /** The name the file is saved under. Default `document.pdf`. */
  fileName?: string;
  options?: BrowserRenderOptions;
  /** What the link shows, or a function of the render's state. */
  children?: ReactNode | ((pdf: UsePdfResult) => ReactNode);
}

/**
 * A link that downloads `document` as a PDF, once it has rendered. It renders when it mounts and
 * when `document` changes: an element of another type or key, or with other props.
 */
export function PdfDownloadLink({
  document,
  fileName = 'document.pdf',
  options,
  children,
  ...rest
}: PdfDownloadLinkProps) {
  const pdf = useDocumentPdf(document, options);
  return (
    <a {...rest} href={pdf.url ?? undefined} download={fileName} aria-disabled={pdf.url === null}>
      {typeof children === 'function' ? children(pdf) : children}
    </a>
  );
}

export interface PdfViewerProps extends Omit<IframeHTMLAttributes<HTMLIFrameElement>, 'src' | 'children'> {
  /** The `<Document>` element to render. */
  document: ReactElement;
  options?: BrowserRenderOptions;
  /** Shown in place of the frame while the first render runs, or when it failed. */
  fallback?: ReactNode;
  /**
   * `false` asks the viewer to hide its toolbar (`#toolbar=0` on the URL). Chromium's viewer
   * hides it; others may not. Default `true`.
   */
  showToolbar?: boolean;
}

/**
 * `document`, rendered and shown in an `<iframe>`, in the browser's own PDF viewer. It renders
 * when it mounts and when `document` changes, and shows the last PDF until the next one is ready.
 */
export function PdfViewer({
  document,
  options,
  fallback = null,
  title = 'PDF',
  showToolbar = true,
  ...rest
}: PdfViewerProps) {
  const pdf = useDocumentPdf(document, options);
  if (pdf.url === null) return <>{fallback}</>;
  return <iframe {...rest} title={title} src={showToolbar ? pdf.url : `${pdf.url}#toolbar=0`} />;
}

export interface BlobProviderProps {
  /** The `<Document>` element to render. */
  document: ReactElement;
  options?: BrowserRenderOptions;
  /** What it shows: a function of the render's state. */
  children: (pdf: UsePdfResult) => ReactNode;
}

/**
 * Renders `document` and shows what `children` makes of the render's state (`blob`, `url`,
 * `loading`, `error`): a link, a frame, a preview of its own. It renders when it mounts and when
 * `document` changes, as `PdfViewer` does.
 */
export function BlobProvider({ document, options, children }: BlobProviderProps) {
  return <>{children(useDocumentPdf(document, options))}</>;
}
