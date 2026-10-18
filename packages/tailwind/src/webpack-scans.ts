/**
 * The scans of the Tailwind build plugin for webpack and Rspack (`plugin.ts`), by key, for its
 * loader (`webpack-loader.ts`). Bundlers load the loader themselves, which may be another copy of
 * this module: the scans are kept on the global object.
 */

/** What the loader asks of a scan. */
export interface TableScan {
  table(): Promise<{ code: string; files: string[] }>;
}

const KEY = Symbol.for('@inkoxide/tailwind webpack scans');

export function webpackScans(): Map<string, TableScan> {
  const g = globalThis as { [KEY]?: Map<string, TableScan> };
  return (g[KEY] ??= new Map());
}
