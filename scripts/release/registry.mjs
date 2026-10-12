#!/usr/bin/env node
// Whether a version is on the npm registry, for the release scripts: scripts/npm-release.mjs
// imports it, scripts/release/publish.sh runs it. Plain Node, no dependencies.
//
//   node scripts/release/registry.mjs <name>@<version>   # prints yes or no
//
// Any other npm failure (network, auth) is an error rather than a guess.

import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const REGISTRY = 'https://registry.npmjs.org';

export function npm(args, opts = {}) {
  // shell: true lets Windows find npm.cmd; every argument here is ours.
  return spawnSync('npm', args, { encoding: 'utf8', shell: process.platform === 'win32', ...opts });
}

// true when name@version is on the registry, false on a 404 or an unknown
// version; anything else (network, auth) throws rather than guessing.
export function isPublished(name, version) {
  const r = npm(['view', `${name}@${version}`, 'version', '--json', `--registry=${REGISTRY}`]);
  if (r.error) throw new Error(`could not run npm view: ${r.error.message}`);
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
  throw new Error(`npm view ${name}@${version} failed (exit ${r.status}):\n${r.stderr || out}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const spec = process.argv[2] ?? '';
  // The version follows the last @: a scoped name starts with one.
  const at = spec.lastIndexOf('@');
  if (process.argv.length !== 3 || at <= 0) {
    console.error('usage: registry.mjs <name>@<version>');
    process.exit(2);
  }
  try {
    console.log(isPublished(spec.slice(0, at), spec.slice(at + 1)) ? 'yes' : 'no');
  } catch (e) {
    console.error(`registry: ${e.message}`);
    process.exit(1);
  }
}
