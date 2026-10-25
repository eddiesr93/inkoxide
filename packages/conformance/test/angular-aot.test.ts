/**
 * `@inkoxide/angular` in an application compiled ahead of time, as the Angular CLI builds it:
 * Angular's compiler (`ngc`, on TypeScript 6) compiles a template with signal inputs, which renders
 * the PDF React renders with no JIT compiler loaded; the browser directives are read from the
 * package's declaration files, as those of any library, and their partial declarations link with
 * Angular's Babel linker. The other Angular tests compile just in time.
 */
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { type CompilerOptions, performCompilation } from '@angular/compiler-cli';
import linker from '@angular/compiler-cli/linker/babel';
import { renderToBuffer as angularBuffer } from '@inkoxide/angular';
import { renderToBuffer as reactBuffer } from '@inkoxide/react';
import { createElement } from 'react';
import ts from 'typescript';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import ReactReceipt, { sample } from '../../../examples/receipt.tsx';
import { sha } from './sha.ts';

const PACKAGES = fileURLToPath(new URL('../../', import.meta.url));
const CONFORMANCE = fileURLToPath(new URL('../', import.meta.url));
// Inside the package, so that the compiled modules find the packages they import.
const out = mkdtempSync(join(CONFORMANCE, '.angular-aot-'));
const dts = join(out, 'dts');
const OPTIONS = { creationDate: '2026-10-25T00:00:00Z' };

type Transform = (code: string, options: object) => Promise<{ code?: string | null } | null>;

/** The Babel of Angular's linker: 7 for Angular 21, 8 for 22. */
async function babelTransform(): Promise<Transform> {
  const cli = createRequire(import.meta.url).resolve('@angular/compiler-cli/package.json');
  const babel = (await import(pathToFileURL(createRequire(cli).resolve('@babel/core')).href)) as {
    transformAsync?: Transform;
    default?: { transformAsync: Transform };
  };
  return (babel.transformAsync ?? babel.default?.transformAsync) as Transform;
}

/** The options of the repository's tsconfig.json, for TypeScript 6. */
const base: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2022,
  lib: ['lib.es2023.d.ts', 'lib.dom.d.ts', 'lib.dom.iterable.d.ts'],
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  strict: true,
  skipLibCheck: true,
  allowImportingTsExtensions: true,
  resolveJsonModule: true,
  types: ['node'],
};

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );

const errors = (diagnostics: readonly ts.Diagnostic[]) =>
  diagnostics
    .filter((d) => d.category === ts.DiagnosticCategory.Error)
    .map((d) => ts.flattenDiagnosticMessageText(d.messageText, '\n'));

/**
 * The declaration files of `@inkoxide/angular/browser` and of the packages it imports, as the
 * release build writes them (scripts/release/stage.mjs): `.ts` specifiers become `.js`.
 */
