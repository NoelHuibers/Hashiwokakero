// App Links: links to the app's pages on https://hashi.rs (an invite, a daily's result), given to
// the debug APK as Android's VIEW intents, with the app not running (cold) and on top of the page
// it shows (warm). The app shows their page itself, with its router: the WebView stays on the
// app's origin, no browser starts, and nothing reaches hashi.rs (the device is offline, see
// app.ts). Android never verifies the debug build's links (its certificate isn't in the site's
// assetlinks.json), so the intents name the package, as `adb` lets them.
import type { AndroidDevice, Page } from '@playwright/test';

import { closeSystemDialogs, expect, hydrated, inFront, started, test, visit } from './app';
import { ACTIVITY, ORIGIN, PKG, shell } from './device';

const INVITE = 'https://hashi.rs/g/testcode';
/** Where a daily result's share text points, in Japanese. */
const DAILY_JA = 'https://hashi.rs/ja';

/** A tapped link to `url` as Android hands it to the app (`-n`: to its activity whatever the link). */
const view = (url: string, target = PKG) => `-a android.intent.action.VIEW -d ${url} ${target}`;

/** The toasts on the screen (src/components/layout/toaster.rs). */
const offlineNotice = (page: Page) => page.locator('[role=log] > div').filter({ hasText: "You're offline. Practice still works." });

/** Navigations of the WebView away from the app's pages: it would leave the app for the website. */
function navigationsOut(page: Page) {
  const out: string[] = [];
  page.on('request', (r) => {
    if (r.isNavigationRequest() && new URL(r.url()).origin !== ORIGIN) out.push(r.url());
  });
  return out;
}

/** Only the app's own activity started since the logcat was cleared: no browser, no chooser. */
async function onlyTheApp(device: AndroidDevice) {
  await closeSystemDialogs(device);
  const starts = await started(device);
  expect(starts.length, 'the intent reached an activity').toBeGreaterThan(0);
  expect(
    starts.filter((l) => !l.includes(`cmp=${PKG}/`)),
    'activities of other apps',
  ).toEqual([]);
  expect(await inFront(device), 'the app in front').toBe(true);
}

/** The invite's page without a network: it waits for the group, and says once that it's offline. */
async function offlineInvite(page: Page) {
  await expect(page.getByText('Loading…')).toBeVisible();
  await expect(offlineNotice(page)).toHaveCount(1);
}

test.describe('started by an invite', () => {
  test.use({ start: view(INVITE) });

  test('the app shows the invite', async ({ page, device }) => {
    const out = navigationsOut(page);
    await expect(page).toHaveURL(`${ORIGIN}/g/testcode`);
    await offlineInvite(page);
    await onlyTheApp(device);
    // Instead of the start page, not on top of it: the back button leaves the app
    expect(await page.evaluate(() => history.length)).toBe(1);
    expect(out).toEqual([]);
  });
});

test.describe("started by a daily's result", () => {
  test.use({ start: view(DAILY_JA) });

  test('the app shows the daily, in its language', async ({ page, device }) => {
    await expect(page).toHaveURL(`${ORIGIN}/ja`);
    await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
    await expect(page.getByRole('link', { name: /ルール/ }).first()).toBeVisible();
    await onlyTheApp(device);
  });
});

test('links bring the running app to their page', async ({ page, device }) => {
  await visit(page, '/rules');
  const out = navigationsOut(page);
  await page.evaluate(() => Object.assign(window, { sameDocument: true }));

  await shell(device, 'logcat -c');
  expect(await shell(device, `am start -W ${view(INVITE)}`)).toMatch(/Status: ok/);
  await expect(page).toHaveURL(`${ORIGIN}/g/testcode`);
  await offlineInvite(page);
  await onlyTheApp(device);

  // From the app in the background too
  await shell(device, 'input keyevent KEYCODE_HOME');
  await expect.poll(() => inFront(device), { message: 'the app in the background' }).toBe(false);
  await shell(device, 'logcat -c');
  expect(await shell(device, `am start -W ${view(DAILY_JA)}`)).toMatch(/Status: ok/);
  await expect(page).toHaveURL(`${ORIGIN}/ja`);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  await onlyTheApp(device);

  // The router showed them: the same page, with the pages before them in its history
  expect(await page.evaluate(() => (window as unknown as { sameDocument?: boolean }).sameDocument)).toBe(true);
  await shell(device, 'input keyevent KEYCODE_BACK');
  await expect(page).toHaveURL(`${ORIGIN}/g/testcode`);
  expect(out).toEqual([]);
});

test('other pages of the site change nothing', async ({ page, device }) => {
  await visit(page, '/rules');
  const out = navigationsOut(page);
  // No intent filter of the app takes it: Android finds nothing to start in the package
  expect(await shell(device, `am start -W ${view('https://hashi.rs/admin')}`)).toMatch(/unable to resolve Intent/);
  // Sent to the app's activity anyway: the app stays where it is
  for (const url of ['https://hashi.rs/admin', 'https://hashi.rs/dojo/seasons/1/set.tar.gz', 'https://example.com/g/testcode']) {
    await shell(device, 'logcat -c');
    expect(await shell(device, `am start -W ${view(url, `-n ${ACTIVITY}`)}`)).toMatch(/Status: ok/);
    await onlyTheApp(device);
  }
  await page.waitForTimeout(1000);
  expect(page.url()).toBe(`${ORIGIN}/rules`);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');
  expect(out).toEqual([]);
});

test.describe('started by another page of the site', () => {
  test.use({ start: view('https://hashi.rs/admin', `-n ${ACTIVITY}`) });

  test('the app starts on its start page', async ({ page, device }) => {
    await page.waitForTimeout(1000);
    expect(page.url()).toBe(`${ORIGIN}/`);
    await hydrated(page);
    await expect(page.getByText("Couldn't load today's puzzle. Try again.")).toBeVisible();
    await onlyTheApp(device);
  });
});
