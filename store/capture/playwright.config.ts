// The store screenshots (app/README.md, Store listings): the built site on a seeded local
// database, every language at phone and tablet size, framed with a caption. Not a test suite:
// `pnpm run store:images` from the repository root writes target/store/images.
import { defineConfig } from '@playwright/test';

import { STORE_URL } from './env';

const CI = !!process.env.CI;

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
  timeout: 240_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: STORE_URL,
    timezoneId: 'UTC',
    colorScheme: 'light',
    trace: 'retain-on-failure',
    browserName: 'chromium',
  },
});
