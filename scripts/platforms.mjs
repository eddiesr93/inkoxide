#!/usr/bin/env node
// The native platforms of the npm packages: one per Rust target in packages/inkoxide/package.json
// (`napi.targets`, the list to edit), each named by the triple packages/inkoxide/index.js looks
// for (`linux-x64-gnu`, `darwin-arm64`, `win32-x64-msvc`). Plain Node, no dependencies.
//
//   node scripts/platforms.mjs          # every platform: <rust target> <triple>, one per line
//   node scripts/platforms.mjs --host   # the triple of this machine
//
// Not derived from here: index.js names the platform it runs on itself (the published loader
// cannot import scripts), and .github/workflows/release.yml lists the targets each build job
// compiles, with its toolchains. scripts/npm-release.mjs refuses to stage a release that misses
// any of these platforms.

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const CPUS = { x86_64: 'x64', aarch64: 'arm64' };
const OSES = { linux: 'linux', darwin: 'darwin', windows: 'win32' };
const SYSTEMS = { linux: 'Linux', darwin: 'macOS', win32: 'Windows' };

/** The npm platform fields of a Rust target, and its triple. */
function platformOf(target) {
  const [arch, , system, abi] = target.split('-');
  const cpu = CPUS[arch];
  const os = OSES[system];
  if (!cpu || !os) throw new Error(`platforms: no npm platform for the Rust target ${target}`);
  if (os === 'linux') {
    const libc = abi === 'musl' ? 'musl' : 'glibc';
    const triple = `linux-${cpu}-${abi === 'musl' ? 'musl' : 'gnu'}`;
    return { target, triple, os, cpu, libc, label: `${SYSTEMS[os]} ${cpu} (${libc})` };
  }
  const triple = os === 'win32' ? `win32-${cpu}-msvc` : `${os}-${cpu}`;
  return { target, triple, os, cpu, label: `${SYSTEMS[os]} ${cpu}` };
}

/** Every platform, in the order of `napi.targets`. */
export function platforms() {
  const pkg = JSON.parse(readFileSync(join(ROOT, 'packages', 'inkoxide', 'package.json'), 'utf8'));
  return pkg.napi.targets.map(platformOf);
}

/** The triple of this machine, as packages/inkoxide/index.js names it. */
export function hostTriple() {
  const { platform, arch } = process;
  if (platform === 'linux') {
    const report = process.report.getReport();
    return `linux-${arch}-${report.header.glibcVersionRuntime ? 'gnu' : 'musl'}`;
  }
  if (platform === 'win32') return `win32-${arch}-msvc`;
  return `${platform}-${arch}`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--host') console.log(hostTriple());
  else if (args.length === 0) for (const p of platforms()) console.log(`${p.target} ${p.triple}`);
  else {
    console.error('usage: platforms.mjs [--host]');
    process.exit(2);
  }
}
