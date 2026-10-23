// The `test` of the Android specs: every test starts the installed app afresh (no saved games,
// settings or session) and gets its WebView as `page`. Like e2e/fixtures.ts it fails a test on
// any error in the page, here also on a content security policy violation and on any answer
// from the server, which the device must not reach.
import { test as base, expect, type AndroidDevice, type Page, type TestInfo } from '@playwright/test';

import { ACTIVITY, connect, ORIGIN, PKG, SERVER, shell } from './device';

export { expect };

/** The browser's notes about requests that couldn't go out: expected without a network. */
const NETWORK_ERROR = /ERR_INTERNET_DISCONNECTED|ERR_NAME_NOT_RESOLVED|ERR_ADDRESS_UNREACHABLE|ERR_NETWORK_CHANGED/;

export const test = base.extend<{ start: string; problems: string[]; consoleLog: void }, { device: AndroidDevice }>({
  /** What `am start` starts the app with: its launcher's activity, or an intent (an App Link). */
  start: [`-n ${ACTIVITY}`, { option: true }],

  device: [
    async ({}, use) => {
      const device = await connect();
      await use(device);
      await device.close();
    },
    { scope: 'worker' },
  ],

  page: async ({ device, start }, use, testInfo) => {
    const page = await launch(device, start, testInfo);
    await use(page);
    if (testInfo.status !== testInfo.expectedStatus) {
      await testInfo.attach('screen', { body: await device.screenshot(), contentType: 'image/png' });
    }
  },

  // Every console message, attached to the test: what the WebView said, for the report
  consoleLog: [
    async ({ page }, use, testInfo) => {
      const lines: string[] = [];
      page.on('console', (m) => lines.push(`${m.type()}: ${m.text()}`));
      page.on('pageerror', (e) => lines.push(`page error: ${e.message}`));
      await use();
      if (lines.length) await testInfo.attach('console', { body: lines.join('\n'), contentType: 'text/plain' });
    },
    { auto: true },
  ],

  problems: [
    async ({ page }, use) => {
      const problems: string[] = [];
      page.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
      page.on('console', (m) => {
        const text = m.text();
        if (/Content.Security.Policy/i.test(text)) problems.push(`CSP: ${text}`);
        else if (m.type() === 'error' && !NETWORK_ERROR.test(text)) problems.push(`console error: ${text}`);
      });
      // The device is offline: an answer from the server means a request got out
      page.on('response', (r) => {
        if (new URL(r.url()).hostname === SERVER) problems.push(`reached the server: ${r.request().method()} ${r.url()}`);
      });
      await use(problems);
      // The page the app started with has no listener yet. A test that stopped the app leaves a
      // page that may not know it's gone yet
      const read = page
        .evaluate(() => (window as unknown as { cspViolations?: string[] }).cspViolations ?? [])
        .catch(() => [] as string[]);
      const seen = page.isClosed() ? [] : await within(10_000, 'reading the violations', read).catch(() => []);
      problems.push(...seen.map((v) => `CSP: ${v}`));
      expect(problems, 'errors in the app').toEqual([]);
    },
    { auto: true },
  ],
});

/**
 * Starts the app with its data cleared (with `am start <start>`) and waits for its WebView to
 * hydrate. Playwright finds
 * the WebView by the debugging socket that debug builds open (`webview_devtools_remote_<pid>`).
 * Attaching to it now and then never ends on the emulator: then the app starts again, up to
 * three times, each one noted for the job summary.
 */
