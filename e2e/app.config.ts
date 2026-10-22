// The smoke test of the mobile app's web bundle (app/, see app/README.md), apart from the
// website's tests. `pnpm run app:build`, then `pnpm run app:e2e` from the repository root.
import { defineConfig, devices } from '@playwright/test';

import { APP_URL } from './app/serve';

const CI = !!process.env.CI;

export default defineConfig({
  testDir: './app',
  outputDir: './test-results-app',
  globalSetup: './app/global-setup.ts',
  forbidOnly: CI,
  retries: CI ? 1 : 0,
  reporter: CI ? [['github'], ['line'], ['html', { open: 'never', outputFolder: 'playwright-report-app' }]] : [['line']],
  // The first page compiles the WASM, which takes a while on a busy runner
  expect: { timeout: 10_000 },
  use: {
    baseURL: APP_URL,
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'light',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'app', use: { ...devices['Pixel 7'] } }],
});
