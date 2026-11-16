// Notifications in the browser (Web Push): turning them on asks for permission, registers the
// service worker, subscribes and registers this browser with the server; off and signing out undo
// it. The test server has no key pair, so the page gets one here (it only needs the public key:
// nothing is sent), and Chromium in the tests has no push service, so `PushManager` is a stand-in
// that hands out a subscription and notes what happens to it.
import { DatabaseSync } from 'node:sqlite';

import type { BrowserContext, Page } from '@playwright/test';

import { register } from '../account.ts';
import { BASE_URL, DB_PATH } from '../env.ts';
import { expect, hydrated, newUsername, test, visit } from '../fixtures.ts';

// RFC 8291's example keys: a valid P-256 point and authentication secret
const PUBLIC_KEY =
  'BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8';
const P256DH =
  'BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4';
const AUTH = 'BTBZMqHH6r4Tts7J_aSIgg';
const STORED = 'hashi.push.v1';

const card = (page: Page) => page.getByTestId('notification-settings');
const master = (page: Page) => card(page).getByRole('switch', { name: 'Notifications' });
const thisBrowser = (page: Page) => card(page).getByTestId('notify-this-browser-state');

/** The pages name the site's push key, as with VAPID_PUBLIC_KEY set (`app::shell`). */
async function withPushKey(page: Page) {
  await page.route(`${BASE_URL}/**`, async (route) => {
    if (route.request().resourceType() !== 'document') return route.fallback();
    const response = await route.fetch();
    const html = await response.text();
    const empty = '<meta name="hashi-push-key">';
    expect(html).toContain(empty);
    const headers = { ...response.headers() };
    delete headers['content-encoding'];
    delete headers['content-length'];
    return route.fulfill({
      response,
      headers,
      body: html.replace(empty, `<meta name="hashi-push-key" content="${PUBLIC_KEY}">`),
    });
  });
}

interface PushMock {
  /** What `Notification.requestPermission()` answers, `undefined` for the browser's own. */
  answer?: NotificationPermission;
}

/**
 * The stand-in push service: `subscribe` and `getSubscription` of every `PushManager`, the
 * subscription kept in local storage across reloads, and what was asked of it in `e2e.push.log`.
 */
async function mockPush(page: Page, { answer }: PushMock = {}) {
  await page.addInitScript(
    ({ answer, p256dh, auth }) => {
      const log = (line: string) => {
        const lines = JSON.parse(localStorage.getItem('e2e.push.log') ?? '[]') as string[];
        localStorage.setItem('e2e.push.log', JSON.stringify([...lines, line]));
      };
      if (answer) {
        let permission: NotificationPermission = 'default';
        Object.defineProperty(Notification, 'permission', { get: () => permission });
        Notification.requestPermission = () => {
          permission = answer;
          log(`permission ${answer}`);
          return Promise.resolve(answer);
        };
      }
      const subscription = (endpoint: string) => ({
        endpoint,
        expirationTime: null,
        toJSON: () => ({ endpoint, expirationTime: null, keys: { p256dh, auth } }),
        unsubscribe: () => {
          localStorage.removeItem('e2e.push.endpoint');
          log(`unsubscribe ${endpoint}`);
          return Promise.resolve(true);
        },
      });
      const current = () => {
        const endpoint = localStorage.getItem('e2e.push.endpoint');
        return endpoint ? subscription(endpoint) : null;
      };
      PushManager.prototype.subscribe = (options?: PushSubscriptionOptionsInit) => {
        const key = options?.applicationServerKey;
        const length = key instanceof ArrayBuffer || ArrayBuffer.isView(key) ? key.byteLength : 0;
        log(`subscribe userVisibleOnly=${String(options?.userVisibleOnly)} key=${length}`);
        const endpoint = `https://push.example.test/e2e/${crypto.randomUUID()}`;
        localStorage.setItem('e2e.push.endpoint', endpoint);
        return Promise.resolve(subscription(endpoint) as unknown as PushSubscription);
      };
      PushManager.prototype.getSubscription = () =>
        Promise.resolve(current() as unknown as PushSubscription | null);
    },
    { answer, p256dh: P256DH, auth: AUTH },
  );
}

const pushLog = (page: Page) =>
  page.evaluate(() => JSON.parse(localStorage.getItem('e2e.push.log') ?? '[]') as string[]);

const stored = (page: Page) =>
  page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key) ?? 'null') as { id: number } | null,
    STORED,
  );

/** This player's registered browsers in the server's database. */
function devices(username: string) {
  const db = new DatabaseSync(DB_PATH, { readOnly: true, timeout: 5_000 });
  try {
    return db
      .prepare(
        `SELECT d.id, d.kind, d.endpoint, d.keys, d.locale FROM notification_devices d
         JOIN users u ON u.id = d.user_id WHERE u.username = ?`,
      )
      .all(username) as {
      id: number;
      kind: string;
      endpoint: string;
      keys: string;
      locale: string;
    }[];
  } finally {
    db.close();
  }
}

