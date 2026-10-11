#!/usr/bin/env node
// Stages (and optionally publishes) the npm packages from a directory of
// prebuilt native bindings. Plain Node, no dependencies.
//
//   node scripts/npm-release.mjs <bin-dir> <staging-dir>
//   node scripts/npm-release.mjs <bin-dir> <staging-dir> --publish
//   node scripts/npm-release.mjs <bin-dir> <staging-dir> --publish --dry-run
//
// <bin-dir> must hold inkoxide.<triple>.node for every supported triple.
// <staging-dir> receives one directory per package:
//   @inkoxide/<triple>  one per platform, the binary only
//   inkoxide            the loader and types; optionalDependencies pin
//                       every platform package to the same version
//   @inkoxide/schema    a copy of packages/schema/
// --publish publishes platform packages first, then inkoxide, then
// @inkoxide/schema, skipping any name@version already on the registry.

import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCOPE = '@inkoxide';
const REGISTRY = 'https://registry.npmjs.org';

// Triple (as packages/inkoxide/index.js platformTriple() names it) -> npm platform fields.
const PLATFORMS = {
  'linux-x64-gnu': { os: 'linux', cpu: 'x64', libc: 'glibc', label: 'Linux x64 (glibc)' },
  'linux-x64-musl': { os: 'linux', cpu: 'x64', libc: 'musl', label: 'Linux x64 (musl)' },
  'linux-arm64-gnu': { os: 'linux', cpu: 'arm64', libc: 'glibc', label: 'Linux arm64 (glibc)' },
  'darwin-arm64': { os: 'darwin', cpu: 'arm64', label: 'macOS arm64' },
  'darwin-x64': { os: 'darwin', cpu: 'x64', label: 'macOS x64' },
  'win32-x64-msvc': { os: 'win32', cpu: 'x64', label: 'Windows x64' },
};

function fail(msg) {
  console.error(`npm-release: ${msg}`);
  process.exit(1);
}

