// How long the site's pages take to hydrate (e2e/perf/), apart from the browser tests: one test,
// one worker, no retries. `pnpm run perf:hydration` from the repository root, on a release build;
// `pnpm run bundle-size` puts the times next to the bundle's sizes. See
// docs/performance/bundle-baseline.md.
import { defineConfig, devices } from '@playwright/test';

import { BASE_URL } from './env';

export default defineConfig({
  testDir: './perf',
  outputDir: './test-results-perf/output',
  // The browser tests' seeded database and server, the pages show what they show there
  globalSetup: './global-setup.ts',
  forbidOnly: !!process.env.CI,
  workers: 1,
  // Every page loads several times on a slowed-down CPU
  timeout: 5 * 60_000,
  reporter: [['line']],
  // The test opens a context per load, with the browser tests' desktop settings
  use: { ...devices['Desktop Chrome'], baseURL: BASE_URL },
});
