// The real Android app on an emulator (or a phone), apart from the website's and the web
// bundle's tests: the debug APK installed and started, its WebView driven through Playwright's
// Android support. Without a network, so nothing reaches hashi.rs. See app/README.md.
// `pnpm run app:android-e2e` from the repository root, with one device on adb.
import path from 'node:path';

import { defineConfig } from '@playwright/test';

const CI = !!process.env.CI;

export default defineConfig({
  testDir: './android',
  outputDir: './test-results-android',
  globalSetup: './android/global-setup.ts',
  forbidOnly: CI,
  // An emulator is flaky by nature: one retry here, and only here. The job summary counts the
  // flaky tests, so they stay visible.
  retries: 1,
  // One device, one app: the tests take turns
  workers: 1,
  fullyParallel: false,
  // Each test starts the app afresh, which compiles the WASM on the emulator's CPU
  timeout: 120_000,
  expect: { timeout: 20_000 },
  reporter: [
    ...(CI ? ([['github']] as const) : []),
    ['line'],
    ['html', { open: 'never', outputFolder: 'playwright-report-android' }],
    // For the job summary: passed, failed and flaky tests, the launch times
    ['json', { outputFile: path.join(import.meta.dirname, 'test-results-android.json') }],
  ],
});