function parseArgs(argv) {
  const flags = new Set(argv.filter((a) => a.startsWith('--')));
  const positional = argv.filter((a) => !a.startsWith('--'));
  for (const f of flags) {
    if (f !== '--publish' && f !== '--dry-run') fail(`unknown flag ${f}`);
  }
  if (positional.length !== 2) {
    fail('usage: npm-release.mjs <bin-dir> <staging-dir> [--publish [--dry-run]]');
  }
  if (flags.has('--dry-run') && !flags.has('--publish')) fail('--dry-run only applies with --publish');
  return {
    binDir: resolve(positional[0]),
    stageDir: resolve(positional[1]),
    publish: flags.has('--publish'),
    dryRun: flags.has('--dry-run'),
  };
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function stagePlatform(stageDir, binDir, triple, version, repository) {
  const p = PLATFORMS[triple];
  const name = `${SCOPE}/${triple}`;
  const binary = `inkoxide.${triple}.node`;
  const dir = join(stageDir, SCOPE, triple);
  mkdirSync(dir, { recursive: true });
  copyFileSync(join(binDir, binary), join(dir, binary));
  copyFileSync(join(ROOT, 'LICENSE'), join(dir, 'LICENSE'));
  const pkg = {
    name,
    version,
    description: `Native binary of the inkoxide PDF engine for ${p.label}.`,
    license: 'MIT',
    repository,
    main: binary,
    files: [binary],
    os: [p.os],
    cpu: [p.cpu],
    ...(p.libc ? { libc: [p.libc] } : {}),
    engines: { node: '>= 18' },
  };
  writeJson(join(dir, 'package.json'), pkg);
  writeFileSync(
    join(dir, 'README.md'),
    `# ${name}\n\nThe prebuilt native binding of [inkoxide](https://www.npmjs.com/package/inkoxide) ` +
      `for ${p.label}. Do not install it directly: \`inkoxide\` lists it as an optional ` +
      `dependency, and npm installs only the package that matches the current platform.\n`,
  );
  return { name, version, dir };
}

function stageMain(stageDir, version, mainPkg) {
  const dir = join(stageDir, 'inkoxide');
  mkdirSync(dir, { recursive: true });
  for (const f of ['index.js', 'index.d.ts', 'README.md']) {
    copyFileSync(join(ROOT, 'packages', 'inkoxide', f), join(dir, f));
  }
  copyFileSync(join(ROOT, 'LICENSE'), join(dir, 'LICENSE'));
  const optionalDependencies = {};
  for (const triple of Object.keys(PLATFORMS)) optionalDependencies[`${SCOPE}/${triple}`] = version;
  const pkg = {
    name: mainPkg.name,
    version,
    description: mainPkg.description,
    license: mainPkg.license,
    repository: mainPkg.repository,
    main: mainPkg.main,
    types: mainPkg.types,
    // No bundled binary: the platform package supplies it.
    files: ['index.js', 'index.d.ts'],
    engines: mainPkg.engines,
    optionalDependencies,
  };
  writeJson(join(dir, 'package.json'), pkg);
  return { name: pkg.name, version, dir };
}

function stageSchema(stageDir) {
  const src = join(ROOT, 'packages', 'schema');
  const pkg = readJson(join(src, 'package.json'));
  const dir = join(stageDir, SCOPE, 'schema');
  mkdirSync(dir, { recursive: true });
  for (const f of ['package.json', 'README.md', ...pkg.files]) copyFileSync(join(src, f), join(dir, f));
  copyFileSync(join(ROOT, 'LICENSE'), join(dir, 'LICENSE'));
  return { name: pkg.name, version: pkg.version, dir };
}

function npm(args, opts = {}) {
  // shell: true lets Windows find npm.cmd; every argument here is ours.
  return spawnSync('npm', args, { encoding: 'utf8', shell: process.platform === 'win32', ...opts });
}

// true when name@version is on the registry, false on a 404 or an unknown
// version; anything else (network, auth) aborts rather than guessing.
function isPublished(name, version) {
  const r = npm(['view', `${name}@${version}`, 'version', '--json', `--registry=${REGISTRY}`]);
  if (r.error) fail(`could not run npm view: ${r.error.message}`);
  const out = (r.stdout || '').trim();
  let parsed;
  try {
    parsed = out ? JSON.parse(out) : undefined;
  } catch {
    parsed = undefined;
  }
  if (r.status === 0) {
    // An existing package without that version prints nothing.
    return parsed === version || (Array.isArray(parsed) && parsed.includes(version));
  }
  const code = parsed?.error?.code;
  if (code === 'E404' || /\bE404\b/.test(r.stderr || '')) return false;
  fail(`npm view ${name}@${version} failed (exit ${r.status}):\n${r.stderr || out}`);
}

function publish(pkg, dryRun) {
  const id = `${pkg.name}@${pkg.version}`;
  if (isPublished(pkg.name, pkg.version)) {
    console.log(`skip ${id}: already on the registry`);
    return;
  }
  const args = ['publish', '--access', 'public', `--registry=${REGISTRY}`];
  if (dryRun) args.push('--dry-run');
  console.log(`${dryRun ? 'dry-run publish' : 'publish'} ${id}`);
  const r = npm(args, { cwd: pkg.dir, stdio: 'inherit' });
  if (r.error) fail(`could not run npm publish: ${r.error.message}`);
  if (r.status !== 0) fail(`npm publish ${id} failed (exit ${r.status}); rerun to resume`);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const mainPkg = readJson(join(ROOT, 'packages', 'inkoxide', 'package.json'));
  const { version, repository } = mainPkg;

  const missing = Object.keys(PLATFORMS)
    .map((t) => `inkoxide.${t}.node`)
    .filter((f) => !existsSync(join(args.binDir, f)));
  if (missing.length) fail(`refusing to stage, missing in ${args.binDir}: ${missing.join(', ')}`);

  // The staging dir is wiped; never let that be the repo or the binaries.
  const inside = (a, b) => a === b || a.startsWith(b + sep);
  if (inside(ROOT, args.stageDir) || inside(args.binDir, args.stageDir)) {
    fail(`staging dir ${args.stageDir} contains the repository or <bin-dir>`);
  }
  rmSync(args.stageDir, { recursive: true, force: true });
  mkdirSync(args.stageDir, { recursive: true });
  const platforms = Object.keys(PLATFORMS).map((t) =>
    stagePlatform(args.stageDir, args.binDir, t, version, repository),
  );
  const loader = stageMain(args.stageDir, version, mainPkg);
  const schema = stageSchema(args.stageDir);
  const all = [...platforms, loader, schema];
  for (const p of all) console.log(`staged ${p.name}@${p.version} in ${p.dir}`);

  if (!args.publish) return;
  // Platform packages first, so inkoxide never points at missing versions.
  for (const p of all) publish(p, args.dryRun);
  console.log(args.dryRun ? 'dry run complete' : 'published');
}

main();
