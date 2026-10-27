// The achievements' icons (store/capture/achievements.icons.ts): `pnpm run store:icons` writes
// target/store/achievements/icons. Unlike the screenshots, no site and no server: only the
// fonts `pnpm install` puts in public/fonts.
import { defineConfig } from '@playwright/test';

import { STORE_URL } from './env';

const CI = !!process.env.CI;

export default defineConfig({
  testDir: '.',
  testMatch: /\.icons\.ts$/,
  outputDir: '../../target/store/test-results',
  fullyParallel: true,
  retries: CI ? 1 : 0,
  forbidOnly: CI,
  reporter: CI ? [['github'], ['line']] : [['line']],
  timeout: 120_000,
  use: {
    baseURL: STORE_URL,
    browserName: 'chromium',
  },
});
