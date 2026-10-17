/**
 * The engine boundary: everything else in this package talks to the engine through these
 * functions, which go through the entry's `EngineHost` (host.ts).
 */
import { fonts, type OutputOptions } from '@inkoxide/react';
import type { Document } from '@inkoxide/schema';
import { SCHEMA_VERSION } from '@inkoxide/schema';
import type { RenderOptions } from 'inkoxide';

import { latin1 } from './bytes.ts';
import { DEFAULT_FACES } from './fonts.ts';
import { engineHost, type EngineVersion } from './host.ts';
import { HYPHENATION_FILES } from './hyphenation.ts';
import { checkEncryptionTypes, checkOptionTypes, timerMs } from './policy.ts';
import { type Abortable, RenderError, type RenderOpts } from './types.ts';

export type { EngineVersion } from './host.ts';

export interface EngineStatus {
  version: EngineVersion;
  fonts: string[];
  hyphenation: Record<string, boolean>;
  warnings: string[];
}

let status: EngineStatus | undefined;

/**
 * The bundled faces the engine does not have yet, with the files they are in, when the host reads
 * them as documents use them (`defersFonts`); each one's load once it has started.
 */
const lazyFaces = new Map<string, string>();
const lazyLoads = new Map<string, Promise<void>>();

/**
 * Process-wide setup, once: fonts, hyphenation patterns and the schema version check. Rejects with
 * an `INTERNAL` RenderError with the fix when fonts are missing or the engine speaks another
 * schema version.
 */
export async function initEngine(): Promise<EngineStatus> {
  if (status) return status;
  try {
    return await startEngine();
  } catch (e) {
    throw startupError(e);
  }
}

/** A failure to start the engine: the install or the deployment is broken, not the document. */
export function startupError(e: unknown): RenderError {
  if (e instanceof RenderError) return e;
  const message = `The engine could not start: ${e instanceof Error ? e.message : String(e)}`;
  return new RenderError('INTERNAL', message, {
    engineCode: engineHost().isEngineError(e) ? e.code : undefined,
    cause: e,
  });
}

async function startEngine(): Promise<EngineStatus> {
  const host = engineHost();
  await host.start();
  const v = host.version();
  if (v.schema !== SCHEMA_VERSION) {
    throw new Error(
      `Schema mismatch: the inkoxide engine speaks schema v${v.schema} but @inkoxide/schema is v${SCHEMA_VERSION}. ` +
        'Install matching versions of inkoxide and @inkoxide/schema.',
    );
  }
  const fontsDir = host.assetLocation('fonts');
  const missing: string[] = [];
  for (const { file, ...face } of DEFAULT_FACES) {
    if (fonts.has(face.id)) continue;
    if (host.defersFonts) {
      fonts.register(face);
      lazyFaces.set(face.id, file);
      continue;
    }
    const bytes = await host.readAsset(`fonts/${file}`);
    if (bytes === undefined) {
      missing.push(face.id);
      continue;
    }
    fonts.register(face, bytes);
  }
  const warnings: string[] = [];
  if (fonts.ids().length === 0) {
    throw new Error(
      `No fonts found in ${fontsDir}. Set INKOXIDE_ASSETS_DIR to a folder with the package's fonts/ and hyphenation/ folders (its assets/).`,
    );
  }
  if (missing.length > 0) warnings.push(`Fonts missing from ${fontsDir}: ${missing.join(', ')}`);
  for (const id of fonts.ids()) {
    const bytes = fonts.bytes(id);
    if (bytes) await host.registerFont(id, bytes);
  }
  const hyphenation: Record<string, boolean> = {};
  for (const [lang, file] of Object.entries(HYPHENATION_FILES)) {
    const path = host.assetLocation(`hyphenation/${file}`);
    const bytes = await host.readAsset(`hyphenation/${file}`);
    if (bytes !== undefined) {
      await host.registerHyphenationPatterns(lang, bytes);
      hyphenation[lang] = true;
    } else {
      hyphenation[lang] = false;
      warnings.push(
        `${lang} hyphenation patterns not found (${path}). ` +
          "Text falls back to the engine's built-in patterns.",
      );
    }
  }
  for (const w of warnings) console.warn(`[inkoxide] ${w}`);
  // The fonts the engine has: not yet the bundled ones a document has not used (`defersFonts`).
  status = { version: v, fonts: fonts.ids().filter((id) => !lazyFaces.has(id)), hyphenation, warnings };
  return status;
}

