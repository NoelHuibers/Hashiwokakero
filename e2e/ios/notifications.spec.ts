// Notifications through Apple's push service (docs/notifications.md, "The iOS app"), on the
// simulator: iOS's permission and the plugin's commands (HashiPlugin.swift), a notification that
// comes while the app is in front shows as a banner, and a tap on one opens its page in the app,
// in front and from a closed app (main.mm). The notifications come from `xcrun simctl push`, as
// APNs would hand them to the phone; the server's side (registering, sending) needs hashi.rs and
// Apple, it has its own tests (server/notify/apns). Nothing here reaches hashi.rs. In the order of
// the file: the first test installs the app anew so iOS asks, the taps need the permission.
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import type { TauriWindow } from '../tauri.ts';
import { type App, expect, inFront, test } from './app.ts';
import { APP, BUNDLE, ORIGIN, simctl } from './device.ts';
import { NATIVE, type Session, WebDriverError } from './webdriver.ts';

/** SpringBoard: the home screen, the banners and iOS's alerts. */
const SPRINGBOARD = 'com.apple.springboard';
/** The page a test notification opens: an App Link (seo::app_link) that works offline. */
const LINK = 'https://hashi.rs/groups';

test.describe.configure({ mode: 'serial' });

/** A plugin command of the app, with its answer or its refusal. */
const invoke = (app: App, command: string) =>
  app.evaluate(
    (command) =>
      (window as TauriWindow).__TAURI_INTERNALS__.invoke(command, {}).then(
        (answer: unknown) => ({ answer: answer ?? null }),
        (e: unknown) => ({ refused: typeof e === 'string' ? e : JSON.stringify(e) }),
      ),
    command,
  );

/** Runs `fn` with the native views of SpringBoard as the root of the lookups, then the page again. */
async function onSpringBoard<T>(session: Session, fn: () => Promise<T>): Promise<T> {
  const context = await session.context();
  await session.setContext(NATIVE);
  await session.command('POST', '/appium/settings', {
    settings: { defaultActiveApplication: SPRINGBOARD },
  });
  try {
    return await fn();
  } finally {
    await session.command('POST', '/appium/settings', {
      settings: { defaultActiveApplication: 'auto' },
    });
    await session.setContext(context);
  }
}

/** The first native element the predicate finds, `null` without one. */
async function element(session: Session, predicate: string) {
  try {
    const found = await session.command<Record<string, string>>('POST', '/element', {
      using: '-ios predicate string',
      value: predicate,
    });
    return Object.values(found)[0] ?? null;
  } catch (e) {
    if (e instanceof WebDriverError && e.error === 'no such element') return null;
    throw e;
  }
}

/** SpringBoard's view of the screen, for the report of a failed test. */
async function springBoardSource(session: Session) {
  return onSpringBoard(session, () => session.command<string>('GET', '/source')).catch(
    (e: unknown) => `no source: ${String(e)}`,
  );
}

/**
 * Sends a notification to the app as APNs would (`simctl push`): our payload
 * (server/notify/apns.rs), with `marker` in its title to find its banner.
 */
