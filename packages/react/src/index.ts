import type { Document } from '@inkoxide/schema';
import type { ReactNode } from 'react';

import { TemplateError } from './errors.ts';
import { fonts as defaultFonts, type FontRegistry } from './fonts/index.ts';
import { type Container, componentStack, type Instance } from './reconciler/instance.ts';
import { renderToInstances } from './reconciler/render.ts';
import { type BuildResult, buildTree, type RemoteImagePolicy } from './tree/build.ts';
import { locate } from './tree/locate.ts';
import { type SchemaProblem, validateTree } from './tree/validate.ts';

export type { Painter } from './canvas.ts';
export * from './components/index.ts';
export { TemplateError, type TemplateErrorKind } from './errors.ts';
export * from './fonts/index.ts';
export { usePage } from './page.ts';
export { PAGE_SIZES, type RemoteImagePolicy, remoteAllowed } from './tree/build.ts';
export { parsePath } from './tree/locate.ts';
export { weightOf as fontWeightValue } from './tree/style.ts';
export { type SchemaProblem, validateTree } from './tree/validate.ts';

export interface TreeOptions {
  fonts?: FontRegistry;
  /** Remote image URLs `<Image src>` may use; they are listed in `remote` for the caller to fetch. */
  remoteImages?: RemoteImagePolicy;
  /** Page count for `render` props (from a previous render); 1 when omitted. */
  pageCount?: number;
}

export interface Located {
  /** The component at the path (`Text`, `Cell`). */
  component: string;
  /** React-style component stack. */
  componentStack: string;
}

export interface TreeResult extends Omit<BuildResult, 'origin'> {
  /** Milliseconds: React render to instances, and instances to the schema tree. */
  timings: { reactMs: number; treeMs: number };
  /** Maps an engine/ajv path back to the component that produced it (rebuilds lazily). */
  locate(path: string): Located | undefined;
  /** ajv validation with paths mapped to components; `[]` when valid. */
  validate(): Array<SchemaProblem & Partial<Located>>;
  /** The page count the tree was built for (`render` props). */
  pageCount: number;
  /** The same template built for another page count, without rendering React again. */
  rebuild(pageCount: number): TreeResult;
}

/**
 * Renders a template element to an Inkoxide document. Synchronous: React renders once, then the
 * instances are walked into the schema tree. Throws TemplateError with a component stack.
 */
export function renderTemplate(element: ReactNode, opts: TreeOptions = {}): TreeResult {
  const t0 = performance.now();
  const container = renderToInstances(element);
  return fromContainer(container, opts, performance.now() - t0);
}

function fromContainer(container: Container, opts: TreeOptions, reactMs: number): TreeResult {
  const registry = opts.fonts ?? defaultFonts;
  const t1 = performance.now();
  const build = { fonts: registry, pageCount: opts.pageCount, remoteImages: opts.remoteImages };
  const built = buildTree(container, build);
  const t2 = performance.now();
  let tracked: { doc: Document; origin: Map<object, Instance> } | undefined;
  const locateFn = (path: string): Located | undefined => {
    tracked ??= track(container, build);
    const inst = locate(tracked.doc, path, tracked.origin);
    if (!inst) return undefined;
    const stack = componentStack(inst);
    return {
      component: stack.split('\n')[1]?.trim().replace(/^in /, '') ?? inst.type,
      componentStack: stack,
    };
  };
  return {
    document: built.document,
    images: built.images,
    fonts: built.fonts,
    warnings: built.warnings,
    dynamic: built.dynamic,
    remote: built.remote,
    pageCount: opts.pageCount ?? 1,
    timings: { reactMs, treeMs: t2 - t1 },
    locate: locateFn,
    validate: () => validateTree(built.document).map((p) => ({ ...p, ...locateFn(p.path) })),
    rebuild: (pageCount) => fromContainer(container, { ...opts, pageCount }, 0),
  };
}

function track(
  container: Container,
  opts: Parameters<typeof buildTree>[1],
): { doc: Document; origin: Map<object, Instance> } {
  const r = buildTree(container, { ...opts, track: true });
  return { doc: r.document, origin: r.origin ?? new Map() };
}

/** The document tree of a template element (the simple API). */
export async function renderToTree(element: ReactNode, opts: TreeOptions = {}): Promise<Document> {
  return renderTemplate(element, opts).document;
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