/**
 * Hands the engine the bundled fonts `doc` declares and it does not have yet (a host with
 * `defersFonts`), and returns those of them that are only its fallback fonts, by their asset
 * paths: the engine fetches one of those only when a character needs it. The React tree declares
 * the fonts its text uses and its fallbacks. A font that cannot be read fails the render with an
 * `INTERNAL` RenderError.
 */
async function loadDocumentFonts(
  doc: Pick<Document, 'fonts' | 'fallbackFonts'>,
): Promise<Record<string, string>> {
  if (lazyFaces.size === 0) return {};
  const fallbacks = new Set(doc.fallbackFonts);
  const deferred: Record<string, string> = {};
  const loads: Promise<void>[] = [];
  // An instance (`source`) needs the font it draws with.
  for (const { id: declared, source } of doc.fonts ?? []) {
    const id = source ?? declared;
    const file = lazyFaces.get(id);
    if (file === undefined) continue;
    if (fallbacks.has(id)) {
      deferred[id] = `fonts/${file}`;
      continue;
    }
    let load = lazyLoads.get(id);
    if (!load) {
      const host = engineHost();
      load = (async () => {
        const path = `fonts/${file}`;
        const bytes = await host.readAsset(path);
        if (bytes === undefined) {
          throw new RenderError(
            'INTERNAL',
            `The bundled font ${id} could not be read from ${host.assetLocation(path)}.`,
          );
        }
        await host.registerFont(id, bytes);
        lazyFaces.delete(id);
        status?.fonts.push(id);
      })();
      lazyLoads.set(id, load);
      // A failed load is tried again by the next render.
      load.catch(() => lazyLoads.delete(id));
    }
    loads.push(load);
  }
  await Promise.all(loads);
  return deferred;
}

/** Registers one more font with the engine, after `initEngine` (fonts added with `Font.register`). */
export async function registerEngineFont(id: string, bytes: Uint8Array): Promise<void> {
  await engineHost().registerFont(id, bytes);
  status?.fonts.push(id);
}

export function engineStatus(): EngineStatus | undefined {
  return status;
}

export function version(): EngineVersion {
  return engineHost().version();
}

/**
 * An engine failure as a RenderError; the pipeline adds the component at `path`. A render option
 * the engine refuses (`INVALID_ARGUMENT` at `options.threads`, `options.creationDate`) is the
 * caller's to fix, so `ENGINE_REJECTED`; any other `INVALID_ARGUMENT` is a bug of this package.
 * `NOT_LOADED` (in a browser) is a bundled font or hyphenation file the engine could not fetch:
 * `INTERNAL`, as a bundled font that cannot be read, since the deployment is broken.
 */
export function toRenderError(e: unknown): RenderError {
  if (engineHost().isEngineError(e)) {
    const badOption = e.code === 'INVALID_ARGUMENT' && e.path.startsWith('options.');
    if (e.code === 'INTERNAL' || e.code === 'NOT_LOADED' || (e.code === 'INVALID_ARGUMENT' && !badOption)) {
      return new RenderError('INTERNAL', `Engine error ${e.code}: ${e.message}`, {
        engineCode: e.code,
        cause: e,
      });
    }
    return new RenderError('ENGINE_REJECTED', `${e.code}: ${e.message}${e.path ? ` (at ${e.path})` : ''}`, {
      engineCode: e.code,
      path: e.path || undefined,
      cause: e,
    });
  }
  return new RenderError('INTERNAL', e instanceof Error ? e.message : String(e), { cause: e });
}

/**
 * The entries of a document's `emojiImages` (sequence → image id) whose image the engine refuses,
 * as it would refuse the document: bytes that are not an image, or an image over the size limits.
 * Sequence → why.
 */
