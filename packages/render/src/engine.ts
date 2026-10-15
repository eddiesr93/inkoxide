/**
 * The engine boundary: the only module of this package that imports `inkoxide`. Everything else
 * talks to the engine through these functions.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { fonts, type OutputOptions } from '@inkoxide/react';
import type { Document } from '@inkoxide/schema';
import { SCHEMA_VERSION } from '@inkoxide/schema';
import * as ink from 'inkoxide';

import { DEFAULT_FACES } from './fonts.ts';
import { HYPHENATION_FILES } from './hyphenation.ts';
import { checkEncryptionTypes, checkOptionTypes } from './policy.ts';
import { RenderError, type RenderOpts } from './types.ts';

/**
 * Where the bundled fonts and hyphenation patterns are read from: `INKOXIDE_ASSETS_DIR` when set,
 * otherwise the package's own `assets/`, next to `dist/` (or `src/` in the repository).
 */
export function assetsDir(): string {
  if (process.env.INKOXIDE_ASSETS_DIR) return resolve(process.env.INKOXIDE_ASSETS_DIR);
  return resolve(dirname(fileURLToPath(import.meta.url)), '..', 'assets');
}

export interface EngineVersion {
  engine: string;
  schema: number;
  gitSha: string;
}

export interface EngineStatus {
  version: EngineVersion;
  fonts: string[];
  hyphenation: Record<string, boolean>;
  warnings: string[];
}

let status: EngineStatus | undefined;

/**
 * Process-wide setup, once: fonts, hyphenation patterns and the schema version check. Throws an
 * `INTERNAL` RenderError with the fix when fonts are missing or the engine speaks another schema
 * version.
 */
export function initEngine(): EngineStatus {
  if (status) return status;
  try {
    return startEngine();
  } catch (e) {
    throw startupError(e);
  }
}

/** A failure to start the engine: the install or the deployment is broken, not the document. */
export function startupError(e: unknown): RenderError {
  if (e instanceof RenderError) return e;
  const message = `The engine could not start: ${e instanceof Error ? e.message : String(e)}`;
  return new RenderError('INTERNAL', message, {
    engineCode: e instanceof ink.InkoxideError ? e.code : undefined,
    cause: e,
  });
}

function startEngine(): EngineStatus {
  const v = ink.version();
  if (v.schema !== SCHEMA_VERSION) {
    throw new Error(
      `Schema mismatch: the inkoxide engine speaks schema v${v.schema} but @inkoxide/schema is v${SCHEMA_VERSION}. ` +
        'Install matching versions of inkoxide and @inkoxide/schema.',
    );
  }
  const fontsDir = join(assetsDir(), 'fonts');
  const missing: string[] = [];
  for (const { file, ...face } of DEFAULT_FACES) {
    if (fonts.has(face.id)) continue;
    let bytes: Uint8Array;
    try {
      bytes = readFileSync(join(fontsDir, file));
    } catch {
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
    if (bytes) ink.registerFont(id, bytes);
  }
  const hyphenation: Record<string, boolean> = {};
  const hyphDir = join(assetsDir(), 'hyphenation');
  for (const [lang, file] of Object.entries(HYPHENATION_FILES)) {
    const path = join(hyphDir, file);
    if (existsSync(path)) {
      ink.registerHyphenationPatterns(lang, readFileSync(path));
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
  status = { version: v, fonts: fonts.ids(), hyphenation, warnings };
  return status;
}

/** Registers one more font with the engine, after `initEngine` (fonts added with `Font.register`). */
export function registerEngineFont(id: string, bytes: Uint8Array): void {
  ink.registerFont(id, bytes);
  status?.fonts.push(id);
}

export function engineStatus(): EngineStatus | undefined {
  return status;
}

export function version(): EngineVersion {
  return ink.version();
}

/**
 * An engine failure as a RenderError; the pipeline adds the component at `path`. A render option
 * the engine refuses (`INVALID_ARGUMENT` at `options.threads`, `options.creationDate`) is the
 * caller's to fix, so `ENGINE_REJECTED`; any other `INVALID_ARGUMENT` is a bug of this package.
 */
export function toRenderError(e: unknown): RenderError {
  if (e instanceof ink.InkoxideError) {
    const badOption = e.code === 'INVALID_ARGUMENT' && e.path.startsWith('options.');
    if (e.code === 'INTERNAL' || (e.code === 'INVALID_ARGUMENT' && !badOption)) {
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
export function refusedEmoji(
  doc: Pick<Document, 'emojiImages' | 'fonts'>,
  images: Record<string, Uint8Array>,
): Record<string, string> {
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
  // `validate` returns at most 100 problems: check again without those found until none is left.
  for (;;) {
    // The document's fonts draw the text of SVG images.
    const check = {
      version: SCHEMA_VERSION,
      fonts: doc.fonts ?? [],
      emojiImages: map,
      pages: [{ size: { width: 100, height: 100 } }],
    };
    const found = ink.validate(JSON.stringify(check), 'json', own).filter((e) => e.path.startsWith(at));
    for (const e of found) {
      const sequence = e.path.slice(at.length);
      refused[sequence] = e.message;
      delete map[sequence];
    }
    if (found.length === 0 || Object.keys(map).length === 0) return refused;
  }
}

/**
 * Renders a document tree to PDF bytes on the libuv pool. `output` holds the render options its
 * `<Document>` set (`conformance`, the passwords and permissions); `opts` wins over them.
 */
export async function renderPdf(
  doc: Document,
  opts: RenderOpts,
  images: Record<string, Uint8Array>,
  output: OutputOptions = {},
): Promise<Uint8Array> {
  checkOptionTypes(opts);
  // The <Document> passwords and permissions, which JavaScript can give any type.
  checkEncryptionTypes(opts.encryption ?? output.encryption);
  const options: ink.RenderOptions = { images };
  if (opts.creationDate != null) options.creationDate = opts.creationDate;
  if (opts.threads != null) options.threads = opts.threads;
  if (opts.compress != null) options.compress = opts.compress;
  if (opts.subsetFonts != null) options.subsetFonts = opts.subsetFonts;
  const pdfA = opts.pdfA ?? output.pdfA;
  if (pdfA !== undefined) options.pdfA = pdfA;
  const encryption = opts.encryption ?? output.encryption;
  if (encryption !== undefined) options.encryption = encryption;
  try {
    return await ink.render(JSON.stringify(doc), options);
  } catch (e) {
    throw toRenderError(e);
  }
}

/**
 * The page count of an engine PDF: `/Count` of the page tree root. The engine writes no object
 * streams, so the dictionary is plain text.
 */
export function countPages(pdf: Uint8Array): number {
  const s = Buffer.from(pdf.buffer, pdf.byteOffset, pdf.byteLength).toString('latin1');
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
