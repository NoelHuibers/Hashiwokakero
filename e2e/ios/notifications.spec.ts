// Notifications through Apple's push service (docs/notifications.md, "The iOS app"), on the
// simulator: iOS's permission and the plugin's commands (HashiPlugin.swift), a notification that
// comes while the app is in front shows as a banner, and a tap on one opens its page in the app,
// in front and from a closed app (main.mm). The notifications come from `xcrun simctl push`, as
// APNs would hand them to the phone; the server's side (registering, sending) needs hashi.rs and
// Apple, it has its own tests (server/notify/apns). Nothing here reaches hashi.rs. In the order of
// the file: iOS asks once, the taps need the permission.
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import type { TauriWindow } from '../tauri.ts';
import { type App, expect, inFront, test } from './app.ts';
import { BUNDLE, ORIGIN, simctl } from './device.ts';
import { NATIVE, type Session, WebDriverError } from './webdriver.ts';

/** SpringBoard: the home screen, the banners and iOS's alerts. */
const SPRINGBOARD = 'com.apple.springboard';
/** The page a test notification opens: an App Link (seo::app_link) that works offline. */
const LINK = 'https://hashi.rs/groups';

test.describe.configure({ mode: 'serial' });

/** A plugin command's answer, or its refusal. */
type Outcome = { answer: unknown } | { refused: string };
/** The outcomes of the commands `start` sent, in the page. */
type Outcomes = { e2eOutcomes?: Record<number, Outcome>; e2eSent?: number };

/**
 * Sends a plugin command of the app in the page and returns at once, with the number to find its
 * outcome by (`outcome`). The script doesn't wait for the command: while a script of the page runs
 * for more than a second, Appium's XCUITest driver asks WebDriverAgent for an alert twice a second,
 * and every ask reads the app's accessibility tree. On iOS 27's simulator that kept the app busy
 * (Apple Intelligence's checks in the app) and the first `notify_state` took 73 s to over 120 s;
 * an alert that shows meanwhile fails the script ("unexpected alert open").
 */
const start = (app: App, command: string) =>
  app.evaluate((command) => {
    const w = window as unknown as Outcomes;
    w.e2eOutcomes ??= {};
    w.e2eSent = (w.e2eSent ?? 0) + 1;
    const outcomes = w.e2eOutcomes;
    const id = w.e2eSent;
    (window as TauriWindow).__TAURI_INTERNALS__.invoke(command, {}).then(
      (answer: unknown) => {
        outcomes[id] = { answer: answer ?? null };
      },
      (e: unknown) => {
        outcomes[id] = { refused: typeof e === 'string' ? e : JSON.stringify(e) };
      },
    );
    return id;
  }, command);

/** The outcome of the command `start` sent as `id`, once there is one. */
async function outcome(app: App, id: number, command: string, timeout = 45_000) {
  const started = Date.now();
  let found: Outcome | null = null;
  await expect
    .poll(
      async () => {
        found = await app.evaluate(
          (id) => (window as unknown as Outcomes).e2eOutcomes?.[id] ?? null,
          id,
        );
        return found;
      },
      { message: `the answer to ${command}`, timeout },
    )
    .not.toBeNull();
  test.info().annotations.push({ type: command, description: `${Date.now() - started} ms` });
  return found as unknown as Outcome;
}

/** A plugin command of the app, with its answer or its refusal. */
const invoke = async (app: App, command: string, timeout?: number) =>
  outcome(app, await start(app, command), command, timeout);

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

/** Clicks the first native element with `marker` in its label; `false` without one, or when it went. */
async function tapLabelled(session: Session, marker: string) {
  const found = await element(session, `label CONTAINS ${JSON.stringify(marker)}`);
  if (!found) return false;
  try {
    await session.command('POST', `/element/${found}/click`);
    return true;
  } catch (e) {
    // A banner that went between the lookup and the click
    if (e instanceof WebDriverError && e.error === 'stale element reference') return false;
    throw e;
  }
}

/**
 * Sends a notification with `marker` (`push`) and taps it: its banner while it shows (about five
 * seconds), else its entry in the Notification Centre, where it stays. A lookup in SpringBoard
 * takes a snapshot of its views, seconds on a busy runner, and so does a click on what it found,
 * and `simctl push` took 24 s once: the banner may be gone before the tap. The search starts with
 * the push; how the notification was tapped and when goes into the report.
 */
