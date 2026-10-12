// What the release scripts share: the repository, the registry and small file helpers. Plain
// Node, no dependencies.
//
// The repository is read from Cargo.toml's `[workspace.package] repository`, so a rename
// changes one line.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

export const REGISTRY = 'https://registry.npmjs.org';

/** `https://github.com/<owner>/<name>`. */
export const REPOSITORY = (() => {
  const m = /^repository = "(https:\/\/github\.com\/[^/"]+\/[^/"]+)"$/m.exec(
    readFileSync(join(ROOT, 'Cargo.toml'), 'utf8'),
  );
  if (!m) throw new Error('Cargo.toml has no `repository = "https://github.com/<owner>/<name>"`');
  return m[1];
})();

/** `<owner>/<name>`, as `gh --repo` takes it. */
export const REPOSITORY_SLUG = REPOSITORY.slice('https://github.com/'.length);

/** The repository in the form npm normalises a package.json `repository.url` to. */
export const REPOSITORY_GIT_URL = `git+${REPOSITORY}.git`;

export function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

/** A `fail(message)` that prints `<tool>: message` and exits 1. */
export function failer(tool) {
  return (message) => {
    console.error(`${tool}: ${message}`);
    process.exit(1);
  };
}

// `node scripts/release/common.mjs <name>` prints one of the values above, for shell scripts.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const values = { registry: REGISTRY, repository: REPOSITORY, 'repository-slug': REPOSITORY_SLUG };
  const value = values[process.argv[2]];
  if (process.argv.length !== 3 || value === undefined) {
    console.error(`usage: common.mjs ${Object.keys(values).join('|')}`);
    process.exit(2);
  }
  console.log(value);
}
