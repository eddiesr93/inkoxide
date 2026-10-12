/**
 * The engine boundary: the only module of this package that imports `inkoxide`. Everything else
 * talks to the engine through these functions.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { fonts } from '@inkoxide/react';
import type { Document } from '@inkoxide/schema';
import { SCHEMA_VERSION } from '@inkoxide/schema';
import * as ink from 'inkoxide';

import { DEFAULT_FACES } from './fonts.ts';
import { HYPHENATION_FILES } from './hyphenation.ts';
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
  const { missing } = fonts.loadDirectory(fontsDir, DEFAULT_FACES);
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

/** An engine failure as a RenderError; the pipeline adds the component at `path`. */
export function toRenderError(e: unknown): RenderError {
  if (e instanceof ink.InkoxideError) {
    if (e.code === 'INTERNAL' || e.code === 'INVALID_ARGUMENT') {
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

/** Renders a document tree to PDF bytes on the libuv pool. */
export async function renderPdf(
  doc: Document,
  opts: RenderOpts,
  images: Record<string, Uint8Array>,
): Promise<Uint8Array> {
  const options: ink.RenderOptions = { images };
  if (opts.creationDate !== undefined) options.creationDate = opts.creationDate;
  if (opts.threads !== undefined) options.threads = opts.threads;
  if (opts.compress !== undefined) options.compress = opts.compress;
  if (opts.subsetFonts !== undefined) options.subsetFonts = opts.subsetFonts;
  if (opts.pdfA !== undefined) options.pdfA = opts.pdfA;
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
