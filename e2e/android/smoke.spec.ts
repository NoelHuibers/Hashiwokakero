// The debug APK on a device without a network: it starts, every page keeps to the content
// security policy of tauri.conf.json, practice runs on the phone through the app's commands,
// the pages that need the server say once that the app is offline, and Android's back button
// goes back in the app. What only a phone shows: the header and the tab bar clear the system
// bars, whose icons follow the page's theme, the app starts without a white frame, links out
// of the app open outside it, and sharing opens Android's share sheet. Nothing here may reach
// hashi.rs: no guest, no daily, no group.
import path from 'node:path';

import type { AndroidDevice, Page } from '@playwright/test';

import { board, PRACTICE_KEY } from '../board';
import { LADDER, LADDER_SOLUTION } from '../puzzles';
import { closeSystemDialogs, expect, focused, hydrated, inFront, started, test, violations, visit } from './app';
import { ACTIVITY, keyboardTop, ORIGIN, PKG, SCREENS, screen, shell, systemBars } from './device';

/** The toasts on the screen, by their kind's mark (src/components/layout/toaster.rs). */
const toasts = (page: Page) => page.locator('[role=log] > div');
const errorToasts = (page: Page) => toasts(page).filter({ hasText: '✕' });
const offlineNotice = (page: Page) => toasts(page).filter({ hasText: "You're offline. Practice still works." });

const islands = (page: Page) => board(page).locator('g.island');
const bridges = (page: Page) => board(page).locator('g.bridge:not(.ghost)');

/** Where the islands are, to tell one puzzle from another. */
const layout = (page: Page) =>
  islands(page).evaluateAll((all) => all.map((g) => `${g.getAttribute('transform')}:${g.textContent}`).join(' '));

/** The server functions the page called (all of them failed, the device is offline). */
function apiCalls(page: Page) {
  const calls: string[] = [];
  page.on('request', (r) => {
    const url = new URL(r.url());
    if (url.pathname.startsWith('/api/')) calls.push(url.pathname);
  });
  return calls;
}

test('the app starts in its WebView, offline', async ({ page }) => {
  expect(new URL(page.url()).origin).toBe(ORIGIN);
  // Tauri's bridge, which the practice takes for its commands (src/api/native.rs)
  expect(await page.evaluate(() => typeof (window as any).__TAURI_INTERNALS__?.invoke)).toBe('function');
  // Without the permission ACCESS_NETWORK_STATE the WebView would take itself for online, always
  expect(await page.evaluate(() => navigator.onLine), 'navigator.onLine').toBe(false);
  // The daily needs the server
  await expect(page.getByText("Couldn't load today's puzzle. Try again.")).toBeVisible();
  await expect(errorToasts(page)).toHaveCount(0);
  expect(await offlineNotice(page).count()).toBeLessThanOrEqual(1);
});

const PAGES = ['/', '/rules', '/ja/rules', '/practice', '/create', '/lab', '/dojo', '/groups', '/daily'];

test('no page breaks the content security policy', async ({ page, problems }) => {
  const found: Record<string, string[]> = {};
  const consoleCsp: string[] = [];
  page.on('console', (m) => {
    if (/Content.Security.Policy/i.test(m.text())) consoleCsp.push(`${new URL(page.url()).pathname}: ${m.text()}`);
  });
  for (const path of PAGES) {
    await visit(page, path);
    // Late work too: fonts, the board's first drawing
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(500);
    const seen = await violations(page);
    if (seen.length) found[path] = seen;
  }
  expect(found, 'securitypolicyviolation events').toEqual({});
  expect(consoleCsp, 'CSP messages on the console').toEqual([]);
  // Reported above already, page by page
  problems.splice(0, problems.length, ...problems.filter((p) => !p.startsWith('CSP:')));
});

