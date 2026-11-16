// Notifications without a push service (docs/notifications.md, "The Android app"): the app asks a
// server itself, in the background (Notifications.kt), and shows what it answers. The server here
// is a fake one on this computer that adb lends the device (`adb reverse`, which airplane mode
// doesn't stop): the device stays offline and nothing reaches hashi.rs. WorkManager waits for a
// network the device doesn't have, so the tests run its jobs themselves (`cmd jobscheduler run
// -f`, which skips the constraints), as Android would once online. The page's side
// (`state::notify::follow`) needs the real server and isn't tested here; the plugin's commands
// are what it calls.
import { execFileSync } from 'node:child_process';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';

import type { AndroidDevice, Page } from '@playwright/test';

import type { TauriWindow } from '../tauri.ts';
import { expect, test } from './app.ts';
import { ORIGIN, PKG, shell } from './device.ts';

const SECRET = 'ab'.repeat(32);
const DEVICE = 7;
const APP = 'android/9.9.9';

/** A call of a server function, as the fake server got it. */
interface Call {
  name: string;
  body: Record<string, unknown>;
  app: string | undefined;
}

/** A server that answers the server function `name` with the JSON of `answers[name]`. */
async function fakeServer(answers: Record<string, unknown>) {
  const calls: Call[] = [];
  const server: Server = createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => {
      body += chunk;
    });
    req.on('end', () => {
      const name = (req.url ?? '').replace(/^\/api\//, '');
      calls.push({
        name,
        body: body ? JSON.parse(body) : {},
        app: req.headers['x-hashi-app'] as string | undefined,
      });
      if (req.method !== 'POST' || !(name in answers)) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(answers[name]));
    });
  });
  await new Promise<void>((done) => {
    server.listen(0, '127.0.0.1', done);
  });
  const port = (server.address() as AddressInfo).port;
  // The device's 127.0.0.1:<port> is this one's
  execFileSync('adb', ['reverse', `tcp:${port}`, `tcp:${port}`]);
  return {
    url: `http://127.0.0.1:${port}`,
    answers,
    calls,
    async close() {
      execFileSync('adb', ['reverse', '--remove', `tcp:${port}`]);
      await new Promise((done) => {
        server.close(done);
      });
    },
  };
}

/** A plugin command of the app, with its answer or its refusal. */
const invoke = (page: Page, command: string, args: Record<string, unknown> = {}) =>
  page.evaluate(
    ([command, args]) =>
      (window as TauriWindow).__TAURI_INTERNALS__.invoke(command, args).then(
        (answer: unknown) => ({ answer: answer ?? null }),
        (e: unknown) => ({ refused: typeof e === 'string' ? e : JSON.stringify(e) }),
      ),
    [command, args] as const,
  );

/**
 * The app's WorkManager jobs that JobScheduler has, from `dumpsys jobscheduler`: WorkManager
 * keeps them in a namespace of its own (`JOB androidx.work.systemjobscheduler:u0a209/1: …
 * rs.hashi.debug/androidx.work.impl.background.systemjob.SystemJobService`), which
 * `cmd jobscheduler run` needs as well. The lines go to the report of a failed test.
 */
