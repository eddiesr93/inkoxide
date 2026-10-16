/**
 * What renders need from where they run: the engine and the bundled fonts and hyphenation
 * patterns. Each entry of the package installs its host before anything renders: the Node one
 * (`node-host.ts`, the `inkoxide` binding and `node:fs`) for the main entry and `/templates`. The
 * rest of the package imports neither the engine nor Node modules; it goes through `engineHost()`.
 */
import type { RenderOptions } from 'inkoxide';

import type { LocalImageOptions } from './types.ts';

export interface EngineVersion {
  engine: string;
  schema: number;
  gitSha: string;
}

/** A problem the engine reports: what `inkoxide` throws or `validate` returns. */
export interface EngineIssue {
  code: string;
  path: string;
  message: string;
}

export interface EngineHost {
  /** Gets the engine ready; `initEngine` calls it once, first. */
  start(): Promise<void>;
  /** The engine's version; after `start`. */
  version(): EngineVersion;
  registerFont(id: string, bytes: Uint8Array): Promise<void>;
  registerHyphenationPatterns(lang: string, bytes: Uint8Array): Promise<void>;
  /** The problems of a JSON document, its image references checked against `images`. */
  validate(doc: string, images: Record<string, Uint8Array>): Promise<EngineIssue[]>;
  /** The PDF of a JSON document. A host that can stop a render does when `signal` aborts. */
  render(doc: string, options: RenderOptions, signal?: AbortSignal): Promise<Uint8Array>;
  /** Whether `e` is an error of the engine, with its code and path. */
  isEngineError(e: unknown): e is Error & EngineIssue;
  /** A bundled asset by its path in `assets/` (`fonts/Inter-Regular.ttf`); undefined when missing. */
  readAsset(path: string): Promise<Uint8Array | undefined>;
  /** Where a bundled asset is read from, for messages. */
  assetLocation(path: string): string;
  /** Files, where there is a file system: undefined in a browser. */
  files?: FileAccess;
  /**
   * Whether the bundled fonts are read when a document uses them, rather than all at start-up
   * (in a browser, where each is a download).
   */
  lazyFonts?: boolean;
}

/** Files: image files (`localImages`) and fonts registered from a path. */
export interface FileAccess {
  /** The local image policy of the options, else of the environment. */
  localPolicy(opts: LocalImageOptions | undefined): LocalImageOptions | undefined;
  /** An image file inside the policy's folders (`local.ts`). */
  readLocalImage(src: string, opts: LocalImageOptions): Promise<Uint8Array>;
  /** A file by path or `file:` URL. */
  readFile(path: string | URL): Promise<Uint8Array>;
}

/** Why a file cannot be read where there is no file system. */
export const NO_FILES = 'files cannot be read here (no file system): pass the bytes or a URL.';

let host: EngineHost | undefined;

/** Installs the host of the entry being loaded. */
export function setEngineHost(h: EngineHost): void {
  host = h;
}

export function engineHost(): EngineHost {
  if (!host)
    throw new Error('@inkoxide/render: no engine host installed; import the package through an entry.');
  return host;
}
