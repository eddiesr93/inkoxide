import { writeFile } from 'node:fs/promises';
import { Readable } from 'node:stream';

import {
  assertValidTree,
  renderTemplate,
  TemplateError,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/react';
import type { Document } from '@inkoxide/schema';
import { initTailwind } from '@inkoxide/tailwind';
import { type ComponentType, createElement, type ReactElement } from 'react';
import type { z } from 'zod';

import { countPages, type EngineStatus, initEngine, renderPdf, version } from './engine.ts';
import { loadRegisteredFonts } from './font.ts';
import { type ImagePolicies, loadImages } from './images.ts';
import { localPolicy } from './local.ts';
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

export interface GenerateOptions extends RenderOpts {
  /** Validate the tree with ajv before the engine (default: outside production). */
  validate?: boolean;
}

let engine: Promise<EngineStatus> | undefined;

/**
 * Tailwind's design system, the bundled fonts and hyphenation patterns and the schema check,
 * once per process; then the fonts added with `Font.register` since the last call. Every render
 * calls it, so calling it yourself only moves that cost to startup.
 */
export async function init(): Promise<EngineStatus> {
  engine ??= (async () => {
    await initTailwind();
    return initEngine();
  })();
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

function fromTemplateError(e: unknown, id: string): never {
  if (e instanceof TemplateError) {
    throw new RenderError(e.kind === 'schema' ? 'INVALID_TREE' : 'TEMPLATE_ERROR', e.message, {
      path: e.path,
      componentStack: e.componentStack,
      component: e.componentStack?.split('\n')[1]?.trim().replace(/^in /, ''),
      className: e.className,
      cause: e,
    });
  }
  throw new RenderError('INTERNAL', `Rendering "${id}" failed: ${(e as Error).message}`, { cause: e });
}

function buildTree<S extends z.ZodType>(
  t: TemplateLike<S>,
  data: z.output<S>,
  opts: RenderOpts = {},
): TreeResult {
  try {
    return renderTemplate(createElement(t.Template, { data }), {
      ...treeImageOptions(opts),
      imageIds: Object.keys({ ...t.images, ...opts.images }),
    });
  } catch (e) {
    fromTemplateError(e, t.meta.id);
  }
}

/** The image policies of a render: its options, else the environment variables. */
function imagePolicies(opts: RenderOpts): ImagePolicies {
  return { remote: remotePolicy(opts.remoteImages), local: localPolicy(opts.localImages) };
}

/** What tree building needs to know of the image policies. */
function treeImageOptions(opts: RenderOpts): Pick<TreeOptions, 'remoteImages' | 'localImages'> {
  const { remote, local } = imagePolicies(opts);
  return { remoteImages: remote, localImages: local !== undefined };
}

/** Most passes for `render` props: each one renders with the page count of the previous. */
const MAX_PASSES = 4;

/** Template + data → tree, without the engine (for tree snapshots and previews). */
export function renderTree<S extends z.ZodType>(t: TemplateLike<S>, data: unknown): TreeResult {
  const tree = buildTree(t, parseData(t, data));
  return withTemplateImages(t, tree);
}

/** Static template images (a logo) come with the tree, so it renders on its own. */
function withTemplateImages(t: TemplateLike, tree: TreeResult): TreeResult {
  if (!t.images) return tree;
  return {
    ...tree,
    images: { ...t.images, ...tree.images },
    rebuild: (pages) => withTemplateImages(t, tree.rebuild(pages)),
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
  let tree = buildTree(t, parsed, opts);

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

  const f0 = performance.now();
  const registered = { ...t.images, ...opts.images };
  const loaded = await loadImages(tree, registered, imagePolicies(opts));
  const fetchMs = performance.now() - f0;
  const images = { ...t.images, ...tree.images, ...loaded, ...opts.images };
  const e0 = performance.now();
  const first = tree;
  const settled = await renderSettled(tree, images, opts);
  tree = settled.tree;
  const { pdf, passes } = settled;
  const e1 = performance.now();
  const v = version();
  return {
    pdf,
    timings: {
      // Rebuilds reuse the first React render; count the first build and the final rebuild.
      reactMs: first.timings.reactMs,
      treeMs: first.timings.treeMs + (tree === first ? 0 : tree.timings.treeMs),
      validateMs,
      engineMs: e1 - e0,
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
  };
}

/**
 * Renders a tree; when it uses `render` props, renders again with the page count until the count
 * stops changing (at most MAX_PASSES renders). Returns the tree that produced the PDF.
 */
export async function renderSettled(
  first: TreeResult,
  images: Record<string, Uint8Array>,
  opts: RenderOpts = {},
): Promise<{ tree: TreeResult; pdf: Uint8Array; pages: number; passes: number }> {
  const render = async (tr: TreeResult): Promise<Uint8Array> => {
    try {
      return await renderPdf(tr.document, opts, images);
    } catch (e) {
      throw locateEngineError(e, tr);
    }
  };
  let tree = first;
  let pdf = await render(tree);
  let passes = 1;
  while (tree.dynamic && passes < MAX_PASSES) {
    const pages = countPages(pdf);
    if (pages === tree.pageCount) break;
    tree = tree.rebuild(pages);
    pdf = await render(tree);
    passes++;
  }
  const pages = countPages(pdf);
  if (tree.dynamic && pages !== tree.pageCount) {
    tree.warnings.push(
      `render props did not settle on a page count after ${MAX_PASSES} passes; page numbers they show may be off.`,
    );
  }
  return { tree, pdf, pages, passes };
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
  /** Images the document refers to by id (`<Image src="logo">`). */
  images?: Record<string, Uint8Array>;
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
  let tree: TreeResult;
  try {
    tree = renderTemplate(element, {
      ...treeImageOptions(opts),
      imageIds: Object.keys(opts.images ?? {}),
    });
    if (opts.validate ?? !isProduction()) assertValidTree(tree);
  } catch (e) {
    fromTemplateError(e, name);
  }
  const loaded = await loadImages(tree, opts.images ?? {}, imagePolicies(opts));
  const { pdf } = await renderSettled(tree, { ...tree.images, ...loaded, ...opts.images }, opts);
  return pdf;
}

/** `renderToBuffer`, then writes the PDF to `path`. */
export async function renderToFile(
  element: ReactElement,
  path: string,
  opts: RenderToBufferOptions = {},
): Promise<void> {
  await writeFile(path, await renderToBuffer(element, opts));
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

function componentName(element: ReactElement): string {
  const t = element.type as string | { displayName?: string; name?: string };
  return typeof t === 'string' ? t : (t.displayName ?? t.name ?? 'document');
}

/** Renders an already-built document (fixtures, benchmarks). */
export async function renderDocument(
  doc: Document,
  images: Record<string, Uint8Array> = {},
  opts: RenderOpts = {},
): Promise<{ pdf: Uint8Array; pages: number; engineMs: number }> {
  await init();
  const t0 = performance.now();
  const pdf = await renderPdf(doc, opts, images);
  return { pdf, pages: countPages(pdf), engineMs: performance.now() - t0 };
}
