// Browser tests of the built site, see the README. `npm run e2e` from the repository root.
import { defineConfig, devices } from '@playwright/test';

import { BASE_URL, VISUAL, VISUAL_URL } from './env';

const CI = !!process.env.CI;
const desktop = { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } };

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
  expect: {
    timeout: 10_000,
    // Small enough for a moved line of text, big enough for antialiasing
    toHaveScreenshot: { maxDiffPixelRatio: 0.01, animations: 'disabled', caret: 'hide' },
  },
  // One set of screenshots for every machine: they are only taken in the Playwright container
  snapshotPathTemplate: '{testDir}/__screenshots__/{testFileName}/{arg}{ext}',
  use: {
    baseURL: BASE_URL,
    // Sent as Accept-Language: pages without a language prefix are in the browser's language
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'light',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', testIgnore: /visual\.spec\.ts/, use: desktop },
    ...(VISUAL ? [{ name: 'visual', testMatch: /visual\.spec\.ts/, use: { ...desktop, baseURL: VISUAL_URL } }] : []),
  ],
});