test('practice generates, hints and checks on the phone', async ({ page, device }) => {
  const calls = apiCalls(page);
  await visit(page, '/practice');
  // The first visit generates a puzzle by itself, without the server: the device is offline
  await expect(islands(page).first()).toBeVisible();
  await expect(page.getByText('Growing islands')).toBeHidden();
  await page.evaluate(() => document.fonts.ready);
  await device.screenshot({ path: path.join(SCREENS, 'practice.png') });

  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  await expect(bridges(page)).toHaveCount(1);
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await expect(toasts(page).filter({ hasText: 'So far so good' })).toBeVisible();

  // Another one on request
  const first = await layout(page);
  await page.getByRole('button', { name: /^Generate/ }).click();
  await expect.poll(() => layout(page), { message: 'a new puzzle' }).not.toBe(first);
  await expect(page.getByText('Growing islands')).toBeHidden();
  await expect(bridges(page)).toHaveCount(0);

  expect(calls.filter((p) => /generate_puzzle|solve_puzzle|puzzle_cnf/.test(p))).toEqual([]);
  await expect(errorToasts(page)).toHaveCount(0);
});

test('pages that need the server say once that the app is offline', async ({ page }) => {
  // The ranks make several calls at once, all of them fail
  await visit(page, '/ranks');
  await expect(offlineNotice(page)).toHaveCount(1);
  await expect(errorToasts(page)).toHaveCount(0);
  // Not again on the next page while offline, once it went away
  await expect(offlineNotice(page)).toHaveCount(0);
  await page.getByRole('link', { name: /Groups/ }).filter({ visible: true }).first().click();
  await expect(page).toHaveURL(/\/groups$/);
  await hydrated(page);
  await expect(toasts(page)).toHaveCount(0);
  // Once the network is back (the window's `online` event), the next call that fails says it again.
  // The real event would need a network, which the device must not have.
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  await page.getByRole('link', { name: /Ranks/ }).filter({ visible: true }).first().click();
  await expect(page).toHaveURL(/\/ranks$/);
  await expect(offlineNotice(page)).toHaveCount(1);
});

