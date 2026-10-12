import { writeFile } from 'node:fs/promises';
import { Readable } from 'node:stream';

import {
  assertValidTree,
  innermostComponent,
  loadTailwind,
  renderTemplate,
  TemplateError,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/react';
import type { Document } from '@inkoxide/schema';
import { type ComponentType, createElement, type ReactElement } from 'react';
import type { z } from 'zod';

import {
  countPages,
  type EngineStatus,
  engineStatus,
  initEngine,
  renderPdf,
  startupError,
  version,
} from './engine.ts';
import { fontsPending, loadRegisteredFonts } from './font.ts';
import { type ImagePolicies, loadImages } from './images.ts';
import { localPolicy } from './local.ts';
import { checkImageLimits, DEFAULT_IMAGE_TIMEOUT_MS } from './policy.ts';
import { remotePolicy } from './remote.ts';
import { type DataIssue, RenderError, type RenderOpts, type RenderResult } from './types.ts';

const isProduction = (): boolean => process.env.NODE_ENV === 'production';

/**
 * A template for `generate`: a component that takes `{ data }`, the zod schema of that data and
 * the static images it refers to by id.
 */
export interface TemplateLike<S extends z.ZodType = z.ZodType> {
  meta: { id: string };
  schema: S;
  Template: ComponentType<{ data: z.output<S> }>;
  images?: Record<string, Uint8Array>;
}

/** The options of `generate`: those of `renderToBuffer`. */
export type GenerateOptions = RenderToBufferOptions;

let engine: Promise<EngineStatus> | undefined;

/**
 * Tailwind's design system (when `@inkoxide/tailwind` is installed), the bundled fonts and
 * hyphenation patterns and the schema check, once per process; then the fonts added with
 * `Font.register` since the last call. Every render calls it, so calling it yourself only moves
 * that cost to startup. A start-up failure is an
 * `INTERNAL` RenderError; a font that cannot be loaded, `INVALID_FONT`.
 */
export async function init(): Promise<EngineStatus> {
  engine ??= (async () => {
    await loadTailwind();
    return initEngine();
  })().catch((e: unknown) => {
    throw startupError(e);
  });
  const status = await engine;
  await loadRegisteredFonts();
  return status;
}

const withArticle = (type: string) => (/^[aeiou]/.test(type) ? `an ${type}` : `a ${type}`);

/**
 * zod's type messages read as parser output ("Invalid input: expected string, received undefined");
 * people editing data need "Missing: expected a string". Other messages lose the generic prefix.
 */
export function issueMessage(message: string, code: string): string {
  const typed = /expected (\w+), received (\w+)/.exec(message);
  if (code === 'invalid_type' && typed) {
    const [, expected = '', received = ''] = typed;
    return received === 'undefined'
      ? `Missing: expected ${withArticle(expected)}`
      : `Expected ${withArticle(expected)}, got ${received === 'null' ? 'null' : withArticle(received)}`;
  }
  const rest = message.replace(/^Invalid input: /, '');
  return rest.charAt(0).toUpperCase() + rest.slice(1);
}

function issuesOf(error: z.ZodError): DataIssue[] {
  return error.issues.map((i) => ({
    path: i.path
      .map((p, n) => (typeof p === 'number' ? `[${p}]` : n === 0 ? String(p) : `.${String(p)}`))
      .join(''),
    message: issueMessage(i.message, i.code),
    code: i.code,
  }));
}

/** Checks data against a template's schema and returns the parsed value (defaults applied). */
export function parseData<S extends z.ZodType>(t: TemplateLike<S>, data: unknown): z.output<S> {
  const parsed = t.schema.safeParse(data);
  if (!parsed.success) {
    const issues = issuesOf(parsed.error);
    const first = issues[0];
    throw new RenderError(
      'INVALID_DATA',
      `The data does not match the "${t.meta.id}" schema${first ? `: ${first.path || '(root)'}: ${first.message}` : ''}.`,
      { issues },
    );
  }
  return parsed.data;
}

function fromTemplateError(e: unknown, id?: string): never {
  if (e instanceof TemplateError) {
    throw new RenderError(e.kind === 'schema' ? 'INVALID_TREE' : 'TEMPLATE_ERROR', e.message, {
      path: e.path,
      componentStack: e.componentStack,
      component: innermostComponent(e.componentStack),
      className: e.className,
      cause: e,
    });
  }
  const what = id === undefined ? 'the document' : `"${id}"`;
  throw new RenderError('INTERNAL', `Rendering ${what} failed: ${(e as Error).message}`, { cause: e });
}

/** The tree of a template for parsed data; a template error becomes a RenderError. */
function templateTree<S extends z.ZodType>(
  t: TemplateLike<S>,
  data: z.output<S>,
  opts: RenderOpts = {},
): TreeResult {
  // Outside the try: a bad option is not a template error.
  const imageOptions = treeImageOptions(opts);
  try {
    return renderTemplate(createElement(t.Template, { data }), {
      ...imageOptions,
      imageIds: Object.keys({ ...t.images, ...opts.images }),
    });
  } catch (e) {
    fromTemplateError(e, t.meta.id);
  }
}

/**
 * The image policies of a render: its options, else the environment variables and defaults. A
 * limit that is not a number from 0 throws.
 */
function imagePolicies(opts: RenderOpts): ImagePolicies {
  checkImageLimits(opts);
  return {
    remote: remotePolicy(opts.remoteImages),
    local: localPolicy(opts.localImages),
    timeoutMs: opts.imageTimeoutMs ?? DEFAULT_IMAGE_TIMEOUT_MS,
  };
}

/** What tree building needs to know of the image policies. */
function treeImageOptions(opts: RenderOpts): Pick<TreeOptions, 'remoteImages' | 'localImages'> {
  const { remote, local } = imagePolicies(opts);
  return { remoteImages: remote, localImages: local !== undefined };
}

/** Most passes for `render` props: each one renders with the page count of the previous. */
const MAX_PASSES = 4;

/**
 * Template + data → tree, without the engine (for tree snapshots and previews). `renderSettled`
 * renders it; pass both the same `opts`. It is synchronous, so it cannot load what `init` loads:
 * before `init` has finished, or while fonts added with `Font.register` are not loaded, it
 * throws an `INTERNAL` RenderError that says to call `init`.
 */
export function renderTree<S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts: RenderOpts = {},
): TreeResult {
  if (engineStatus() === undefined || fontsPending()) {
    throw new RenderError(
      'INTERNAL',
      'renderTree needs Tailwind, the engine and the fonts loaded: call `await init()` first, and again after Font.register.',
    );
  }
  const tree = templateTree(t, parseData(t, data), opts);
  return withTemplateImages(t, tree, opts.images);
}

