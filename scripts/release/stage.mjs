#!/usr/bin/env node
/**
 * Builds the TypeScript packages (tailwind, react, render) and writes publish-ready copies into
 * tmp/release/<pkg>/. The workspace stays as it is: its `exports` keep pointing at src/ and its
 * workspace: links stay. Only the staged copies point at dist/ and carry registry versions.
 * `inkoxide` and `@inkoxide/schema` are released by scripts/npm-release.mjs.
 *
 *   node scripts/release/stage.mjs
 *   node scripts/release/stage.mjs --list   # the package names, in publish order
 *
 * Plain Node, no dependencies: TypeScript's own `tsc` does the build.
 */
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OUT = join(ROOT, 'tmp', 'release');
const TSC = join(ROOT, 'node_modules', 'typescript', 'bin', 'tsc');
/** `github:eddiesr93/inkoxide`, in the object form npm would otherwise normalise it to. */
const REPOSITORY_URL = 'git+https://github.com/eddiesr93/inkoxide.git';

/**
 * In dependency order: each package only depends on the ones before it. scripts/release/publish.sh
 * publishes them in this order (`--list`).
 */
const PACKAGES = ['tailwind', 'react', 'render'];

/** Packages with no import-time side effects (module state only, no I/O or globals). */
const SIDE_EFFECT_FREE = new Set(['tailwind', 'react']);

/** Licence files that must ship with the fonts. */
const FONT_LICENCES = ['Inter-OFL.txt', 'NotoSans-OFL.txt', 'RobotoMono-OFL.txt'];