async function push(udid: string, marker: string, url = LINK) {
  const dir = mkdtempSync(path.join(tmpdir(), 'hashi-push-'));
  try {
    const file = path.join(dir, 'payload.json');
    writeFileSync(
      file,
      JSON.stringify({
        aps: {
          alert: { title: `Race ${marker}`, body: 'Join the lobby before it starts.' },
          sound: 'default',
          'thread-id': 'race',
        },
        url,
      }),
    );
    await simctl('push', udid, BUNDLE, file);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * Taps the banner of the notification with `marker` while it shows (about five seconds, in the
 * Notification Centre's list after that). A lookup in SpringBoard takes a snapshot of its views,
 * seconds on a busy runner: the search is bounded by time, not by tries. How long it took goes
 * into the report.
 */
async function tapBanner(app: App, marker: string) {
  const started = Date.now();
  const tapped = await onSpringBoard(app.session, async () => {
    while (Date.now() - started < 45_000) {
      const banner = await element(app.session, `label CONTAINS ${JSON.stringify(marker)}`);
      if (banner) {
        await app.session.command('POST', `/element/${banner}/click`);
        return true;
      }
      await sleep(250);
    }
    return false;
  });
  test.info().annotations.push({
    type: 'banner',
    description: `${tapped ? 'tapped' : 'not found'} after ${Date.now() - started} ms`,
  });
  if (!tapped) {
    await test.info().attach('springboard', {
      body: await springBoardSource(app.session),
      contentType: 'text/xml',
    });
  }
  expect(tapped, `the banner of ${marker}`).toBe(true);
}

/** The page of the app in the WebKit inspector after a new start, selected. */
async function pageAgain(app: App) {
  await app.session.setContext(NATIVE);
  let context: string | undefined;
  await expect
    .poll(
      async () => {
        const contexts = await app.session.mobile<{ id: string; url?: string }[]>('getContexts', {
          waitForWebviewMs: 5000,
        });
        context = contexts.find((c) => c.id !== NATIVE && c.url?.startsWith(ORIGIN))?.id;
        return context ? 'found' : JSON.stringify(contexts);
      },
      { message: "the app's page in the WebKit inspector", timeout: 60_000 },
    )
    .toBe('found');
  await app.session.setContext(context!);
  await app.hydrated();
}

/**
 * The app installed anew and started: iOS forgets an app's notification permission when it is
 * deleted, so it asks again. Every try of the permission test starts from there, also the retry
 * of one that ended with iOS's alert still open (iOS stores the app's end as a no).
 */
async function reinstall(app: App, udid: string) {
  await app.session.setContext(NATIVE);
  await app.session.mobile('terminateApp', { bundleId: BUNDLE });
  await simctl('uninstall', udid, BUNDLE);
  await simctl('install', udid, APP);
  await app.session.mobile('launchApp', { bundleId: BUNDLE });
  await pageAgain(app);
}

test("iOS asks once, and the plugin gives the push service's token", async ({ app, udid }) => {
  await reinstall(app, udid);
  // Not yet asked: not allowed
  expect(await invoke(app, 'plugin:hashi|notify_state')).toEqual({ answer: { granted: false } });
  // The page's tap: iOS's alert, which waits for the player. The command only starts the request.
  // While a command in the page runs, Appium's XCUITest driver fails it as soon as an alert shows
  // ("unexpected alert open"): a page busy for a moment (the first question to iOS's
  // notification service after the simulator's boot took 73 s once) makes the command outlast
  // the request, and the alert comes before its answer. That alert is the one asked for. From
  // here on nothing runs in the page until it is answered.
  try {
    await app.evaluate(() => {
      Object.assign(window, {
        e2ePermission: (window as TauriWindow).__TAURI_INTERNALS__
          .invoke('plugin:hashi|notify_permission', {})
          .then(
            (answer: unknown) => ({ answer }),
            (e: unknown) => ({ refused: String(e) }),
          ),
      });
      return null;
    });
  } catch (e) {
    if (!(e instanceof WebDriverError && e.error === 'unexpected alert open')) throw e;
    test.info().annotations.push({ type: 'alert', description: 'before the command answered' });
  }
  const allowed = await onSpringBoard(app.session, async () => {
    for (let i = 0; i < 40; i++) {
      const allow = await element(
        app.session,
        "type == 'XCUIElementTypeButton' AND label == 'Allow'",
      );
      if (allow) {
        await app.session.command('POST', `/element/${allow}/click`);
        return true;
      }
      await sleep(250);
    }
    return false;
  });
  if (!allowed)
    await test.info().attach('springboard', {
      body: await springBoardSource(app.session),
      contentType: 'text/xml',
    });
  expect(allowed, "iOS's alert with Allow").toBe(true);
  await expect
    .poll(() =>
      app.evaluate(() => (window as unknown as { e2ePermission?: unknown }).e2ePermission),
    )
    .toEqual({ answer: { granted: true } });
  expect(await invoke(app, 'plugin:hashi|notify_state')).toEqual({ answer: { granted: true } });
  // Asked again: iOS doesn't ask twice, the answer stays
  expect(await invoke(app, 'plugin:hashi|notify_permission')).toEqual({
    answer: { granted: true },
  });

  // The token: a simulator on Apple silicon may get one from Apple's sandbox, a runner's may not
  // (no entitlement in an unsigned build, no way to Apple); either way the command answers
  const token = await invoke(app, 'plugin:hashi|notify_token');
  test.info().annotations.push({ type: 'notify_token', description: JSON.stringify(token) });
  if ('answer' in token) {
    expect(token.answer).toEqual({
      token: expect.stringMatching(/^([0-9a-f]{2}){32,}$/),
      environment: 'development',
    });
  } else {
    expect(token.refused).toContain('device token');
  }
});

test('a notification shows while the app is in front, and a tap opens its page', async ({
  app,
  udid,
}) => {
  // SpringBoard's lookups and, below, a start of the app
  test.slow();
  await app.visit('/rules');
  await push(udid, 'in-front');
  await tapBanner(app, 'in-front');
  // The same page, its router on the link's page (api::native::follow_app_links)
  await expect.poll(() => app.url()).toBe(`${ORIGIN}/groups`);
  expect(await inFront(app.session)).toBe(BUNDLE);
  await app.screenshot('notification-tapped.png');
});

test('a tap on a notification starts the app on its page', async ({ app, udid }) => {
  // SpringBoard's lookups and, below, a start of the app
  test.slow();
  await app.session.mobile('terminateApp', { bundleId: BUNDLE });
  await push(udid, 'closed');
  await tapBanner(app, 'closed');
  await expect
    .poll(() => inFront(app.session), { message: 'the app started by the tap' })
    .toBe(BUNDLE);
  await pageAgain(app);
  await expect.poll(() => app.url()).toBe(`${ORIGIN}/groups`);
});
