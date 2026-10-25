/**
 * The files in examples/ and the benchmark invoices write their PDF when run with tsx, and only
 * export their component when imported. Each is started here through a symlinked directory, where
 * process.argv[1] keeps the link but import.meta.filename is the real path.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const ROOT = join(import.meta.dirname, '..', '..', '..');
const TSX = join(ROOT, 'node_modules', '.bin', 'tsx');

/** Each runnable file, with the PDF it writes. */
const RUNNABLE: [string, string, string][] = (
  [
    ['examples', /\.tsx$/],
    ['benchmarks', /^invoice-.*\.tsx$/],
  ] as const
).flatMap(([dir, pattern]) =>
  readdirSync(join(ROOT, dir))
    .filter((f) => pattern.test(f))
    .map((f): [string, string, string] => [dir, f, f.replace(/\.tsx$/, '.pdf')]),
);

let cwd: string;

beforeAll(() => {
  cwd = mkdtempSync(join(tmpdir(), 'inkoxide-examples-'));
  for (const dir of ['examples', 'benchmarks']) symlinkSync(join(ROOT, dir), join(cwd, dir), 'dir');
});

afterAll(() => {
  rmSync(cwd, { recursive: true, force: true });
});

describe('run through a symlinked directory', () => {
  it.each(RUNNABLE)('%s/%s writes %s', (dir, file, pdf) => {
    execFileSync(TSX, ['--tsconfig', join(ROOT, dir, 'tsconfig.json'), join(dir, file)], {
      cwd,
      stdio: 'pipe',
    });
    expect(existsSync(join(cwd, pdf))).toBe(true);
  });
});