test("Android's back button goes back in the app", async ({ page, device }) => {
  await visit(page, '/rules');
  await page.locator('a[href="/practice"]').filter({ visible: true }).first().click();
  await expect(page).toHaveURL(/\/practice$/);
  await expect(islands(page).first()).toBeVisible();

  await closeSystemDialogs(device);
  await shell(device, 'input keyevent KEYCODE_BACK');
  // Gives the app the time to go back, or Android to close it
  await page.waitForTimeout(1500);
  expect(page.isClosed() || !(await inFront(device)), 'the app closed').toBe(false);
  await expect(page).toHaveURL(/\/rules$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');
});

/** The middle of the darkest and of the brightest pixels in rows `from` to `to` of the screen. */
async function extremes(device: AndroidDevice, from: number, to: number) {
  const shot = await screen(device);
  const values: number[] = [];
  for (let y = Math.max(0, from); y < Math.min(shot.height, to); y += 2) {
    for (let x = 0; x < shot.width; x += 2) values.push(shot.luminance(x, y));
  }
  values.sort((a, b) => a - b);
  const at = (q: number) => values[Math.floor(q * (values.length - 1))];
  return { darkest: at(0.002), median: at(0.5), brightest: at(0.998) };
}

for (const theme of ['light', 'dark'] as const) {
  test(`the header and the tab bar clear the system bars, ${theme}`, async ({ page, device }) => {
    // The page's theme, whatever the phone's
    await page.evaluate((t) => localStorage.setItem('hashi.theme', t), theme);
    await visit(page, '/practice');
    await expect(islands(page).first()).toBeVisible();
    await expect(page.getByText('Growing islands')).toBeHidden();
    await page.evaluate(() => document.fonts.ready);
    // The board scrolled to the top, the toasts gone
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(toasts(page)).toHaveCount(0);
    await closeSystemDialogs(device);
    await page.waitForTimeout(500);
    await device.screenshot({ path: path.join(SCREENS, `practice-${theme}.png`) });

    const bars = await systemBars(device);
    const page_ = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      const probe = document.createElement('div');
      probe.style.cssText = 'position:fixed;padding:env(safe-area-inset-top) 0 env(safe-area-inset-bottom)';
      document.body.append(probe);
      const env = `${getComputedStyle(probe).paddingTop} ${getComputedStyle(probe).paddingBottom}`;
      probe.remove();
      return {
        dpr: devicePixelRatio,
        height: innerHeight,
        width: innerWidth,
        env,
        app: ['top', 'bottom'].map((side) => style.getPropertyValue(`--app-inset-${side}`)).join(' '),
      };
    });
    test.info().annotations.push(
      { type: 'system bars', description: `top ${bars.top} px, bottom ${bars.bottom} px` },
      { type: 'insets', description: `env(safe-area-inset-top/bottom) ${page_.env}, --app-inset-top/bottom ${page_.app}` },
    );
    await test.info().attach('dumpsys window', { body: bars.source, contentType: 'text/plain' });
    const shot = await screen(device);
    // The page fills the screen, under the bars too (edge to edge)
    expect(Math.abs(page_.height * page_.dpr - shot.height), 'the page is as high as the screen').toBeLessThan(3);

    // In CSS pixels, half a pixel for the rounding
    const statusBar = bars.top / page_.dpr;
    const tabBarEnd = (shot.height - bars.bottom) / page_.dpr;
    const header = await page.locator('header > div').first().boundingBox();
    expect(header, 'the header').not.toBeNull();
    expect(header!.y, 'the header below the status bar').toBeGreaterThanOrEqual(statusBar - 0.5);
    const labels = await page.locator('.tab-label').evaluateAll((all) =>
      all.filter((l) => l.getClientRects().length).map((l) => l.getBoundingClientRect().bottom),
    );
    expect(labels.length, 'the tab bar').toBe(6);
    for (const bottom of labels) expect(bottom, 'a tab above the navigation bar').toBeLessThanOrEqual(tabBarEnd + 0.5);

    // The bars show the page behind them, and their icons have to stand out: dark ones on the
    // light page, light ones on the dark page
    for (const [bar, from, to] of [
      ['status bar', 0, bars.top],
      ['navigation bar', shot.height - bars.bottom, shot.height],
    ] as const) {
      const { darkest, median, brightest } = await extremes(device, from, to);
      test.info().annotations.push({ type: bar, description: `luminance ${darkest.toFixed(2)} / ${median.toFixed(2)} / ${brightest.toFixed(2)}` });
      // Android's dark icons are a grey (about 0.37 on the light page), its light ones white
      if (theme === 'light') {
        expect(median, `the light page behind the ${bar}`).toBeGreaterThan(0.6);
        expect(median - darkest, `dark icons on the ${bar}`).toBeGreaterThan(0.4);
      } else {
        expect(median, `the dark page behind the ${bar}`).toBeLessThan(0.3);
        expect(brightest - median, `light icons on the ${bar}`).toBeGreaterThan(0.4);
      }
    }
  });
}

test('the app starts in dark mode without a white frame', async ({ page, device }) => {
  // The phone's dark mode, a fresh start: before the page paints, the window's background shows
  await shell(device, 'cmd uimode night yes');
  try {
    await shell(device, `am force-stop ${PKG}`);
    // The test's page goes with it
    await page.waitForEvent('close', { timeout: 10_000 }).catch(() => {});
    await expect.poll(() => shell(device, 'cmd uimode night')).toMatch(/Night mode: yes/);
    // From the app's first frame on (-W waits for it), not the screen before
    await shell(device, 'input keyevent KEYCODE_HOME');
    await shell(device, `am start -W -n ${ACTIVITY}`);
    // A row in the middle and one at a quarter of the screen, their middle brightness: the text
    // on them doesn't count
    const row = (shot: Awaited<ReturnType<typeof screen>>, y: number) => {
      const values = Array.from({ length: shot.width >> 2 }, (_, i) => shot.luminance(i * 4, y)).sort((a, b) => a - b);
      return values[values.length >> 1];
    };
    const samples: number[] = [];
    const end = Date.now() + 6000;
    while (Date.now() < end) {
      const shot = await screen(device);
      samples.push(row(shot, shot.height >> 1), row(shot, shot.height >> 2));
    }
    test.info().annotations.push({ type: 'luminance', description: samples.map((l) => l.toFixed(2)).join(' ') });
    expect(samples.length).toBeGreaterThan(4);
    expect(Math.max(...samples), 'the brightest moment').toBeLessThan(0.5);
  } finally {
    await shell(device, 'cmd uimode night no');
    await shell(device, `am force-stop ${PKG}`);
  }
});

