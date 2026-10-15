/**
 * The engine host in Node: the `inkoxide` binding, and the bundled assets read from disk. The
 * only module of this package that imports `inkoxide`. Importing it installs it.
 */
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as ink from 'inkoxide';

import { type EngineHost, setEngineHost } from './host.ts';
import { localPolicy, readLocalImage } from './local.ts';

/**
 * Where the bundled fonts and hyphenation patterns are read from: `INKOXIDE_ASSETS_DIR` when set,
 * otherwise the package's own `assets/`, next to `dist/` (or `src/` in the repository).
 */
export function assetsDir(): string {
  if (process.env.INKOXIDE_ASSETS_DIR) return resolve(process.env.INKOXIDE_ASSETS_DIR);
  return resolve(dirname(fileURLToPath(import.meta.url)), '..', 'assets');
}

export const nodeHost: EngineHost = {
  start: async () => {},
  version: () => ink.version(),
  registerFont: async (id, bytes) => ink.registerFont(id, bytes),
  registerHyphenationPatterns: async (lang, bytes) => ink.registerHyphenationPatterns(lang, bytes),
  validate: async (doc, images) => ink.validate(doc, 'json', images),
  render: (doc, options) => ink.render(doc, options),
  isEngineError: (e): e is ink.InkoxideError => e instanceof ink.InkoxideError,
  readAsset: async (path) => {
    try {
      return await readFile(join(assetsDir(), path));
    } catch {
      return undefined;
    }
  },
  assetLocation: (path) => join(assetsDir(), path),
  files: { localPolicy, readLocalImage, readFile: (path) => readFile(path) },
};

setEngineHost(nodeHost);
