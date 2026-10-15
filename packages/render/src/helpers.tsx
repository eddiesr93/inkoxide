/**
 * React helpers of the browser entry: a hook that renders a document to a PDF blob and its URL,
 * a link that downloads it and a frame that shows it in the browser's own viewer.
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
  /** An object URL of `blob`, revoked when a new one replaces it and on unmount; or null. */
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
    };
  }, [update]);

  return { ...state, update };
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
 * A link that downloads `document` as a PDF, once it has rendered. It renders when it mounts: give
 * it another `key` to render again.
 */
export function PdfDownloadLink({
  document,
  fileName = 'document.pdf',
  options,
  children,
  ...rest
}: PdfDownloadLinkProps) {
  const pdf = usePdf(document, options);
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
}

/**
 * `document`, rendered and shown in an `<iframe>`, in the browser's own PDF viewer. It renders
 * when it mounts: give it another `key` to render again.
 */
export function PdfViewer({ document, options, fallback = null, title = 'PDF', ...rest }: PdfViewerProps) {
  const pdf = usePdf(document, options);
  if (pdf.url === null) return <>{fallback}</>;
  return <iframe {...rest} title={title} src={pdf.url} />;
}