async function launch(device: AndroidDevice, start: string, testInfo: TestInfo): Promise<Page> {
  return test.step('start the app', async () => {
    for (let attempt = 1; ; attempt++) {
      // Also stops the app; then its old WebView goes away
      expect(await shell(device, `pm clear ${PKG}`)).toBe('Success');
      await expect.poll(() => device.webViews().some((w) => w.pkg() === PKG), { message: 'the old WebView is gone' }).toBe(false);
      const started = Date.now();
      expect(await shell(device, `am start -W ${start}`), 'the activity started').toMatch(/Status: ok/);
      const webView = await device.webView({ pkg: PKG }, { timeout: 60_000 });
      const attached = webView.page();
      const page = await within(20_000, 'attaching to the WebView', attached).catch((e: Error) => {
        attached.catch(() => {});
        if (attempt === 3) throw e;
        testInfo.annotations.push({ type: 'relaunch', description: e.message });
        return null;
      });
      if (!page) continue;
      await closeSystemDialogs(device);
      // Only the mark: the requests the page sent before Playwright was attached never end for it
      await expect(page.locator('html')).toHaveAttribute('data-hydrated', '', { timeout: 60_000 });
      testInfo.annotations.push({ type: 'launch', description: `${Date.now() - started} ms` });
      // From the next page on, the policy's violations are kept (read by `violations`)
      await page.addInitScript(() => {
        const seen: string[] = [];
        Object.defineProperty(window, 'cspViolations', { value: seen });
        document.addEventListener('securitypolicyviolation', (e) =>
          seen.push(`${e.effectiveDirective} blocked ${e.blockedURI || 'inline'} (${e.sourceFile || e.documentURI}:${e.lineNumber})`),
        );
      });
      return page;
    }
  });
}

/** `promise`, or an error after `ms`: some of Playwright's Android calls take no timeout. */
async function within<T>(ms: number, what: string, promise: Promise<T>): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} took more than ${ms} ms`)), ms);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/** The content security policy's violations on the current page, since it loaded. */
export async function violations(page: Page): Promise<string[]> {
  const seen = await page.evaluate(() => (window as unknown as { cspViolations?: string[] }).cspViolations ?? null);
  expect(seen, 'the listener for violations is on the page').not.toBeNull();
  return seen!;
}

/** Opens a page of the app, a new document like a reload, and waits until it hydrated. */
export async function visit(page: Page, path: string) {
  await page.goto(`${ORIGIN}${path}`);
  await hydrated(page);
}

/** Waits for the mark App sets once the WASM rendered the page (src/lib.rs), then for its calls. */
export async function hydrated(page: Page) {
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '', { timeout: 60_000 });
  await page.waitForLoadState('networkidle');
}

/** Whether the app's activity is the one in front. */
export async function inFront(device: AndroidDevice) {
  const activities = await shell(device, 'dumpsys activity activities');
  return (activities.match(/(?:mResumedActivity|topResumedActivity)[^\n]*/g) ?? []).some((l) => l.includes(PKG));
}

/** The activities the device started since `logcat -c`. */
export const started = async (device: AndroidDevice) =>
  (await shell(device, 'logcat -d -s ActivityTaskManager:I')).split('\n').filter((l) => /START u\d+ \{/.test(l));

/** The window that gets the keys (`mCurrentFocus`): `<package>/<activity>`, or a dialog's title. */
export async function focused(device: AndroidDevice) {
  return (await shell(device, 'dumpsys window')).match(/mCurrentFocus=Window\{\S+ u\d+ ([^}]*)\}/)?.[1] ?? '';
}

/**
 * Closes a system dialog in front of the app, noted for the job summary. Android shows one when
 * another app stops answering (a busy emulator's launcher did) or crashes; it dims the screen and
 * takes the back button. The global setup turns them off (`hide_error_dialogs`): this is for the
 * one that comes anyway, before what a dialog would spoil.
 */
export async function closeSystemDialogs(device: AndroidDevice) {
  const focus = () => focused(device);
  const dialog = await focus();
  if (!/^Application (Not Responding|Error):/.test(dialog)) return;
  test.info().annotations.push({ type: 'system dialog', description: dialog });
  await shell(device, 'am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS');
  await expect.poll(focus, { message: 'the system dialog closed' }).not.toBe(dialog);
}