/**
 * Static template images (a logo) come with the tree, so it renders on its own. An image the
 * caller passes under the same id replaces the template's, as in `generate`.
 */
function withTemplateImages(
  t: TemplateLike,
  tree: TreeResult,
  passed: Record<string, Uint8Array> = {},
): TreeResult {
  if (!t.images) return tree;
  const own = Object.fromEntries(Object.entries(t.images).filter(([id]) => !Object.hasOwn(passed, id)));
  return {
    ...tree,
    images: { ...own, ...tree.images },
    rebuild: (pages) => withTemplateImages(t, tree.rebuild(pages), passed),
  };
}

/**
 * The whole pipeline: data → zod → React → tree (+ Tailwind) → ajv (dev) → engine → PDF,
 * with per-stage timings.
 */
export async function generate<S extends z.ZodType>(
  t: TemplateLike<S>,
  data: unknown,
  opts: GenerateOptions = {},
): Promise<RenderResult> {
  const t0 = performance.now();
  await init();
  const parsed = parseData(t, data);
  let tree = templateTree(t, parsed, opts);

  let validateMs = 0;
  if (opts.validate ?? !isProduction()) {
    const v0 = performance.now();
    try {
      assertValidTree(tree);
    } catch (e) {
      fromTemplateError(e, t.meta.id);
    }
    validateMs = performance.now() - v0;
  }

  const e0 = performance.now();
  const first = tree;
  const settled = await renderSettled(tree, { ...t.images }, opts);
  tree = settled.tree;
  const { pdf, passes, fetchMs } = settled;
  const e1 = performance.now();
  const v = version();
  return {
    pdf,
    timings: {
      // Rebuilds reuse the first React render: they count as tree time, not engine time.
      reactMs: first.timings.reactMs,
      treeMs: first.timings.treeMs + settled.treeMs,
      validateMs,
      engineMs: e1 - e0 - fetchMs - settled.treeMs,
      fetchMs,
      totalMs: e1 - t0,
    },
    passes,
    pages: countPages(pdf),
    bytes: pdf.byteLength,
    schemaVersion: v.schema,
    engineVersion: v.engine,
    fonts: tree.fonts,
    warnings: tree.warnings,
    document: tree.document,
    images: settled.images,
  };
}

