/**
 * What the package reads from where it runs, without importing Node: guarded, so that the code
 * shared with the browser entry loads in a browser too.
 */

interface NodeProcess {
  env?: Record<string, string | undefined>;
  getBuiltinModule?(id: string): unknown;
}

/** Node's `process`, where there is one. */
export const nodeProcess = (): NodeProcess | undefined => (globalThis as { process?: NodeProcess }).process;

/** The address of the page (or worker) the code runs in; undefined in Node. */
export const pageUrl = (): string | undefined =>
  (globalThis as { location?: { href: string } }).location?.href;

/** `url` resolved against the page, as a link on it would be. */
export const onPage = (url: string | URL): URL => new URL(url, pageUrl());