export async function refusedEmoji(
  doc: Pick<Document, 'emojiImages' | 'fonts' | 'fallbackFonts'>,
  images: Record<string, Uint8Array>,
): Promise<Record<string, string>> {
  const emojiImages = doc.emojiImages ?? {};
  const map: Record<string, string> = {};
  const own: Record<string, Uint8Array> = {};
  for (const [sequence, id] of Object.entries(emojiImages)) {
    if (id === undefined) continue;
    map[sequence] = id;
    if (Object.hasOwn(images, id)) own[id] = images[id] as Uint8Array;
  }
  const refused: Record<string, string> = {};
  const at = 'emojiImages.';
  await loadDocumentFonts(doc);
  // `validate` returns at most 100 problems: check again without those found until none is left.
  for (;;) {
    // The document's fonts draw the text of SVG images.
    const check = {
      version: SCHEMA_VERSION,
      fonts: doc.fonts ?? [],
      emojiImages: map,
      pages: [{ size: { width: 100, height: 100 } }],
    };
    const found = (await engineHost().validate(JSON.stringify(check), own)).filter((e) =>
      e.path.startsWith(at),
    );
    for (const e of found) {
      const sequence = e.path.slice(at.length);
      refused[sequence] = e.message;
      delete map[sequence];
    }
    if (found.length === 0 || Object.keys(map).length === 0) return refused;
  }
}

/**
 * Renders a document tree to PDF bytes, off the calling thread. `output` holds the render options its
 * `<Document>` set (`conformance`, the passwords and permissions); `opts` wins over them.
 */
export async function renderPdf(
  doc: Document,
  opts: RenderOpts & Abortable,
  images: Record<string, Uint8Array>,
  output: OutputOptions = {},
): Promise<Uint8Array> {
  checkOptionTypes(opts);
  // The <Document> passwords and permissions, which JavaScript can give any type.
  checkEncryptionTypes(opts.encryption ?? output.encryption);
  const options: RenderOptions = { images };
  if (opts.attachments !== undefined) options.attachments = opts.attachments;
  if (opts.creationDate != null) options.creationDate = opts.creationDate;
  if (opts.threads != null) options.threads = opts.threads;
  if (opts.compress != null) options.compress = opts.compress;
  if (opts.subsetFonts != null) options.subsetFonts = opts.subsetFonts;
  const pdfA = opts.pdfA ?? output.pdfA;
  if (pdfA !== undefined) options.pdfA = pdfA;
  const encryption = opts.encryption ?? output.encryption;
  if (encryption !== undefined) options.encryption = encryption;
  const signal = withTimeout(opts.signal, opts.timeoutMs);
  signal?.throwIfAborted();
  const deferredFonts = await loadDocumentFonts(doc);
  try {
    return await engineHost().render(JSON.stringify(doc), options, signal, deferredFonts);
  } catch (e) {
    if (signal?.aborted && e === signal.reason) throw e;
    throw toRenderError(e);
  }
}

/** `signal`, and a timeout after `timeoutMs`: a signal that aborts with whichever comes first. */
function withTimeout(
  signal: AbortSignal | undefined,
  timeoutMs: number | undefined,
): AbortSignal | undefined {
  const ms = timeoutMs === undefined ? undefined : timerMs(timeoutMs);
  if (ms === undefined) return signal;
  const timeout = AbortSignal.timeout(ms);
  return signal ? AbortSignal.any([signal, timeout]) : timeout;
}

/**
 * The page count of an engine PDF: `/Count` of the page tree root. The engine writes no object
 * streams, so the dictionary is plain text.
 */
export function countPages(pdf: Uint8Array): number {
  const s = latin1(pdf);
  const at = s.search(/\/Type\s*\/Pages\b/);
  if (at < 0) return 0;
  const open = s.lastIndexOf('<<', at);
  let depth = 0;
  let end = s.length;
  for (let i = open; i < s.length - 1; i++) {
    if (s[i] === '<' && s[i + 1] === '<') {
      depth++;
      i++;
    } else if (s[i] === '>' && s[i + 1] === '>') {
      depth--;
      i++;
      if (depth === 0) {
        end = i;
        break;
      }
    }
  }
  const m = /\/Count\s+(\d+)/.exec(s.slice(open, end));
  return m?.[1] ? Number(m[1]) : 0;
}
