/** A webpack loader for the tests: TypeScript to JavaScript with esbuild. */
import { transform } from 'esbuild';

interface LoaderContext {
  async(): (error: Error | null, code?: string) => void;
}

export default function esbuildLoader(this: LoaderContext, source: string): void {
  const done = this.async();
  transform(source, { loader: 'ts', format: 'esm', target: 'es2023' }).then(
    (r) => done(null, r.code),
    (e: Error) => done(e),
  );
}
