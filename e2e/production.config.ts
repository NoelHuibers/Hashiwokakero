// The deployed site in a browser (e2e/production/), apart from the browser tests of a local build:
// its main pages load, hydrate and react, with every asset there. Read-only, it never makes a guest
// or anything else in the production database. `pnpm run smoke:production` from the repository
// root; SMOKE_URL points it at another deployment, SMOKE_BUILD names the build it must serve.
import { defineConfig, devices } from '@playwright/test';

const CI = !!process.env.CI;

const SMOKE_URL = process.env.SMOKE_URL || 'https://hashi.rs';

export default defineConfig({
  testDir: './production',
  outputDir: './test-results-production',
  forbidOnly: CI,
  fullyParallel: true,
  // Over the internet: one retry for a dropped connection, flaky tests still show in the report
  retries: CI ? 1 : 0,
  // Gentle on the site, and on its rate limits
  workers: 2,
  // A cold machine of the site may take a moment to start
  expect: { timeout: 20_000 },
  reporter: CI
    ? [['github'], ['line'], ['html', { open: 'never', outputFolder: 'playwright-report-production' }]]
    : [['line']],
  use: {
    ...devices['Desktop Chrome'],
    viewport: { width: 1280, height: 900 },
    baseURL: SMOKE_URL,
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'light',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
});
