/**
 * The Tailwind plugin of `@inkoxide/tailwind` in an Angular CLI build, wired as the Angular guide
 * says (`@angular-builders/custom-esbuild`): the application builder gives the browser bundle the
 * table of the classes in a component's template file, without Tailwind's compiler, which a build
 * without the plugin keeps. The builder runs through Angular's Architect, as `ng build` runs it.
 */
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterAll, describe, expect, it } from 'vitest';

const require = createRequire(import.meta.url);
const { Architect } = require('@angular-devkit/architect') as typeof import('@angular-devkit/architect');
const { WorkspaceNodeModulesArchitectHost } =
  require('@angular-devkit/architect/node') as typeof import('@angular-devkit/architect/node');
const { logging, schema, workspaces } =
  require('@angular-devkit/core') as typeof import('@angular-devkit/core');
const { NodeJsSyncHost } = require('@angular-devkit/core/node') as typeof import('@angular-devkit/core/node');

const APP = fileURLToPath(new URL('../angular/cli/', import.meta.url));
const out = mkdtempSync(fileURLToPath(new URL('../.angular-cli-', import.meta.url)));

afterAll(() => {
  if (!process.env.KEEP) rmSync(out, { recursive: true, force: true });
});

/** The browser bundle of the application, built with `configuration` of its angular.json. */
async function build(configuration?: string): Promise<string> {
  const host = workspaces.createWorkspaceHost(new NodeJsSyncHost());
  const { workspace } = await workspaces.readWorkspace(APP, host);
  const registry = new schema.CoreSchemaRegistry();
  registry.addPostTransform(schema.transforms.addUndefinedDefaults);
  const architect = new Architect(new WorkspaceNodeModulesArchitectHost(workspace, APP), registry);
  const errors: string[] = [];
  const logger = new logging.Logger('build');
  logger.subscribe((entry) => {
    if (entry.level === 'error' || entry.level === 'fatal') errors.push(entry.message);
  });
  const outputPath = join(out, configuration ?? 'plugin');
  const run = await architect.scheduleTarget(
    { project: 'card', target: 'build', ...(configuration ? { configuration } : {}) },
    { outputPath },
    { logger },
  );
  const { success } = await run.result;
  await run.stop();
  expect(errors).toEqual([]);
  expect(success).toBe(true);
  const browser = join(outputPath, 'browser');
  return readdirSync(browser)
    .filter((file) => file.endsWith('.js'))
    .map((file) => readFileSync(join(browser, file), 'utf8'))
    .join('\n');
}

describe('an Angular CLI build', () => {
  it('takes the class table of the Tailwind plugin, through @angular-builders/custom-esbuild', async () => {
    const bundle = await build();
    expect(bundle).toContain('@inkoxide/tailwind table');
    expect(bundle).toContain('text-sky-700');
    expect(bundle).not.toContain('candidatesToCss');
  }, 120_000);

  it("keeps Tailwind's compiler without the plugin", async () => {
    const bundle = await build('compiler');
    expect(bundle).not.toContain('@inkoxide/tailwind table');
    expect(bundle).toContain('candidatesToCss');
  }, 120_000);
});
