import type { Document } from '@inkoxide/schema';
import {
  assertValidTree,
  innermostComponent,
  loadTailwind,
  TemplateError,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/tree';
import type { z } from 'zod';

import {
  countPages,
  type EngineStatus,
  engineStatus,
  initEngine,
  refusedEmoji,
  renderPdf,
  renderSignal,
  startupError,
  untilAborted,
  version,
} from './engine.ts';
import { fontsPending, loadRegisteredFonts } from './font.ts';
import { engineHost } from './host.ts';
import { type ImagePolicies, loadEmojiImages, loadImages } from './images.ts';
import { checkImageLimits, checkOptionTypes, DEFAULT_IMAGE_TIMEOUT_MS, isProduction } from './policy.ts';
import { REMOTE_IMAGES_VARIABLE, remotePolicy } from './remote.ts';
import { type Abortable, type DataIssue, RenderError, type RenderOpts, type RenderResult } from './types.ts';

/**
 * A template for `generate`: a component that takes `{ data }` (a React, Vue, Svelte or Angular
 * component: `C`), the zod schema of that data and the static images it refers to by id.
 */
export interface TemplateLike<S extends z.ZodType = z.ZodType, C = unknown> {
  meta: { id: string };
  schema: S;
  Template: C;
  images?: Record<string, Uint8Array>;
}

/**
 * What a front end (`@inkoxide/react`, `@inkoxide/vue`, …) gives the pipeline: how to build the
 * document tree of what its users render (`Input`: an element, a component and its props), and
 * of a template's component (`C`) with its data.
 */
export interface FrontEnd<Input, C> {
  /** The tree of `input`, built with `opts`. Throws TemplateError for a problem in the template. */
  tree(input: Input, opts: TreeOptions): TreeResult;
  /** What renders the template component `Template` with `{ data }`. */
  withData(Template: C, data: unknown): Input;
  /** The name of the component `input` renders, for errors (`Invoice`); `'document'` when unknown. */
  name(input: Input): string;
}

/** The options of `generate`: those of `renderToBuffer`. */
export type GenerateOptions = RenderToBufferOptions;

let engine: Promise<EngineStatus> | undefined;

/**
 * Tailwind's design system (when `@inkoxide/tailwind` is installed), the bundled fonts and
 * hyphenation patterns and the schema check, once per process; then the fonts added with
 * `Font.register` since the last call. Every render calls it, so calling it yourself only moves
 * that cost to startup. A start-up failure is an `INTERNAL` RenderError, and the next call tries
 * again; a font that cannot be loaded, `INVALID_FONT`.
 */
export async function init(): Promise<EngineStatus> {
  engine ??= (async () => {
    await loadTailwind();
    return initEngine();
  })().catch((e: unknown) => {
    // The next call starts it again: the module may download then, say.
    engine = undefined;
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
function templateTree<S extends z.ZodType, I, C>(
  fe: FrontEnd<I, C>,
  t: TemplateLike<S, C>,
  data: z.output<S>,
  opts: RenderOpts,
  policies: ImagePolicies,
): TreeResult {
  const imageOptions = treeImageOptions(policies);
  try {
    return fe.tree(fe.withData(t.Template, data), {
      ...imageOptions,
      imageIds: Object.keys({ ...t.images, ...opts.images }),
    });
  } catch (e) {
    fromTemplateError(e, t.meta.id);
  }
}

/**
 * The image policies of a render: its options, else the environment variables and defaults. An
 * option of the wrong type, or a limit that is not a number from 0, throws.
 */
function imagePolicies(opts: RenderOpts): ImagePolicies {
  checkOptionTypes(opts);
  checkImageLimits(opts);
  const files = engineHost().files;
  if (!files && opts.localImages !== undefined) {
    throw new RenderError(
      'ENGINE_REJECTED',
      'The render option localImages needs a file system, which is not available here: pass the bytes in images.',
      { path: 'options.localImages' },
    );
  }
  return {
    remote: remotePolicy(opts.remoteImages),
    local: files?.localPolicy(opts.localImages),
    timeoutMs: opts.imageTimeoutMs ?? DEFAULT_IMAGE_TIMEOUT_MS,
  };
}

/** What tree building needs to know of the image policies. */
function treeImageOptions({
  remote,
  local,
}: ImagePolicies): Pick<TreeOptions, 'remoteImages' | 'remoteImagesElsewhere' | 'localImages'> {
  return {
    remoteImages: remote,
    remoteImagesElsewhere: REMOTE_IMAGES_VARIABLE,
    localImages: local !== undefined,
  };
}

/** Most passes for `render` props: each one renders with the page count of the previous. */
const MAX_PASSES = 4;

/**
 * Template + data → tree, without the engine (for tree snapshots and previews). `renderSettled`
 * renders it; pass both the same `opts`. It is synchronous, so it cannot load what `init` loads:
 * before `init` has finished, or while fonts added with `Font.register` are not loaded, it
 * throws an `INTERNAL` RenderError that says to call `init`.
 */
export function renderTree<S extends z.ZodType, I, C>(
  fe: FrontEnd<I, C>,
  t: TemplateLike<S, C>,
  data: unknown,
  opts: RenderOpts = {},
): TreeResult {
  if (engineStatus() === undefined || fontsPending()) {
    throw new RenderError(
      'INTERNAL',
      'renderTree needs Tailwind, the engine and the fonts loaded: call `await init()` first, and again after Font.register.',
    );
  }
  const parsed = parseData(t, data);
  // Before the template runs: a bad option is not a template error.
  const tree = templateTree(fe, t, parsed, opts, imagePolicies(opts));
  return withTemplateImages(t, tree, opts.images);
}

/**
 * Static template images (a logo) come with the tree, so it renders on its own. An image the
 * caller passes under the same id replaces the template's, as in `generate`.
 */
function withTemplateImages(
  t: Pick<TemplateLike, 'images'>,
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
 * The whole pipeline: data → zod → the front end's render → tree (+ Tailwind) → ajv (dev) →
 * engine → PDF, with per-stage timings.
 */
export async function generate<S extends z.ZodType, I, C>(
  fe: FrontEnd<I, C>,
  t: TemplateLike<S, C>,
  data: unknown,
  opts: GenerateOptions = {},
): Promise<RenderResult> {
  const t0 = performance.now();
  await init();
  const parsed = parseData(t, data);
  // Before the template runs: a bad option is not a template error.
  const policies = imagePolicies(opts);
  let tree = templateTree(fe, t, parsed, opts, policies);

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
  const settled = await settle(tree, { ...t.images }, opts, policies);
  tree = settled.tree;
  const { pdf, pages, passes, fetchMs } = settled;
  const e1 = performance.now();
  const v = version();
  return {
    pdf,
    timings: {
      // Rebuilds reuse the first render of the template: they count as tree time, not engine time.
      templateMs: first.timings.templateMs,
      treeMs: first.timings.treeMs + settled.treeMs,
      validateMs,
      engineMs: e1 - e0 - fetchMs - settled.treeMs,
      fetchMs,
      totalMs: e1 - t0,
    },
    passes,
    pages,
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
): Promise<Settled> {
  await init();
  return settle(first, images, opts, imagePolicies(opts));
}

/** What {@link renderSettled} gives. */
interface Settled {
  tree: TreeResult;
  pdf: Uint8Array;
  pages: number;
  passes: number;
  images: Record<string, Uint8Array>;
  fetchMs: number;
  treeMs: number;
}

/** {@link renderSettled} under image policies already read from its options. */
async function settle(
  first: TreeResult,
  images: Record<string, Uint8Array>,
  opts: RenderOpts & Abortable,
  policies: ImagePolicies,
): Promise<Settled> {
  // One time limit for the whole render: its images and every pass.
  const signal = renderSignal(opts);
  const passOpts = { ...opts, signal, timeoutMs: undefined };
  const registered = { ...images, ...opts.images };
  const all: Record<string, Uint8Array> = { ...registered };
  let fetchMs = 0;
  const render = async (tr: TreeResult): Promise<Uint8Array> => {
    checkIds(tr, registered);
    Object.assign(all, tr.images);
    const missing = { remote: notIn(tr.remote, all), pending: notIn(tr.pending, all) };
    // Emoji images that cannot be loaded leave their emoji to the fonts, with a warning.
    const emoji = new Set(Object.values(tr.document.emojiImages ?? {}));
    const isEmoji = (id: string) => emoji.has(id);
    const f0 = performance.now();
    const [images, emojiImages] = await untilAborted(
      Promise.all([
        loadImages(
          split(missing, (id) => !isEmoji(id)),
          registered,
          policies,
        ),
        loadEmojiImages(split(missing, isEmoji), registered, policies),
      ]),
      signal,
    );
    Object.assign(all, images, emojiImages.loaded);
    fetchMs += performance.now() - f0;
    dropEmoji(tr, 'could not be loaded', (_, id) => emojiImages.failed[id]);
    // So are those whose image the engine refuses: not an image, or one too large.
    const refused = emoji.size > 0 ? await untilAborted(refusedEmoji(tr.document, all), signal) : {};
    dropEmoji(tr, 'cannot be drawn', (sequence) => refused[sequence]);
    try {
      return await renderPdf(tr.document, passOpts, all, tr.output);
    } catch (e) {
      throw locateEngineError(e, tr);
    }
  };
  let tree = own(first);
  let pdf = await render(tree);
  let pages = countPages(pdf);
  let passes = 1;
  let treeMs = 0;
  while (tree.dynamic && passes < MAX_PASSES && pages !== tree.pageCount) {
    const r0 = performance.now();
    try {
      tree = own(tree.rebuild(pages));
    } catch (e) {
      fromTemplateError(e);
    }
    treeMs += performance.now() - r0;
    pdf = await render(tree);
    pages = countPages(pdf);
    passes++;
  }
  if (tree.dynamic && pages !== tree.pageCount) {
    tree.warnings.push(
      `render props did not settle on a page count after ${MAX_PASSES} passes; page numbers they show may be off.`,
    );
  }
  return { tree, pdf, pages, passes, images: all, fetchMs, treeMs };
}

/**
 * `tree` with its own `emojiImages` and `warnings`, which a render changes: the caller's tree
 * stays as it was, to render again.
 */
function own(tree: TreeResult): TreeResult {
  const { emojiImages } = tree.document;
  const document = emojiImages ? { ...tree.document, emojiImages: { ...emojiImages } } : tree.document;
  return { ...tree, document, warnings: [...tree.warnings] };
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

/** The remote and pending images of `tree` whose id passes `keep`. */
function split(
  tree: Pick<TreeResult, 'remote' | 'pending'>,
  keep: (id: string) => boolean,
): Pick<TreeResult, 'remote' | 'pending'> {
  const pick = <T>(from: Record<string, T>) =>
    Object.fromEntries(Object.entries(from).filter(([id]) => keep(id)));
  return { remote: pick(tree.remote), pending: pick(tree.pending) };
}

/**
 * Takes the emoji for which `why` gives a reason (their image `failed`) out of the document, with
 * a warning each.
 */
function dropEmoji(
  tree: TreeResult,
  failed: string,
  why: (sequence: string, id: string) => string | undefined,
): void {
  const map = tree.document.emojiImages;
  if (!map) return;
  for (const [sequence, id] of Object.entries(map)) {
    const reason = id === undefined ? undefined : why(sequence, id);
    if (reason === undefined) continue;
    delete map[sequence];
    tree.warnings.push(`The image of the emoji ${sequence} ${failed}, so a font draws it: ${reason}`);
  }
}

/** The entries of `from` whose id `have` lacks. */
function notIn<T>(from: Record<string, T>, have: Record<string, unknown>): Record<string, T> {
  return Object.fromEntries(Object.entries(from).filter(([id]) => !Object.hasOwn(have, id)));
}

/**
 * Adds the component that produced the node at the engine's error path. A render option's path
 * (`options.threads`) is in no component.
 */
function locateEngineError(e: unknown, tree: TreeResult): unknown {
  if (!(e instanceof RenderError) || !e.path || e.path === 'options' || e.path.startsWith('options.'))
    return e;
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
 * A document straight to PDF bytes, rendered by the front end `fe`: no template registry, schema
 * or sample data. A template is any component and its data are its props.
 */
export async function renderToBuffer<I, C>(
  fe: FrontEnd<I, C>,
  input: I,
  opts: RenderToBufferOptions = {},
): Promise<Uint8Array> {
  await init();
  const name = fe.name(input);
  // Outside the try: a bad option is not a template error.
  const policies = imagePolicies(opts);
  const imageOptions = treeImageOptions(policies);
  let tree: TreeResult;
  try {
    tree = fe.tree(input, {
      ...imageOptions,
      imageIds: Object.keys(opts.images ?? {}),
    });
    if (opts.validate ?? !isProduction()) assertValidTree(tree);
  } catch (e) {
    fromTemplateError(e, name);
  }
  const { pdf } = await settle(tree, {}, opts, policies);
  return pdf;
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