async function grant(context: BrowserContext) {
  await context.grantPermissions(['notifications'], { origin: BASE_URL });
}

test('turning notifications on subscribes this browser, off unsubscribes it', async ({
  page,
  context,
}) => {
  await grant(context);
  await withPushKey(page);
  await mockPush(page);
  const username = newUsername('webpush');
  await visit(page, '/account');
  await register(page, username, 'bells-over-bridges');
  await expect(master(page)).toBeEnabled();
  await expect(card(page).getByTestId('notifications-unavailable')).toBeHidden();
  // Nothing to say about this browser while everything is off
  await expect(card(page).getByTestId('notify-this-browser')).toBeHidden();

  await master(page).click();
  await expect(master(page)).toHaveAttribute('aria-checked', 'true');
  await expect(thisBrowser(page)).toHaveText('Gets notifications.');
  expect(await pushLog(page)).toEqual(['subscribe userVisibleOnly=true key=65']);
  const registered = devices(username);
  expect(registered).toHaveLength(1);
  const [device] = registered;
  expect(device?.kind).toBe('web');
  expect(device?.endpoint).toMatch(/^https:\/\/push\.example\.test\/e2e\//);
  expect(JSON.parse(device?.keys ?? '{}')).toEqual({ p256dh: P256DH, auth: AUTH });
  expect(device?.locale).toBe('en');
  expect((await stored(page))?.id).toBe(device?.id);
  // The service worker at its fixed address
  const worker = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.getRegistration();
    return registration?.active?.scriptURL ?? null;
  });
  expect(worker).toBe(`${BASE_URL}/sw.js`);

  // Still on after a reload: the account's switch and this browser
  await page.reload();
  await hydrated(page);
  await expect(master(page)).toHaveAttribute('aria-checked', 'true');
  await expect(thisBrowser(page)).toHaveText('Gets notifications.');

  // This browser off, the account stays on
  await card(page).getByRole('button', { name: 'Turn off here' }).click();
  await expect(thisBrowser(page)).toHaveText('Gets no notifications.');
  await expect.poll(() => devices(username)).toEqual([]);
  expect(await stored(page)).toBeNull();
  expect(await pushLog(page)).toContain(`unsubscribe ${device?.endpoint}`);
  await expect(master(page)).toHaveAttribute('aria-checked', 'true');

  // And on again
  await card(page).getByRole('button', { name: 'Turn on here' }).click();
  await expect(thisBrowser(page)).toHaveText('Gets notifications.');
  await expect.poll(() => devices(username).length).toBe(1);

  // The master switch off turns this browser off too
  await master(page).click();
  await expect(master(page)).toHaveAttribute('aria-checked', 'false');
  await expect(card(page).getByTestId('notify-this-browser')).toBeHidden();
  await expect.poll(() => devices(username)).toEqual([]);
  expect(await stored(page)).toBeNull();

  // Signing out does as well
  await master(page).click();
  await expect(thisBrowser(page)).toHaveText('Gets notifications.');
  await expect.poll(() => devices(username).length).toBe(1);
  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page.getByText('Signed out.')).toBeVisible();
  await expect.poll(() => devices(username)).toEqual([]);
  expect(await stored(page)).toBeNull();
});

test('a browser that may not notify keeps the switch off and says how to allow it', async ({
  page,
}) => {
  await withPushKey(page);
  await mockPush(page, { answer: 'denied' });
  const username = newUsername('blocked');
  await visit(page, '/account');
  await register(page, username, 'bells-over-bridges');
  await expect(master(page)).toBeEnabled();

  await master(page).click();
  await expect(card(page).getByTestId('notifications-blocked')).toContainText(
    'Your browser blocks notifications from Hashi.',
  );
  await expect(master(page)).toHaveAttribute('aria-checked', 'false');
  await expect(master(page)).toBeEnabled();
  expect(await pushLog(page)).toEqual(['permission denied']);
  expect(devices(username)).toEqual([]);
  // Nothing was saved
  await page.reload();
  await hydrated(page);
  await expect(master(page)).toBeEnabled();
  await expect(master(page)).toHaveAttribute('aria-checked', 'false');
});

test('Safari on an iPhone points to the home screen', async ({ browser }) => {
  const context = await browser.newContext({
    userAgent:
      'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1',
  });
  const page = await context.newPage();
  await withPushKey(page);
  // Safari's tabs on iOS have no push API
  await page.addInitScript(() => {
    Reflect.deleteProperty(window, 'PushManager');
  });
  await visit(page, '/account');
  await register(page, newUsername('iphone'), 'bells-over-bridges');
  await expect(card(page).getByTestId('notifications-home-screen')).toContainText(
    'Add to Home Screen',
  );
  await expect(card(page).getByTestId('notifications-unavailable')).toBeHidden();
  await context.close();
});
