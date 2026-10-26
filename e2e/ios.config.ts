// The real iOS app on a simulator, apart from the website's and the web bundle's tests: the debug
// build for the simulator installed and started, the page in its WKWebView driven through Appium
// (e2e/ios/app.ts), Playwright only runs the tests. With hashi.rs blocked, so nothing reaches it.
// See app/README.md. `pnpm run app:ios-e2e` from the repository root, on a Mac with one booted
// simulator and Appium's XCUITest driver.
import path from 'node:path';

import { defineConfig } from '@playwright/test';

const CI = !!process.env.CI;

export default defineConfig({
  testDir: './ios',
  outputDir: './test-results-ios',
  globalSetup: './ios/global-setup.ts',
  forbidOnly: CI,
  // A simulator is flaky by nature, like the Android emulator: one retry. The job summary counts
  // the flaky tests, so they stay visible.
  retries: 1,
  // One simulator, one app: the tests take turns
  workers: 1,
  fullyParallel: false,
  timeout: 120_000,
  expect: { timeout: 20_000 },
  reporter: [
    ...(CI ? ([['github']] as const) : []),
    ['line'],
    ['html', { open: 'never', outputFolder: 'playwright-report-ios' }],
    // For the job summary: passed, failed and flaky tests, the launch times
    ['json', { outputFile: path.join(import.meta.dirname, 'test-results-ios.json') }],
  ],
});