async function pushAndTap(app: App, udid: string, marker: string) {
  const started = Date.now();
  const pushed: { at?: number } = {};
  const sending = push(udid, marker).finally(() => {
    pushed.at = Date.now();
  });
  sending.catch(() => {
    // Thrown by the `await` below; a failed push ends the banner's search like a finished one
  });
  const how = await onSpringBoard(app.session, async () => {
    while (pushed.at === undefined || Date.now() - pushed.at < 15_000) {
      if (await tapLabelled(app.session, marker)) return 'banner';
      await sleep(250);
    }
    await sending;
    // The Notification Centre: a swipe down from the top left corner (the right one opens the
    // Control Centre)
    const screen = await app.session.command<{ width: number; height: number }>(
      'GET',
      '/window/rect',
    );
    await app.session.mobile('dragFromToForDuration', {
      duration: 0.5,
      fromX: screen.width * 0.2,
      fromY: 2,
      toX: screen.width * 0.2,
      toY: screen.height * 0.6,
    });
    const shown = Date.now();
    while (Date.now() - shown < 30_000) {
      if (await tapLabelled(app.session, marker)) return 'Notification Centre';
      await sleep(500);
    }
    return null;
  });
  await sending;
  test.info().annotations.push({
    type: 'notification',
    description: `${how ? `tapped in the ${how}` : 'not found'} after ${Date.now() - started} ms (pushed after ${pushed.at! - started} ms)`,
  });
  if (!how) {
    await test.info().attach('springboard', {
      body: await springBoardSource(app.session),
      contentType: 'text/xml',
    });
  }
  expect(how, `the notification ${marker}`).not.toBeNull();
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

/** The Allow button of iOS's notification alert, if it shows. */
const allowButton = (session: Session) =>
  element(session, "type == 'XCUIElementTypeButton' AND label == 'Allow'");

// A try that failed with iOS's alert still open: answered, so that the app's end doesn't answer it.
// iOS takes an app that ends while it asks as a no, and its retry would find notifications refused.
test.afterEach(async ({ app }, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus) return;
  await onSpringBoard(app.session, async () => {
    const allow = await allowButton(app.session);
    if (allow) await app.session.command('POST', `/element/${allow}/click`);
  }).catch(() => {
    // Nothing to answer, or the app is gone already
  });
});

test("iOS asks once, and the plugin gives the push service's token", async ({ app }) => {
  // The first question after the install may still take a while (see `start`)
  test.slow();
  // Not yet asked: not allowed. iOS keeps the answer, not the app: a retry finds the first try's
  // (see `afterEach`).
  const before = await invoke(app, 'plugin:hashi|notify_state', 180_000);
  if (test.info().retry === 0) expect(before).toEqual({ answer: { granted: false } });
  // The page's tap: iOS's alert, which waits for the player (not on a retry after an Allow). Nothing
  // runs in the page until it is answered: a script that runs while an alert shows fails
  // ("unexpected alert open").
  const permission = await start(app, 'plugin:hashi|notify_permission');
  if (!('answer' in before && (before.answer as { granted: boolean }).granted)) {
    const started = Date.now();
    const allowed = await onSpringBoard(app.session, async () => {
      while (Date.now() - started < 60_000) {
        const allow = await allowButton(app.session);
        if (allow) {
          await app.session.command('POST', `/element/${allow}/click`);
          return true;
        }
        await sleep(250);
      }
      return false;
    });
    test.info().annotations.push({
      type: 'alert',
      description: `${allowed ? 'allowed' : 'not found'} after ${Date.now() - started} ms`,
    });
    if (!allowed)
      await test.info().attach('springboard', {
        body: await springBoardSource(app.session),
        contentType: 'text/xml',
      });
    expect(allowed, "iOS's alert with Allow").toBe(true);
  }
  expect(await outcome(app, permission, 'plugin:hashi|notify_permission')).toEqual({
    answer: { granted: true },
  });
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
  await pushAndTap(app, udid, 'in-front');
  // The same page, its router on the link's page (api::native::follow_app_links)
  await expect.poll(() => app.url()).toBe(`${ORIGIN}/groups`);
  expect(await inFront(app.session)).toBe(BUNDLE);
  await app.screenshot('notification-tapped.png');
});

test('a tap on a notification starts the app on its page', async ({ app, udid }) => {
  // SpringBoard's lookups and, below, a start of the app
  test.slow();
  await app.session.mobile('terminateApp', { bundleId: BUNDLE });
  await pushAndTap(app, udid, 'closed');
  await expect
    .poll(() => inFront(app.session), { message: 'the app started by the tap' })
    .toBe(BUNDLE);
  await pageAgain(app);
  await expect.poll(() => app.url()).toBe(`${ORIGIN}/groups`);
});
