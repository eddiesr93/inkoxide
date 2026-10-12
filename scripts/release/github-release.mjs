#!/usr/bin/env node
// Creates the git tag and the GitHub release for a CHANGELOG.md section.
// Plain Node, no dependencies; needs the GitHub CLI (`gh`), authenticated.
//
//   node scripts/release/github-release.mjs [--dry-run] [--section <name>] [--target <commit>]
//
// --section picks a section by its heading without the date ("0.2.0" or
// "@inkoxide/render 0.1.1"); the default is the newest one below "Unreleased".
// The tag is v<version> for a bare version and <package>@<version> otherwise;
// --target is the commit a new tag points at (default: HEAD); an existing tag
// keeps its commit. A release that already exists is left alone. --dry-run prints the tag and the notes only.

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { failer, REPOSITORY, ROOT } from './common.mjs';

const fail = failer('github-release');

function parseArgs(argv) {
  const args = { dryRun: false, section: undefined, target: undefined };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dry-run') args.dryRun = true;
    else if (a === '--section' || a === '--target') {
      const value = argv[++i];
      if (!value) fail(`${a} needs a value`);
      args[a.slice(2)] = value;
    } else fail(`unknown argument ${a}`);
  }
  return args;
}

// The "## <name> - <date>" sections of the changelog, newest first.
function sections(changelog) {
  const out = [];
  let current;
  for (const line of changelog.split('\n')) {
    const m = /^## (.+?)(?: - (\d{4}-\d{2}-\d{2}))?$/.exec(line);
    if (m) {
      current = { name: m[1], date: m[2], lines: [] };
      out.push(current);
    } else if (current) current.lines.push(line);
  }
  return out.filter((s) => s.name !== 'Unreleased');
}

function tagFor(name) {
  if (/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(name)) return { tag: `v${name}`, title: `v${name}`, bare: true };
  const m = /^(\S+) (\d+\.\d+\.\d+(-[\w.]+)?)$/.exec(name);
  if (!m) fail(`cannot derive a tag from the changelog heading "${name}"`);
  return { tag: `${m[1]}@${m[2]}`, title: name, bare: false };
}

// Links relative to the repository root would break on the release page.
function absolutize(text, ref) {
  return text.replace(
    /\]\((?!https?:|#|mailto:)([^)\s]+)\)/g,
    (_, path) => `](${REPOSITORY}/blob/${ref}/${path})`,
  );
}

// Release pages keep single line breaks, so the changelog's wrapped
// paragraphs and list items are joined into one line each.
function unwrap(text) {
  const out = [];
  let fence = false;
  for (const line of text.split('\n')) {
    if (line.startsWith('```')) fence = !fence;
    const prev = out.at(-1);
    const continues =
      !fence &&
      prev &&
      line.trim() &&
      !/^(\s*[-*+] |\s*\d+\. |#|\||```)/.test(line) &&
      !/^(#|```)/.test(prev);
    if (continues) out[out.length - 1] = `${prev} ${line.trim()}`;
    else out.push(line);
  }
  return out.join('\n');
}

function gh(args, opts = {}) {
  const r = spawnSync('gh', args, { encoding: 'utf8', ...opts });
  if (r.error) fail(`could not run gh: ${r.error.message}`);
  return r;
}

function git(args) {
  const r = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  if (r.error || r.status !== 0) fail(`git ${args.join(' ')} failed: ${r.error?.message ?? r.stderr}`);
  return r.stdout.trim();
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const all = sections(readFileSync(resolve(ROOT, 'CHANGELOG.md'), 'utf8'));
  const section = args.section ? all.find((s) => s.name === args.section) : all[0];
  if (!section)
    fail(args.section ? `no changelog section "${args.section}"` : 'no released changelog section');

  const { tag, title, bare } = tagFor(section.name);
  // An existing tag keeps its commit; otherwise the release tags --target,
  // which must already be on GitHub. Links point at that commit.
  const existing = spawnSync('git', ['rev-parse', '--verify', '--quiet', `${tag}^{commit}`], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  const commit =
    existing.status === 0
      ? existing.stdout.trim()
      : git(['rev-parse', '--verify', `${args.target ?? 'HEAD'}^{commit}`]);
  const notes = unwrap(absolutize(section.lines.join('\n').trim(), commit));
  if (!notes) fail(`the changelog section "${section.name}" is empty`);

  if (args.dryRun) {
    console.log(`tag: ${tag}\ncommit: ${commit}\ntitle: ${title}\n\n${notes}`);
    return;
  }
  if (gh(['release', 'view', tag, '--json', 'tagName']).status === 0) {
    console.log(`skip ${tag}: the release exists`);
    return;
  }
  const create = ['release', 'create', tag, '--title', title, '--notes-file', '-', '--target', commit];
  // Package releases never take "latest" from the repository release.
  if (!bare) create.push('--latest=false');
  const r = gh(create, { input: notes, stdio: ['pipe', 'inherit', 'inherit'] });
  if (r.status !== 0) fail(`gh release create ${tag} failed (exit ${r.status})`);
}

main();