/** Brings the app back in front of what it opened. */
async function backToTheApp(device: AndroidDevice) {
  await shell(device, `am start -n ${ACTIVITY}`);
  await expect.poll(() => inFront(device), { message: 'the app in front' }).toBe(true);
}

test('links out of the app open outside it', async ({ page, device }) => {
  await visit(page, '/rules');
  const link = page.locator('footer a[href="https://github.com/NoelHuibers/Hashiwokakero"]');
  await link.scrollIntoViewIfNeeded();
  await shell(device, 'logcat -c');
  await link.click();
  // Android opens the browser (offline here), the app's WebView stays on its page
  await expect.poll(() => started(device), { message: 'a VIEW intent for the link' }).toContainEqual(
    expect.stringMatching(/act=android\.intent\.action\.VIEW dat=https:\/\/github\.com/),
  );
  expect(page.url()).toBe(`${ORIGIN}/rules`);
  await backToTheApp(device);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');

  // A page of another site the WebView would go to by itself: refused, and opened outside too.
  // Last: Playwright keeps waiting for the navigation the app refused
  await shell(device, 'logcat -c');
  await page.evaluate(() => {
    location.href = 'https://example.com/';
  });
  await expect.poll(() => started(device), { message: 'a VIEW intent for the address' }).toContainEqual(
    expect.stringMatching(/act=android\.intent\.action\.VIEW dat=https:\/\/example\.com/),
  );
  await page.waitForTimeout(1000);
  expect(page.url()).toBe(`${ORIGIN}/rules`);
  await backToTheApp(device);
});

test("sharing opens Android's share sheet, copying works", async ({ page, device }) => {
  // A practice puzzle solved: its win screen has the share bar
  await page.evaluate(
    ([key, game]) => {
      localStorage.setItem(key, game);
      localStorage.setItem('hashi.practice.solver.open', 'true');
    },
    [
      PRACTICE_KEY,
      JSON.stringify({ puzzle: LADDER, bridges: LADDER_SOLUTION, solution: LADDER_SOLUTION, status: 'Won', moves: 9, hints: 0, unique: null, source: 'Custom', elapsed: 42 }),
    ],
  );
  await visit(page, '/practice');
  await expect(page.getByRole('heading', { name: /Well done!/ })).toBeVisible();

  // The clipboard of the WebView itself, on a tap
  await page.getByRole('button', { name: 'Copy', exact: true }).click();
  await expect(toasts(page).filter({ hasText: 'Result copied to the clipboard.' })).toBeVisible();

  const chooser = async (what: string) => {
    await expect.poll(() => started(device), { message: `the share sheet for ${what}` }).toContainEqual(
      expect.stringMatching(/act=android\.intent\.action\.CHOOSER/),
    );
    await expect.poll(() => inFront(device), { message: 'the share sheet in front' }).toBe(false);
    // Its window has the keys only a moment after it is in front: the back button would go to the
    // app's WebView behind it until then
    await expect.poll(() => focused(device), { message: 'the share sheet has the keys' }).toMatch(/intentresolver|Chooser/i);
    await shell(device, 'input keyevent KEYCODE_BACK');
    await expect.poll(() => inFront(device), { message: 'the app in front again' }).toBe(true);
  };
  await shell(device, 'logcat -c');
  await page.getByRole('button', { name: 'Share result', exact: true }).click();
  await chooser('the result');

  // The CNF, which the website downloads, as a file to share
  await page.getByRole('button', { name: 'Admire', exact: true }).click();
  await shell(device, 'logcat -c');
  await page.getByRole('button', { name: 'CNF ↓' }).click();
  await chooser('the CNF');
  const cnf = await shell(device, `run-as ${PKG} cat cache/share/hashi.cnf`);
  expect(cnf).toMatch(/^p cnf \d+ \d+/m);
  expect(page.url()).toBe(`${ORIGIN}/practice`);
});