/**
 * Renders a tree; when it uses `render` props, renders again with the page count until the count
 * stops changing (at most MAX_PASSES renders). Before each render, loads the images of the tree it
 * does not have yet (a rebuild can add some) under the policies of `opts`. `images` are images by
 * id besides `opts.images` and the tree's own. Returns the tree that produced the PDF, the images
 * it was rendered with by id, the time spent loading images and the time spent rebuilding the
 * tree for the page count.
 */
export async function renderSettled(
  first: TreeResult,
  images: Record<string, Uint8Array>,
  opts: RenderOpts = {},
): Promise<{
  tree: TreeResult;
  pdf: Uint8Array;
  pages: number;
  passes: number;
  images: Record<string, Uint8Array>;
  fetchMs: number;
  treeMs: number;
}> {
  await init();
  const registered = { ...images, ...opts.images };
  const policies = imagePolicies(opts);
  const all: Record<string, Uint8Array> = { ...registered };
  let fetchMs = 0;
  const render = async (tr: TreeResult): Promise<Uint8Array> => {
    checkIds(tr, registered);
    Object.assign(all, tr.images);
    const missing = { remote: notIn(tr.remote, all), pending: notIn(tr.pending, all) };
    const f0 = performance.now();
    Object.assign(all, await loadImages(missing, registered, policies));
    fetchMs += performance.now() - f0;
    try {
      return await renderPdf(tr.document, opts, all);
    } catch (e) {
      throw locateEngineError(e, tr);
    }
  };
  let tree = first;
  let pdf = await render(tree);
  let passes = 1;
  let treeMs = 0;
  while (tree.dynamic && passes < MAX_PASSES) {
    const pages = countPages(pdf);
    if (pages === tree.pageCount) break;
    const r0 = performance.now();
    try {
      tree = tree.rebuild(pages);
    } catch (e) {
      fromTemplateError(e);
    }
    treeMs += performance.now() - r0;
    pdf = await render(tree);
    passes++;
  }
  const pages = countPages(pdf);
  if (tree.dynamic && pages !== tree.pageCount) {
    tree.warnings.push(
      `render props did not settle on a page count after ${MAX_PASSES} passes; page numbers they show may be off.`,
    );
  }
  return { tree, pdf, pages, passes, images: all, fetchMs, treeMs };
}

/**
 * An image passed in under an id the tree gave another image would take its place. A tree built
 * with the caller's ids never does; one built without them (`renderTree` without the `images`
 * option) can.
 */
function checkIds(tree: TreeResult, registered: Record<string, Uint8Array>): void {
  const clash =
    [...Object.keys(tree.remote), ...Object.keys(tree.pending)].find((id) => Object.hasOwn(registered, id)) ??
    Object.keys(tree.images).find(
      (id) => Object.hasOwn(registered, id) && registered[id] !== tree.images[id],
    );
  if (clash === undefined) return;
  throw new RenderError(
    'TEMPLATE_ERROR',
    `The image id "${clash}" is passed in, but the tree gave it to another image. Build the tree with the same images option (renderTree(template, data, options)), or rename the image.`,
  );
}

