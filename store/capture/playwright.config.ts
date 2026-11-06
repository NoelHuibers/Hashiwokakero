// The store screenshots (app/README.md, Store listings): the built site on a seeded local
// database, every language at phone and tablet size, framed with a caption. Not a test suite:
// `pnpm run store:images` from the repository root writes target/store/images. CI splits the
// languages over several runners (`--shard=1/5`, .github/workflows/store-listing.yml).
import { defineConfig } from '@playwright/test';

import { STORE_URL } from './env.ts';

const CI = Boolean(process.env.CI);

export default defineConfig({
  testDir: '.',
  testMatch: /\.capture\.ts$/,
  outputDir: '../../target/store/test-results',
  globalSetup: './global-setup.ts',
  // A test per language, side by side (the runner has four cores)
  fullyParallel: true,
  workers: 4,
  retries: CI ? 1 : 0,
  forbidOnly: CI,
  reporter: CI ? [['github'], ['line']] : [['line']],
  // A language: about 3 minutes with four side by side on CI's four cores, room for a slow one
  timeout: 600_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: STORE_URL,
    timezoneId: 'UTC',
    colorScheme: 'light',
    // Not on the first try: tracing every screenshot of a 3× phone costs CPU the run needs
    trace: 'on-first-retry',
    browserName: 'chromium',
  },
});
