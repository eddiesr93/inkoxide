import { defineConfig, devices } from '@playwright/test';

import { CDN_PORT } from './vite.config.ts';

/**
 * The built pages, served by `vite preview` with their Content Security Policies, and again,
 * cross-origin isolated, on ISOLATED_PORT (tests/threads.spec.ts); and a build whose scripts and
 * assets CDN_PORT serves, as a CDN would, for pages on CDN_PAGE_PORT (tests/cdn.spec.ts). The
 * ports are not examples/browser's, so that both run at once.
 */
const PORT = 4183;
export const ISOLATED_PORT = 4184;
export const CDN_PAGE_PORT = 4186;

export default defineConfig({
  testDir: 'tests',
  timeout: 60_000,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? 'github' : 'list',
  use: { baseURL: `http://localhost:${PORT}` },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  // Started in order: the second serves what the first built.
  webServer: [
    {
      command: `vite build --configLoader runner && vite preview --configLoader runner --port ${PORT} --strictPort`,
      port: PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: `vite preview --configLoader runner --port ${ISOLATED_PORT} --strictPort`,
      env: { INKOXIDE_ISOLATED: '1' },
      port: ISOLATED_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: `vite build --configLoader runner && vite preview --configLoader runner --port ${CDN_PORT} --strictPort`,
      env: { INKOXIDE_CDN: 'assets' },
      port: CDN_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: `vite preview --configLoader runner --port ${CDN_PAGE_PORT} --strictPort`,
      env: { INKOXIDE_CDN: 'page' },
      port: CDN_PAGE_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
  ],
});