const rootPkg = readJson(join(ROOT, 'package.json'));
const NODE_ENGINE = rootPkg.engines?.node;
if (!NODE_ENGINE) fail('The root package.json has no engines.node.');

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function fail(message) {
  console.error(`stage: ${message}`);
  process.exit(1);
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

/** Relative `.ts`/`.tsx` specifiers in declaration files become `.js` (tsc keeps them as written). */
function rewriteDeclarations(dist) {
  const spec = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)(['"])(\.{1,2}\/[^'"]*?)\.(ts|tsx|mts|cts)\2/g;
  const ext = { ts: 'js', tsx: 'js', mts: 'mjs', cts: 'cjs' };
  for (const file of walk(dist).filter((f) => f.endsWith('.d.ts'))) {
    const src = readFileSync(file, 'utf8');
    const out = src.replace(spec, (_, lead, q, path, e) => `${lead}${q}${path}.${ext[e]}${q}`);
    if (out !== src) writeFileSync(file, out);
  }
}

/** tsc into packages/<pkg>/dist, then the files tsc does not emit (images, unimported JSON). */
function build(name) {
  const dir = join(ROOT, 'packages', name);
  const dist = join(dir, 'dist');
  rmSync(dist, { recursive: true, force: true });
  console.log(`stage: building @inkoxide/${name}`);
  execFileSync(process.execPath, [TSC, '-p', join(dir, 'tsconfig.build.json')], { stdio: 'inherit' });
  const src = join(dir, 'src');
  for (const file of walk(src)) {
    if (/\.(ts|tsx|md)$/.test(file)) continue;
    const target = join(dist, relative(src, file));
    if (existsSync(target)) continue;
    mkdirSync(dirname(target), { recursive: true });
    cpSync(file, target);
  }
  rewriteDeclarations(dist);
  if (!existsSync(join(dist, 'index.js')) || !existsSync(join(dist, 'index.d.ts'))) {
    fail(`@inkoxide/${name}: the build produced no dist/index.js and dist/index.d.ts.`);
  }
  return dist;
}

/** Bare package names that a package's emitted JS and declarations import. */
function importedPackages(dist) {
  const names = new Set();
  const spec = /(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)['"]([^'".][^'"]*)['"]/g;
  for (const file of walk(dist).filter((f) => /\.(js|d\.ts)$/.test(f))) {
    for (const m of readFileSync(file, 'utf8').matchAll(spec)) {
      const s = m[1];
      if (s.startsWith('node:')) continue;
      names.add(s.startsWith('@') ? s.split('/').slice(0, 2).join('/') : s.split('/')[0]);
    }
  }
  return names;
}

/** The package directory of each workspace package, by name. */
const WORKSPACE = new Map(
  readdirSync(join(ROOT, 'packages')).flatMap((dir) => {
    const manifest = join(ROOT, 'packages', dir, 'package.json');
    return existsSync(manifest) ? [[readJson(manifest).name, join(ROOT, 'packages', dir)]] : [];
  }),
);

/** The exact registry version for a dependency spec. */
function resolveVersion(dep, spec) {
  if (spec.startsWith('workspace:')) {
    const dir = WORKSPACE.get(dep);
    if (!dir) fail(`${dep} is a workspace dependency but not a package in packages/.`);
    return readJson(join(dir, 'package.json')).version;
  }
  if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(spec)) fail(`${dep}@${spec} is not an exact version.`);
  return spec;
}

function manifest(name, pkg, dist) {
  const deps = {};
  for (const [dep, spec] of Object.entries(pkg.dependencies ?? {})) {
    deps[dep] = resolveVersion(dep, spec);
  }
  // A dev dependency that the emitted code or declarations import (types such as
  // @inkoxide/schema in @inkoxide/tailwind's .d.ts) is a runtime dependency of the published copy.
  for (const used of importedPackages(dist)) {
    if (deps[used]) continue;
    const dev = pkg.devDependencies?.[used];
    if (!dev) fail(`@inkoxide/${name} imports ${used}, which is not in its package.json.`);
    deps[used] = resolveVersion(used, dev);
  }
  // Each workspace entry `./src/<file>.ts` (`.` and subpaths such as `./templates`) is
  // `./dist/<file>.js` with its declarations.
  const exports = {};
  for (const [key, target] of Object.entries(pkg.exports ?? {})) {
    if (key === './*') continue;
    const m = typeof target === 'string' ? /^\.\/src\/(.+)\.tsx?$/.exec(target) : null;
    if (!m) fail(`${pkg.name}: the export "${key}" is not a ./src/*.ts file.`);
    const file = join(dist, `${m[1]}.js`);
    if (!existsSync(file)) fail(`${pkg.name}: the export "${key}" has no ${relative(ROOT, file)}.`);
    exports[key] = { types: `./dist/${m[1]}.d.ts`, default: `./dist/${m[1]}.js` };
  }
  exports['./package.json'] = './package.json';
  if (pkg.exports?.['./*']) exports['./*'] = './dist/*';
  const out = {
    name: pkg.name,
    version: pkg.version,
    description: pkg.description,
    license: 'MIT',
    repository: { type: 'git', url: REPOSITORY_URL, directory: `packages/${name}` },
    type: 'module',
    main: './dist/index.js',
    types: './dist/index.d.ts',
    exports,
    files: name === 'render' ? ['dist', 'assets', 'media', 'GUIDE.md', 'MIGRATING.md'] : ['dist'],
    engines: { node: NODE_ENGINE },
  };
  if (SIDE_EFFECT_FREE.has(name)) out.sideEffects = false;
  // React is a peer: the reconciler and the user's components must share one copy. The range
  // matches react-reconciler's own peer range; the workspace keeps the exact pin.
  const peers = {};
  if (deps.react) {
    peers.react = `^${deps.react}`;
    delete deps.react;
  }
  out.dependencies = Object.fromEntries(Object.entries(deps).sort(([a], [b]) => a.localeCompare(b)));
  if (Object.keys(peers).length > 0) out.peerDependencies = peers;
  return out;
}

/** Fonts, their OFL licences and the hyphenation patterns, for @inkoxide/render. */
async function stageAssets(stageDir, dist) {
  const src = join(ROOT, 'packages', 'render', 'assets');
  const { DEFAULT_FACES } = await import(pathToFileURL(join(dist, 'fonts.js')).href);
  const { HYPHENATION_FILES } = await import(pathToFileURL(join(dist, 'hyphenation.js')).href);
  const fonts = DEFAULT_FACES.map((f) => f.file).filter(Boolean);
  const wanted = [
    ...[...fonts, ...FONT_LICENCES].map((f) => join('fonts', f)),
    ...Object.values(HYPHENATION_FILES).map((f) => join('hyphenation', f)),
  ];
  const missing = wanted.filter((f) => !existsSync(join(src, f)));
  if (missing.length > 0) {
    fail(
      `@inkoxide/render ships its assets, but these are missing from ${src}:\n  ${missing.join('\n  ')}\n` +
        'They come from fixtures/fonts (through the assets/fonts link) and assets/hyphenation.',
    );
  }
  for (const f of wanted) {
    mkdirSync(dirname(join(stageDir, 'assets', f)), { recursive: true });
    cpSync(join(src, f), join(stageDir, 'assets', f));
  }
  writeFileSync(join(stageDir, 'assets', 'NOTICE.md'), ASSETS_NOTICE);
}

const ASSETS_NOTICE = `# Bundled assets

These files are not covered by the package's MIT licence; each keeps its own licence.

| Files | Origin | Licence |
| --- | --- | --- |
| \`fonts/Inter-*.ttf\`, \`fonts/InterDisplay-Bold.otf\` | Inter, by The Inter Project Authors | SIL Open Font License 1.1, \`fonts/Inter-OFL.txt\` |
| \`fonts/NotoSans-Regular.ttf\` | Noto Sans, by The Noto Project Authors | SIL Open Font License 1.1, \`fonts/NotoSans-OFL.txt\` |
| \`fonts/RobotoMono-Regular.ttf\` | Roboto Mono, by The Roboto Mono Project Authors | SIL Open Font License 1.1, \`fonts/RobotoMono-OFL.txt\` |
| \`hyphenation/hyph-en-us.tex\` | hyph-utf8, US English patterns by Gerard D.C. Kuiken | Copying and distribution permitted provided the copyright notice and licence in the file header are preserved; the file is shipped unmodified |
`;

const READMES = {
  tailwind: {
    blurb:
      'Turns Tailwind class strings into Inkoxide style objects, using the real Tailwind v4 compiler. ' +
      'Unsupported classes throw outside production and are ignored with a warning in production. ' +
      'To make PDFs, install `@inkoxide/render`, which uses this package.',
    install: 'npm install @inkoxide/tailwind',
    example: `import { initTailwind, tw } from '@inkoxide/tailwind';

await initTailwind(); // once per process
const style = tw('p-4 text-lg font-bold text-slate-700');
console.log(style); // { paddingTop: 12, …, fontSize: 13.5, fontWeight: 700, color: […] }`,
  },
  react: {
    blurb:
      'React components and a custom reconciler that build Inkoxide document trees. ' +
      'Style with `className` (Tailwind) or `style`; fonts come from a registry. ' +
      'To make PDFs, install `@inkoxide/render`, which re-exports these components.',
    install: 'npm install @inkoxide/react @inkoxide/tailwind react',
    example: `import { readFileSync } from 'node:fs';
import { createElement as h } from 'react';
import { Document, Page, Text, fonts, renderToTree } from '@inkoxide/react';
import { initTailwind } from '@inkoxide/tailwind';

await initTailwind();
fonts.register({ id: 'Inter-Regular', family: 'Inter' }, readFileSync('Inter-Regular.ttf'));
const doc = await renderToTree(
  h(Document, null, h(Page, { size: 'A4', margin: '20mm' }, h(Text, { className: 'text-2xl' }, 'Hello'))),
);
console.log(JSON.stringify(doc)); // an Inkoxide document (schema v1); @inkoxide/render turns it into a PDF`,
  },
};

/**
 * npm shows a README without the repository's files, so the screenshots and the docs are
 * published in the package (`media/`, `GUIDE.md`, `MIGRATING.md`)
 * and linked through jsDelivr, which serves files of npm packages: their links become absolute
 * URLs pinned to this version.
 */
function withPublishedMedia(text, pkg) {
  const base = `https://cdn.jsdelivr.net/npm/${pkg.name}@${pkg.version}/`;
  return text.replace(/(src="|\]\()(media\/|GUIDE\.md|MIGRATING\.md)/g, `$1${base}$2`);
}

function readme(name, pkg) {
  const own = join(ROOT, 'packages', name, 'README.md');
  if (existsSync(own)) return withPublishedMedia(readFileSync(own, 'utf8'), pkg);
  const r = READMES[name];
  return `# ${pkg.name}

${r.blurb}

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js ${NODE_ENGINE}; ES modules only.

\`\`\`sh
${r.install}
\`\`\`

## Usage

\`\`\`js
${r.example}
\`\`\`

## Licence

MIT
`;
}

async function stage(name) {
  const pkgDir = join(ROOT, 'packages', name);
  const pkg = readJson(join(pkgDir, 'package.json'));
  const dist = build(name);
  const stageDir = join(OUT, name);
  rmSync(stageDir, { recursive: true, force: true });
  mkdirSync(stageDir, { recursive: true });
  cpSync(dist, join(stageDir, 'dist'), { recursive: true });
  const out = manifest(name, pkg, dist);
  if (name === 'render') await stageAssets(stageDir, dist);
  if (existsSync(join(pkgDir, 'media')))
    cpSync(join(pkgDir, 'media'), join(stageDir, 'media'), { recursive: true });
  for (const doc of ['GUIDE.md', 'MIGRATING.md']) {
    if (existsSync(join(pkgDir, doc))) cpSync(join(pkgDir, doc), join(stageDir, doc));
  }
  writeJson(join(stageDir, 'package.json'), out);
  cpSync(join(ROOT, 'LICENSE'), join(stageDir, 'LICENSE'));
  writeFileSync(join(stageDir, 'README.md'), readme(name, pkg));
  const leftover = Object.entries(out.dependencies).filter(([, v]) => /^(workspace|file|link):/.test(v));
  if (leftover.length > 0) fail(`${pkg.name} still has local dependency specs: ${JSON.stringify(leftover)}`);
  console.log(`stage: ${pkg.name}@${pkg.version} -> ${relative(ROOT, stageDir)}`);
}

if (process.argv.includes('--list')) {
  console.log(PACKAGES.join('\n'));
  process.exit(0);
}
if (!existsSync(TSC)) fail(`${TSC} not found; run pnpm install.`);
mkdirSync(OUT, { recursive: true });
for (const name of PACKAGES) await stage(name);
console.log('stage: done');
