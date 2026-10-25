// `@inkoxide/tree`: what every front end shares. The components' host types and props, where each
// may go, and the conversion of the host nodes a front end renders (React, Vue, Svelte, Angular)
// into the engine's document tree, with fonts, images, Tailwind classes and errors that name the
// component at fault.
import type { Document } from '@inkoxide/schema';

import { TemplateError } from './errors.ts';
import { fonts as defaultFonts, type FontRegistry } from './fonts/index.ts';
import { type Container, componentStack, innermostComponent, type Instance } from './nodes.ts';
import { type BuildResult, buildTree, type RenderPage } from './tree/build.ts';
import { ImageIds, type ImageTableOptions, type RemoteImagePolicy } from './tree/images.ts';
import { locate } from './tree/locate.ts';
import { type SchemaProblem, validateTree } from './tree/validate.ts';

export type { CanvasGradient, CanvasTextOptions, DrawingFont, Painter } from './canvas.ts';
export * from './components/index.ts';
export { TemplateError, type TemplateErrorInit, type TemplateErrorKind } from './errors.ts';
export { facturX, type FacturXOptions, type FacturXProfile, facturXRelationship } from './facturx.ts';
export * from './fonts/index.ts';
export { familyKey } from './fonts/registry.ts';
export {
  type Container,
  componentNames,
  componentStack,
  dedupe,
  formatStack,
  hostName,
  innermostComponent,
  type Instance,
  isHostType,
  type Node,
  type TextInstance,
  type Trace,
} from './nodes.ts';
export {
  checkElement,
  checkText,
  checkTree,
  childContext,
  type HostContext,
  ROOT_CONTEXT,
  rootContext,
} from './placement.ts';
export { loadTailwind } from './tailwind.ts';
export type { EncryptionOptions, OutputOptions, PdfALevel, PdfUaPart, RenderPage } from './tree/build.ts';
export {
  type ImageSource,
  imageSource,
  isImagePath,
  type PendingImage,
  type RemoteImagePolicy,
  remoteAllowed,
  remoteRefused,
} from './tree/images.ts';
export { parsePath } from './tree/locate.ts';
export { PAGE_SIZES } from './tree/page-sizes.ts';
export {
  stretchOf as fontStretchValue,
  variationsOf as fontVariationsValue,
  weightOf as fontWeightValue,
} from './tree/style.ts';
export { type SchemaProblem, validateTree } from './tree/validate.ts';

export interface TreeOptions {
  fonts?: FontRegistry;
  /** Remote image URLs `<Image src>` may use; they are listed in `remote` for the caller to fetch. */
  remoteImages?: RemoteImagePolicy;
  /**
   * Where else the caller reads that allowlist from (`'the INKOXIDE_REMOTE_IMAGES variable'`),
   * named after the option when a URL is refused.
   */
  remoteImagesElsewhere?: string;
  /**
   * File paths in `src` (`./logo.png`, `file:` URLs) are listed in `pending` for the caller to
   * read. Off by default: a path is then an image id like any other string.
   */
  localImages?: boolean;
  /** Page count for `render` props (from a previous render); 1 when omitted. */
  pageCount?: number;
  /**
   * Ids of the images the caller passes to the renderer: a `src` string naming one is that image,
   * even when it looks like a path or a URL.
   */
  imageIds?: readonly string[];
}

export interface Located {
  /** The component at the path (`Text`, `Cell`). */
  component: string;
  /** The component stack (`\n    in Text\n    in Invoice`). */
  componentStack: string;
}

export interface TreeResult extends Omit<BuildResult, 'origin'> {
  /** Milliseconds: the front end's render to host nodes, and host nodes to the schema tree. */
  timings: { templateMs: number; treeMs: number };
  /** Maps an engine/ajv path back to the component that produced it (rebuilds lazily). */
  locate(path: string): Located | undefined;
  /** ajv validation with paths mapped to components; `[]` when valid. */
  validate(): Array<SchemaProblem & Partial<Located>>;
  /** The page count the tree was built for (`render` props). */
  pageCount: number;
  /**
   * The same template built for another page count, without rendering the template again. An
   * image keeps its id in every rebuild; images that only a rebuild uses (from `render` props) get
   * new ids, listed in its `images`, `remote` and `pending`.
   */
  rebuild(pageCount: number): TreeResult;
}

/** What a front end gives `buildDocument` besides the host nodes. */
export interface FrontEnd {
  /** Renders an element's `render` prop for one page; see {@link RenderPage}. */
  renderPage?: RenderPage;
}

/**
 * Turns host nodes a front end rendered into an Inkoxide document. `templateMs` is the time the
 * front end took to render them, reported in `timings`. Throws a TemplateError with a component
 * stack.
 */
export function buildDocument(
  container: Container,
  frontEnd: FrontEnd,
  opts: TreeOptions = {},
  templateMs = 0,
): TreeResult {
  const registered = new Set(opts.imageIds);
  const images: ImageTableOptions = {
    registered,
    generated: new ImageIds(registered),
    remoteImages: opts.remoteImages,
    remoteElsewhere: opts.remoteImagesElsewhere,
    localImages: opts.localImages,
  };
  return fromContainer(container, frontEnd, opts, templateMs, images);
}

/** `images` is shared with the rebuilds, so an image keeps its id on every pass. */
function fromContainer(
  container: Container,
  frontEnd: FrontEnd,
  opts: TreeOptions,
  templateMs: number,
  images: ImageTableOptions,
): TreeResult {
  const t1 = performance.now();
  const build = {
    fonts: opts.fonts ?? defaultFonts,
    pageCount: opts.pageCount,
    images,
    renderPage: frontEnd.renderPage,
  };
  const built = buildTree(container, build);
  const t2 = performance.now();
  let tracked: { doc: Document; origin: Map<object, Instance> } | undefined;
  const locateFn = (path: string): Located | undefined => {
    // Tracking runs `render` props again; the functions they create need not stay in the table.
    tracked ??= track(container, {
      ...build,
      images: { ...images, generated: new ImageIds(images.registered) },
    });
    const inst = locate(tracked.doc, path, tracked.origin);
    if (!inst) return undefined;
    const stack = componentStack(inst);
    return {
      component: innermostComponent(stack) ?? inst.type,
      componentStack: stack,
    };
  };
  return {
    document: built.document,
    output: built.output,
    images: built.images,
    fonts: built.fonts,
    warnings: built.warnings,
    dynamic: built.dynamic,
    remote: built.remote,
    pending: built.pending,
    pageCount: opts.pageCount ?? 1,
    timings: { templateMs, treeMs: t2 - t1 },
    locate: locateFn,
    validate: () => validateTree(built.document).map((p) => ({ ...p, ...locateFn(p.path) })),
    rebuild: (pageCount) => fromContainer(container, frontEnd, { ...opts, pageCount }, 0, images),
  };
}

function track(
  container: Container,
  opts: Parameters<typeof buildTree>[1],
): { doc: Document; origin: Map<object, Instance> } {
  const r = buildTree(container, { ...opts, track: true });
  return { doc: r.document, origin: r.origin ?? new Map() };
}

/** Throws a TemplateError for the first schema problem, with its component stack. */
export function assertValidTree(result: TreeResult): void {
  const [first] = result.validate();
  if (first) {
    throw new TemplateError(`The document tree is invalid at ${first.path}: ${first.message}`, {
      kind: 'schema',
      path: first.path,
      componentStack: first.componentStack,
    });
  }
}