async function jobs(device: AndroidDevice) {
  const service = `${PKG}/androidx.work.impl.background.systemjob.SystemJobService`;
  const lines = (await shell(device, 'dumpsys jobscheduler'))
    .split('\n')
    .filter((l) => l.includes(service) && /^\s*JOB /.test(l));
  jobLines = lines;
  const found = lines
    .map((l) => l.match(/JOB #?(?:(\S+):)?u\d+a\d+\/(\d+):/))
    .filter((m): m is RegExpMatchArray => m !== null)
    .map(([, namespace, id]) => `${namespace ? `-n ${namespace} ` : ''}${PKG} ${id}`);
  return [...new Set(found)];
}

/** What `jobs` saw last, attached to a failed test. */
let jobLines: string[] = [];

test.afterEach(async ({ device }, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus) return;
  const dump = await shell(device, 'dumpsys jobscheduler');
  const app = dump.split('\n').filter((l) => l.includes('hashi'));
  await testInfo.attach('jobs', {
    body: [`parsed from: ${jobLines.join('\n')}`, ...app].join('\n'),
    contentType: 'text/plain',
  });
});

/** Runs the app's jobs now, whatever their constraints (no network here). */
async function runJobs(device: AndroidDevice) {
  for (const job of await jobs(device)) {
    await shell(device, `cmd jobscheduler run -f ${job}`);
  }
}

/** The app's notifications on the device, with their extras. */
const notifications = async (device: AndroidDevice) =>
  (await shell(device, 'dumpsys notification --noredact'))
    .split(/\n\s*NotificationRecord\(/)
    .filter((record) => record.includes(`pkg=${PKG} `));

/** The app's alarm for the streak's last call (Notifications.kt). */
const lastCallAlarm = async (device: AndroidDevice) =>
  (await shell(device, 'dumpsys alarm')).includes('rs.hashi.action.LAST_CALL');

const RACE = {
  id: 12,
  kind: 'race',
  title: 'Ada opened a race',
  body: 'In Lunch club: join the lobby before it starts.',
  url: 'https://hashi.rs/groups/1/races/7',
  tag: 'race:group:1',
  expires_at: Date.now() + 3_600_000,
};

test('the app asks the server itself, shows what it answers, and stops when it forgets the phone', async ({
  page,
  device,
}) => {
  // In three hours: the alarm goes off halfway there first
  const lastCall = Date.now() + 3 * 3_600_000;
  const server = await fakeServer({
    poll_notifications: { messages: [RACE], next_last_call: lastCall },
  });
  try {
    // A fresh install has no leave yet (Android 13 and newer)
    expect(await invoke(page, 'plugin:hashi|notify_state')).toEqual({
      answer: { granted: false },
    });
    await shell(device, `pm grant ${PKG} android.permission.POST_NOTIFICATIONS`);
    expect(await invoke(page, 'plugin:hashi|notify_permission')).toEqual({
      answer: { granted: true },
    });
    expect(
      await invoke(page, 'plugin:hashi|notify_start', {
        server: server.url,
        app: APP,
        device: DEVICE,
        secret: SECRET,
      }),
    ).toEqual({ answer: { granted: true, device: DEVICE } });

    // The periodic poll and the one right away, waiting for a network
    await expect.poll(async () => (await jobs(device)).length, { message: 'the jobs' }).toBe(2);
    await runJobs(device);
    await expect.poll(() => server.calls.length, { message: 'the polls' }).toBeGreaterThan(0);
    expect(server.calls[0]).toEqual({
      name: 'poll_notifications',
      body: { device: DEVICE, secret: SECRET },
      app: APP,
    });

    // Shown in its kind's channel, by its tag
    await expect
      .poll(async () => (await notifications(device)).join('\n'), { message: 'the notification' })
      .toContain(RACE.title);
    const shown = (await notifications(device)).join('\n');
    await test.info().attach('notification', { body: shown, contentType: 'text/plain' });
    expect(shown).toContain(`tag=${RACE.tag}`);
    expect(shown).toMatch(/channel=race\b/);
    expect(shown).toContain(RACE.body);
    expect(await lastCallAlarm(device), 'the last call alarm').toBe(true);

    // A tap opens the race's page in the app
    await shell(device, 'cmd statusbar expand-notifications');
    await tapText(device, RACE.title);
    await expect(page).toHaveURL(`${ORIGIN}/groups/1/races/7`);
    await expect.poll(async () => (await notifications(device)).length).toBe(0);

    // `null`: the server forgot the phone (the account deleted, say), the app stops asking
    // WorkManager skips a run of the periodic job before its period is up: a start again asks
    // right away, with a one-time job
    server.answers.poll_notifications = null;
    await invoke(page, 'plugin:hashi|notify_start', {
      server: server.url,
      app: APP,
      device: DEVICE,
      secret: SECRET,
    });
    await runJobs(device);
    await expect
      .poll(() => invoke(page, 'plugin:hashi|notify_state'), { message: 'forgotten' })
      .toEqual({ answer: { granted: true } });
    await expect.poll(async () => (await jobs(device)).length, { message: 'no jobs' }).toBe(0);
    expect(await lastCallAlarm(device), 'the last call alarm').toBe(false);
    expect(server.calls.every((c) => c.name === 'poll_notifications')).toBe(true);
  } finally {
    await server.close();
  }
});

test('turned off, the app tells the server to forget the phone', async ({ page, device }) => {
  const server = await fakeServer({
    poll_notifications: { messages: [], next_last_call: null },
    unregister_notification_device: null,
  });
  try {
    await shell(device, `pm grant ${PKG} android.permission.POST_NOTIFICATIONS`);
    await invoke(page, 'plugin:hashi|notify_start', {
      server: server.url,
      app: APP,
      device: DEVICE,
      secret: SECRET,
    });
    expect(await invoke(page, 'plugin:hashi|notify_stop')).toEqual({
      answer: { granted: true },
    });
    // Only the job that tells the server is left
    await expect.poll(async () => (await jobs(device)).length, { message: 'the jobs' }).toBe(1);
    await runJobs(device);
    await expect
      .poll(() => server.calls.filter((c) => c.name === 'unregister_notification_device'))
      .toEqual([
        {
          name: 'unregister_notification_device',
          body: { id: DEVICE, secret: SECRET },
          app: APP,
        },
      ]);
    expect(server.calls.filter((c) => c.name === 'poll_notifications')).toEqual([]);
  } finally {
    await server.close();
  }
});

/** Taps the view with `text` on the screen (the notification shade's, say). */
async function tapText(device: AndroidDevice, text: string) {
  let bounds: number[] = [];
  await expect
    .poll(
      async () => {
        await shell(device, 'uiautomator dump /sdcard/window.xml');
        const xml = await shell(device, 'cat /sdcard/window.xml');
        const node = xml
          .split('<node ')
          .find((n) => n.includes(`text="${text}"`) && n.includes('bounds='));
        bounds =
          node
            ?.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/)
            ?.slice(1)
            .map(Number) ?? [];
        return bounds.length === 4;
      },
      { message: `"${text}" on the screen` },
    )
    .toBe(true);
  const [left = 0, top = 0, right = 0, bottom = 0] = bounds;
  await shell(
    device,
    `input tap ${Math.round((left + right) / 2)} ${Math.round((top + bottom) / 2)}`,
  );
}
