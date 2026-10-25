/**
 * React helpers of the browser entry: a hook that renders a document to a PDF blob and its URL,
 * a link that downloads it, a frame that shows it in the browser's own viewer, and a component
 * that hands the render's state to a function.
 */
import {
  type BrowserRenderOptions,
  PdfRun,
  type PdfSnapshot,
  type PdfState,
  viewerUrl,
} from '@inkoxide/render/browser';
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

import { browserRenderer } from './browser-render.ts';

/** `usePdf`'s state, as every front end's helpers give it, and `update()`. */
export interface UsePdfResult extends PdfState<ReactElement> {
  /**
   * An object URL of `blob`, revoked when a new one replaces it and on unmount; or null, and null
   * again while the component is hidden (`<Activity mode="hidden">`) until it renders again.
   */
  url: string | null;
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
  return usePdfRun(element, opts)[0];
}

/**
 * The render of `element` a component keeps: rendered when the component mounts (and shows again
 * after `<Activity>` hid it), stopped when it unmounts or hides. Gives `usePdf`'s result, and
 * `follow`, which renders a document when it is another one than the last (`PdfRun.follow`).
 */
function usePdfRun(
  element: ReactElement | undefined,
  opts: BrowserRenderOptions = {},
): [UsePdfResult, follow: (next: ReactElement) => void] {
  const [state, setState] = useState<PdfSnapshot>({
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
  const run = useRef<PdfRun<ReactElement> | undefined>(undefined);
  run.current ??= new PdfRun(browserRenderer.renderToBlob, setState, () => latest.current.opts);

  const update = useCallback((next?: ReactElement) => run.current?.update(next), []);
  const follow = useCallback((next: ReactElement) => run.current?.follow(next), []);

  useEffect(() => {
    const pdf = run.current;
    pdf?.update(pdf.document ?? latest.current.element);
    // React also cleans up without unmounting (a hidden `<Activity>`): stopped, it shows no
    // revoked URL.
    return () => pdf?.stop();
  }, []);

  return [{ ...state, update }, follow];
}

/**
 * `usePdf` for a component given `document` by its parent: renders again when the parent gives
 * it another one. Not for `usePdf` itself, whose caller makes the element on each of its renders,
 * the ones the hook's own state causes as well.
 */
function useDocumentPdf(document: ReactElement, options?: BrowserRenderOptions): UsePdfResult {
  const [pdf, follow] = usePdfRun(document, options);
  useEffect(() => follow(document), [document, follow]);
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
  return <iframe {...rest} title={title} src={viewerUrl(pdf.url, showToolbar)} />;
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