test('the focused input stays above the soft keyboard', async ({ page, device }) => {
  // The soft keyboard, also where the emulator has a hardware one
  await shell(device, 'settings put secure show_ime_with_hard_keyboard 1');
  await visit(page, '/kontakt');
  const input = page.locator('main textarea').first();
  await expect(input).toBeVisible();
  // At the bottom of the screen, just above the tab bar: where the keyboard comes up
  const { dpr, screenHeight } = await input.evaluate((el) => {
    const bar = document.querySelector('nav.fixed')!.getBoundingClientRect().top;
    window.scrollBy(0, el.getBoundingClientRect().bottom - (bar - 8));
    return { dpr: devicePixelRatio, screenHeight: innerHeight * devicePixelRatio };
  });
  const box = (await input.boundingBox())!;
  test.info().annotations.push({ type: 'input before', description: `${Math.round(box.y * dpr)}–${Math.round((box.y + box.height) * dpr)} px` });
  expect((box.y + box.height) * dpr, 'the input in the lower half of the screen').toBeGreaterThan(screenHeight / 2);

  // A tap, as a finger does: only that brings the keyboard up
  await closeSystemDialogs(device);
  await shell(device, `input tap ${Math.round((box.x + box.width / 2) * dpr)} ${Math.round((box.y + box.height - 12) * dpr)}`);
  await expect.poll(() => keyboardTop(device), { message: 'the keyboard is up' }).not.toBeNull();
  const top = (await keyboardTop(device))!;
  expect(top, 'the keyboard covers the input where it was').toBeLessThan((box.y + box.height) * dpr);
  expect(await input.evaluate((el) => el === document.activeElement), 'the input has the focus').toBe(true);
  // The page ends above the keyboard, and the input scrolled into view
  await expect.poll(() => page.evaluate(() => innerHeight * devicePixelRatio), { message: 'the page ends at the keyboard' }).toBeLessThanOrEqual(top + 1);
  await expect
    .poll(async () => {
      const b = (await input.boundingBox())!;
      return (b.y + b.height) * dpr;
    }, { message: 'the input above the keyboard' })
    .toBeLessThanOrEqual(top);
  const after = (await input.boundingBox())!;
  test.info().annotations.push(
    { type: 'keyboard', description: `top ${top} px of ${screenHeight} px` },
    { type: 'input with the keyboard', description: `${Math.round(after.y * dpr)}–${Math.round((after.y + after.height) * dpr)} px` },
  );
  expect(after.y, 'the input below the header').toBeGreaterThan(0);
  await page.waitForTimeout(500);
  await device.screenshot({ path: path.join(SCREENS, 'keyboard.png') });

  // Typing works, and the keyboard goes away again with the back button: the page is whole again
  await shell(device, 'input text hello');
  await expect(input).toHaveValue(/hello$/);
  await shell(device, 'input keyevent KEYCODE_BACK');
  await expect.poll(() => keyboardTop(device), { message: 'the keyboard is down' }).toBeNull();
  await expect.poll(() => page.evaluate(() => innerHeight * devicePixelRatio), { message: 'the page as high as the screen' }).toBeGreaterThan(screenHeight - 3);
  expect(page.url()).toBe(`${ORIGIN}/kontakt`);
});