/** The entries of `from` whose id `have` lacks. */
function notIn<T>(from: Record<string, T>, have: Record<string, unknown>): Record<string, T> {
  return Object.fromEntries(Object.entries(from).filter(([id]) => !Object.hasOwn(have, id)));
}

/** Adds the component that produced the node at the engine's error path. */
function locateEngineError(e: unknown, tree: TreeResult): unknown {
  if (!(e instanceof RenderError) || !e.path) return e;
  const at = tree.locate(e.path);
  if (!at) return e;
  return new RenderError(e.code, `${e.message} in <${at.component}>`, {
    status: e.status,
    path: e.path,
    component: at.component,
    componentStack: at.componentStack,
    engineCode: e.engineCode,
    cause: e,
  });
}

export interface RenderToBufferOptions extends RenderOpts {
  /** Check the tree against the engine schema first (default: outside production). */
  validate?: boolean;
}

/**
 * A `<Document>` element straight to PDF bytes: no template registry, schema or sample data. A
 * template is any component and its data are its props.
 *
 * ```tsx
 * const pdf = await renderToBuffer(<Invoice invoice={invoice} />);
 * ```
 */
export async function renderToBuffer(
  element: ReactElement,
  opts: RenderToBufferOptions = {},
): Promise<Uint8Array> {
  await init();
  const name = componentName(element);
  // Outside the try: a bad option is not a template error.
  const imageOptions = treeImageOptions(opts);
  let tree: TreeResult;
  try {
    tree = renderTemplate(element, {
      ...imageOptions,
      imageIds: Object.keys(opts.images ?? {}),
    });
    if (opts.validate ?? !isProduction()) assertValidTree(tree);
  } catch (e) {
    fromTemplateError(e, name);
  }
  const { pdf } = await renderSettled(tree, {}, opts);
  return pdf;
}

/** `renderToBuffer`, then writes the PDF to `path`; a failed write is an `INTERNAL` RenderError. */
export async function renderToFile(
  element: ReactElement,
  path: string,
  opts: RenderToBufferOptions = {},
): Promise<void> {
  const pdf = await renderToBuffer(element, opts);
  try {
    await writeFile(path, pdf);
  } catch (e) {
    throw new RenderError('INTERNAL', `Cannot write the PDF to ${path}: ${(e as Error).message}`, {
      cause: e,
    });
  }
}

/**
 * `renderToBuffer` as a Node stream, to pipe into an HTTP response:
 *
 * ```tsx
 * (await renderToStream(<Invoice invoice={invoice} />)).pipe(res);
 * ```
 *
 * The engine writes the whole file at once, so the stream starts when the PDF is ready, and a
 * render that fails rejects the promise instead of erroring the stream.
 */
export async function renderToStream(
  element: ReactElement,
  opts: RenderToBufferOptions = {},
): Promise<Readable> {
  const pdf = await renderToBuffer(element, opts);
  // Buffer chunks, as from a file stream; the view shares the PDF's memory.
  return Readable.from([Buffer.from(pdf.buffer, pdf.byteOffset, pdf.byteLength)], { objectMode: false });
}

/** The name of the element's component, for errors; JavaScript callers can pass anything. */
function componentName(element: ReactElement): string {
  const t = (element as { type?: unknown } | null | undefined)?.type as
    | string
    | { displayName?: string; name?: string }
    | null
    | undefined;
  if (typeof t === 'string') return t;
  return (t && (t.displayName ?? t.name)) || 'document';
}

/**
 * Renders an already-built document, such as a stored `result.document` with `result.images`.
 * Its images are ids: `images` and `opts.images` (which wins) provide them, and the image
 * policies of `opts` do not apply.
 */
export async function renderDocument(
  doc: Document,
  images: Record<string, Uint8Array> = {},
  opts: RenderOpts = {},
): Promise<{ pdf: Uint8Array; pages: number; engineMs: number }> {
  await init();
  const t0 = performance.now();
  const pdf = await renderPdf(doc, opts, { ...images, ...opts.images });
  return { pdf, pages: countPages(pdf), engineMs: performance.now() - t0 };
}
