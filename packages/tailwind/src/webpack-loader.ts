/**
 * The loader of the Tailwind build plugin for webpack and Rspack (`plugin.ts`): it puts the table
 * of the classes the sources use in the table stub (`table.ts`), from the scan of its plugin. Not
 * cached: each build scans again, and the stub has the new table.
 */
import { webpackScans } from './webpack-scans.ts';

/** What the loader uses of webpack's and Rspack's loader context. */
interface LoaderContext {
  getOptions(): { scan: string };
  cacheable(flag: boolean): void;
  async(): (error: Error | null, code?: string) => void;
  addDependency(file: string): void;
}

export default function tableLoader(this: LoaderContext): void {
  this.cacheable(false);
  const done = this.async();
  const sources = webpackScans().get(this.getOptions().scan);
  if (sources === undefined) {
    done(new Error('@inkoxide/tailwind: the table stub was loaded without its build plugin.'));
    return;
  }
  sources.table().then(
    ({ code, files }) => {
      for (const f of files) this.addDependency(f);
      done(null, code);
    },
    (error: unknown) => done(error instanceof Error ? error : new Error(String(error))),
  );
}