function declarations(): void {
  const program = ts.createProgram([join(PACKAGES, 'angular/src/browser.ts')], {
    ...base,
    declaration: true,
    emitDeclarationOnly: true,
    rootDir: PACKAGES,
    outDir: dts,
  });
  expect(errors(ts.getPreEmitDiagnostics(program))).toEqual([]);
  program.emit();
  const spec = /(\bfrom\s*|\bimport\s*\(\s*)(['"])(\.{1,2}\/[^'"]*?)\.tsx?\2/g;
  for (const file of walk(dts)) {
    writeFileSync(file, readFileSync(file, 'utf8').replace(spec, '$1$2$3.js$2'));
  }
}

/** The fixtures of angular/aot, compiled by `ngc` against those declarations. */
function compile(): void {
  const options: CompilerOptions = {
    ...base,
    rootDir: CONFORMANCE,
    outDir: join(out, 'js'),
    rewriteRelativeImportExtensions: true,
    // Each workspace package's entries, as their declaration files.
    paths: {
      '@inkoxide/*/browser': [join(dts, '*/src/browser.d.ts')],
      '@inkoxide/*/props': [join(dts, '*/src/props.d.ts')],
      '@inkoxide/*/common': [join(dts, '*/src/common.d.ts')],
      '@inkoxide/*/internal': [join(dts, '*/src/internal.d.ts')],
      '@inkoxide/*': [join(dts, '*/src/index.d.ts')],
    },
    strictTemplates: true,
    compilationMode: 'full',
  };
  const { diagnostics } = performCompilation({
    rootNames: ['receipt.ts', 'preview.ts'].map((f) => join(CONFORMANCE, 'angular/aot', f)),
    options,
  });
  expect(errors(diagnostics as readonly ts.Diagnostic[])).toEqual([]);
}

beforeAll(() => {
  declarations();
  compile();
}, 120_000);

afterAll(() => {
  if (!process.env.KEEP) rmSync(out, { recursive: true, force: true });
});

describe('ahead of time', () => {
  it('compiles a template with signal inputs, which renders the PDF React renders', async () => {
    const { Receipt } = (await import(pathToFileURL(join(out, 'js/angular/aot/receipt.js')).href)) as {
      Receipt: { ɵcmp?: unknown };
    };
    expect(Receipt.ɵcmp).toBeDefined();
    const angular = await angularBuffer({ component: Receipt as never, inputs: sample }, OPTIONS);
    expect(sha(angular)).toBe(sha(await reactBuffer(createElement(ReactReceipt, sample), OPTIONS)));
  });

  it("reads the browser directives from the package's declaration files", () => {
    const preview = readFileSync(join(out, 'js/angular/aot/preview.js'), 'utf8');
    expect(preview).toMatch(/dependencies: \[InkPdfViewer, InkPdfDownload, InkBlobProvider\]/);
    const declared = readFileSync(join(dts, 'angular/src/directives.d.ts'), 'utf8');
    expect(declared).toMatch(/ɵɵDirectiveDeclaration<\s*InkPdfViewer,\s*['"]iframe\[inkPdfViewer\]['"]/);
  });

  it("links the directives' partial declarations", async () => {
    const file = join(PACKAGES, 'angular/src/directives.ts');
    const { outputText } = ts.transpileModule(readFileSync(file, 'utf8'), {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
    });
    const linked = await (
      await babelTransform()
    )(outputText, {
      // A JavaScript file: the linker writes types into a TypeScript one.
      filename: file.replace(/\.ts$/, '.js'),
      plugins: [linker],
      configFile: false,
      babelrc: false,
    });
    const code = linked?.code ?? '';
    expect(code).not.toMatch(/ɵɵngDeclare\w*\(/);
    expect(code.match(/ɵɵdefineDirective\(/g)).toHaveLength(3);
    // Next to the sources, so that its imports find them.
    const at = join(out, 'linked', 'directives.js');
    mkdirSync(dirname(at), { recursive: true });
    const from = relative(dirname(at), dirname(file));
    writeFileSync(at, code.replace(/(from\s*['"])\.\/([\w-]+)\.ts(['"])/g, `$1${from}/$2.ts$3`));
    const { InkPdfViewer, InkPdfDownload, InkBlobProvider } = (await import(
      pathToFileURL(at).href
    )) as Record<
      string,
      { ɵdir: { selectors: unknown; inputs: Record<string, unknown>; exportAs: string[] | null } }
    >;
    expect(InkPdfViewer?.ɵdir.selectors).toEqual([['iframe', 'inkPdfViewer', '']]);
    expect(Object.keys(InkPdfViewer?.ɵdir.inputs ?? {}).sort()).toEqual([
      'inkPdfViewer',
      'inkPdfViewerOptions',
      'inkPdfViewerToolbar',
    ]);
    expect(InkPdfDownload?.ɵdir.exportAs).toEqual(['inkPdfDownload']);
    expect(InkBlobProvider?.ɵdir.selectors).toEqual([['', 'inkBlobProvider', '']]);
  });
});
