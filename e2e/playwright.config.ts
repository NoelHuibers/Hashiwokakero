// Browser tests of the built site, see the README. `npm run e2e` from the repository root.
import { defineConfig, devices } from '@playwright/test';

import { BASE_URL } from './env';

const CI = !!process.env.CI;

export default defineConfig({
  testDir: './tests',
  outputDir: './test-results',
  globalSetup: './global-setup.ts',
  fullyParallel: true,
  forbidOnly: CI,
  // One retry with a trace, to see what went wrong; flaky tests still show up in the report
  retries: CI ? 1 : 0,
  workers: CI ? 4 : undefined,
  reporter: CI
    ? [['github'], ['dot'], ['html', { open: 'never', outputFolder: 'playwright-report' }]]
    : [['line'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],
  // The first page of a worker compiles the WASM, which takes a while on a busy runner
  expect: { timeout: 10_000 },
  use: {
    baseURL: BASE_URL,
    // Sent as Accept-Language: pages without a language prefix are in the browser's language
    locale: 'en-US',
    colorScheme: 'light',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } } }],
});
