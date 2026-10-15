/**
 * What renders need from where they run: the engine and the bundled fonts and hyphenation
 * patterns. Each entry of the package installs its host before anything renders: the Node one
 * (`node-host.ts`, the `inkoxide` binding and `node:fs`) for the main entry and `/templates`. The
 * rest of the package imports neither the engine nor Node modules; it goes through `engineHost()`.
 */
import type { RenderOptions } from 'inkoxide';

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
  render(doc: string, options: RenderOptions): Promise<Uint8Array>;
  /** Whether `e` is an error of the engine, with its code and path. */
  isEngineError(e: unknown): e is Error & EngineIssue;
  /** A bundled asset by its path in `assets/` (`fonts/Inter-Regular.ttf`); undefined when missing. */
  readAsset(path: string): Promise<Uint8Array | undefined>;
  /** Where a bundled asset is read from, for messages. */
  assetLocation(path: string): string;
}

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
