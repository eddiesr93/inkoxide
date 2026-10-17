import { defineConfig, devices } from '@playwright/test';

/**
 * The built page, served by `vite preview` with its Content Security Policy, and again, cross-origin
 * isolated, on ISOLATED_PORT (tests/threads.spec.ts).
 */
const PORT = 4179;
export const ISOLATED_PORT = 4180;

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
  ],
});
