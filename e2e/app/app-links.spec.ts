// The App Links the app is opened with (src/api/native.rs, `follow_app_links`): the page of the
// link the app started with, and of those that bring it back to the front later, shown by the
// router without loading the page anew. A fake `window.__TAURI_INTERNALS__` stands in for Tauri
// and its deep-link plugin: `get_current` answers the link the app started with, and the test
// sends the next ones as the plugin's event. The emulator test (e2e/android) opens the real app
// with Android's intents.
import type { Page } from '@playwright/test';

import { expect, test, visit } from '../fixtures';

const EVENT = 'deep-link://new-url';

/** Tauri with the deep-link plugin, started with the App Link `start` (or none). */
async function fakeTauri(page: Page, start: string | null) {
  await page.addInitScript(
    ([start, event]) => {
      const callbacks = new Map<number, (message: unknown) => void>();
      const listeners: { event: string; handler: number }[] = [];
      let next = 1;
      Object.assign(window, {
        __TAURI_INTERNALS__: {
          transformCallback(callback: (message: unknown) => void) {
            const id = next++;
            callbacks.set(id, callback);
            return id;
          },
          invoke(cmd: string, args: { event: string; handler: number }) {
            if (cmd === 'plugin:deep-link|get_current') return Promise.resolve(start === null ? null : [start]);
            if (cmd === 'plugin:event|listen') {
              listeners.push(args);
              return Promise.resolve(listeners.length);
            }
            // The other plugins' commands (the system bars' icons) need no answer
            return Promise.resolve(null);
          },
        },
        // What the plugin sends when Android brings the app back with a link
        openAppLink(url: string) {
          const heard = listeners.filter((l) => l.event === event);
          for (const l of heard) callbacks.get(l.handler)!({ event, id: l.handler, payload: [url] });
          return heard.length;
        },
      });
    },
    [start, EVENT] as const,
  );
}

/** Sends the App Link `url` to the running app, as Android does through the plugin. */
async function openAppLink(page: Page, url: string) {
  const heard = await page.evaluate((url) => (window as unknown as { openAppLink(url: string): number }).openAppLink(url), url);
  expect(heard, 'the page listens for App Links').toBe(1);
}

/** Marks the document: the mark is gone if the page was loaded anew. */
const mark = (page: Page) => page.evaluate(() => Object.assign(window, { sameDocument: true }));
const sameDocument = (page: Page) => page.evaluate(() => (window as unknown as { sameDocument?: boolean }).sameDocument === true);

/** The server is out of reach, as on a phone without a network: the pages show it calmly. */
async function offline(page: Page) {
  await page.route(/\/api\//, (route) => route.abort('internetdisconnected'));
}

function withoutNetworkErrors(problems: string[]) {
  const rest = problems.filter((p) => !/ERR_INTERNET_DISCONNECTED|Failed to load resource/.test(p));
  problems.splice(0, problems.length, ...rest);
}

test('the app started with an invite shows the invite, in its language', async ({ page, problems }) => {
  await fakeTauri(page, 'https://hashi.rs/ja/g/testcode');
  await page.addInitScript(() => Object.assign(window, { startHistory: history.length }));
  await offline(page);
  await visit(page, '/');
  await expect(page).toHaveURL(/\/ja\/g\/testcode$/);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  // Instead of the start page: the back button leaves the app rather than going back to it
  expect(await page.evaluate(() => history.length)).toBe(await page.evaluate(() => (window as unknown as { startHistory: number }).startHistory));

  // A reload doesn't go back to the invite: the plugin still has it, the page showed it already
  await page.locator('a[href="/ja/practice"]').filter({ visible: true }).first().click();
  await expect(page).toHaveURL(/\/ja\/practice$/);
  await page.reload();
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  await expect(page).toHaveURL(/\/ja\/practice$/);
  withoutNetworkErrors(problems);
});

test('links that bring the app back show their page on top of the one it was on', async ({ page, problems }) => {
  await fakeTauri(page, null);
  await offline(page);
  await visit(page, '/rules');
  await mark(page);

  await openAppLink(page, 'https://hashi.rs/g/abc');
  await expect(page).toHaveURL(/\/g\/abc$/);
  // A daily's result: the daily
  await openAppLink(page, 'https://hashi.rs/');
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByText("Couldn't load today's puzzle. Try again.")).toBeVisible();
  // The router, not a new page
  expect(await sameDocument(page)).toBe(true);
  await page.goBack();
  await expect(page).toHaveURL(/\/g\/abc$/);
  await page.goBack();
  await expect(page).toHaveURL(/\/rules$/);

  // The same link twice shows it twice
  await openAppLink(page, 'https://hashi.rs/practice');
  await expect(page).toHaveURL(/\/practice$/);
  await page.goBack();
  await openAppLink(page, 'https://hashi.rs/practice');
  await expect(page).toHaveURL(/\/practice$/);
  expect(await sameDocument(page)).toBe(true);
  withoutNetworkErrors(problems);
});

test('other links change nothing', async ({ page, problems }) => {
  await fakeTauri(page, 'https://hashi.rs/admin');
  await offline(page);
  await visit(page, '/rules');
  await page.waitForTimeout(500);
  await expect(page).toHaveURL(/\/rules$/);
  for (const url of ['https://hashi.rs/admin', 'https://hashi.rs/g/../admin', 'https://example.com/g/abc', 'https://hashi.rs/dojo']) {
    await openAppLink(page, url);
  }
  await page.waitForTimeout(500);
  await expect(page).toHaveURL(/\/rules$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');
  withoutNetworkErrors(problems);
});
