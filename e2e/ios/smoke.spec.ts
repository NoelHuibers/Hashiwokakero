// The debug build on an iOS simulator with hashi.rs blocked: it starts in its WKWebView, every
// page keeps to the content security policy of tauri.conf.json, practice runs on the phone
// through the app's commands, the pages that need the server say once that the app is offline,
// and links out of the app and other sites' pages open in Safari. Like e2e/android/smoke.spec.ts,
// as far as iOS has it (no back button): the header and the tab bar clear the safe areas, upright
// and sideways, and the app's own plugin answers (HashiPlugin.swift). Nothing here may reach
// hashi.rs: no guest, no daily, no group.
import { expect, inFront, type App, test } from './app';
import { BUNDLE, ORIGIN, SAFARI } from './device';
import { NATIVE } from './webdriver';

/** The toasts on the screen, by their kind's mark (src/components/layout/toaster.rs). */
const TOASTS = '[role=log] > div';
const errorToasts = (app: App) => app.count(TOASTS, '✕');
const offlineNotices = (app: App) => app.count(TOASTS, "You're offline. Practice still works.");

/** The first board of the app (e2e/board.ts), its islands and its bridges. */
const islands = (app: App) => app.evaluate(() => document.querySelector('svg.board-svg')?.querySelectorAll('g.island').length ?? 0);
const bridges = (app: App) => app.evaluate(() => document.querySelector('svg.board-svg')?.querySelectorAll('g.bridge:not(.ghost)').length ?? 0);

/** Where the islands are, to tell one puzzle from another. */
const layout = (app: App) =>
  app.evaluate(() =>
    [...(document.querySelector('svg.board-svg')?.querySelectorAll('g.island') ?? [])]
      .map((g) => `${g.getAttribute('transform')}:${g.textContent}`)
      .join(' '),
  );

const shows = (app: App, text: string) => app.evaluate((t) => document.body.innerText.includes(t), text);

/** The practice board with a puzzle, done growing, its fonts loaded. */
async function practiceBoard(app: App) {
  await expect.poll(() => islands(app), { message: 'the islands' }).toBeGreaterThan(0);
  await expect.poll(() => shows(app, 'Growing islands'), { message: 'the generator done' }).toBe(false);
  await app.evaluate(() => document.fonts.ready.then(() => null));
}

test('the app starts in its WebView, offline', async ({ app }) => {
  // The start page, `tauri://localhost` without a path
  expect((await app.url()).replace(/\/$/, '')).toBe(ORIGIN);
  // Tauri's bridge, which the practice takes for its commands (src/api/native.rs)
  expect(await app.evaluate(() => typeof (window as any).__TAURI_INTERNALS__?.invoke)).toBe('function');
  // What makes the app send `x-hashi-app: ios/<version>` (src/api/app.rs)
  expect(await app.evaluate(() => navigator.userAgent)).toContain('iPhone');
  // The simulator has the Mac's network: only hashi.rs is out of reach, the WebView is online
  test.info().annotations.push({ type: 'navigator.onLine', description: String(await app.evaluate(() => navigator.onLine)) });
  // The daily needs the server
  await expect.poll(() => shows(app, "Couldn't load today's puzzle. Try again.")).toBe(true);
  expect(await errorToasts(app)).toBe(0);
  expect(await offlineNotices(app)).toBeLessThanOrEqual(1);
  await app.screenshot('start.png');
});

const PAGES = ['/', '/rules', '/ja/rules', '/practice', '/create', '/lab', '/dojo', '/groups', '/daily'];

test('no page breaks the content security policy', async ({ app }) => {
  const found: Record<string, string[]> = {};
  for (const path of PAGES) {
    await app.visit(path);
    // Late work too: fonts, the board's first drawing
    await app.evaluate(() => document.fonts.ready.then(() => null));
    await new Promise((resolve) => setTimeout(resolve, 500));
    const seen = (await app.records())?.csp;
    expect(seen, `the listener for violations is on ${path}`).toBeDefined();
    if (seen!.length) found[path] = seen!;
  }
  // The console's CSP messages, from the documents' start on: the `problems` fixture
  expect(found, 'securitypolicyviolation events').toEqual({});
});

test('practice generates, hints and checks on the phone', async ({ app }) => {
  await app.visit('/practice');
  // The first visit generates a puzzle by itself, without the server
  await practiceBoard(app);
  await app.screenshot('practice.png');

  await app.click({ css: 'button', name: 'Hint' });
  await expect.poll(() => bridges(app), { message: 'the hint' }).toBe(1);
  await app.click({ css: 'button', name: 'Check' });
  await expect.poll(() => app.count(TOASTS, 'So far so good')).toBe(1);

  // Another one on request
  const first = await layout(app);
  await app.click({ css: 'button', name: /^Generate/ });
  await expect.poll(() => layout(app), { message: 'a new puzzle' }).not.toBe(first);
  await expect.poll(() => shows(app, 'Growing islands')).toBe(false);
  expect(await bridges(app)).toBe(0);

  // Tauri's IPC is a fetch of `ipc://localhost/<command>` on iOS: the generator ran on the phone
  const calls = (await app.records())!.calls;
  test.info().annotations.push({ type: 'calls', description: calls.join(' ') });
  expect(calls.filter((c) => c.startsWith('ipc://localhost/generate_puzzle')), 'the native generator').toHaveLength(1);
  expect(calls.filter((c) => /\/api\/(generate_puzzle|solve_puzzle|puzzle_cnf)/.test(c)), 'server functions').toEqual([]);
  expect(await errorToasts(app)).toBe(0);
});

test('pages that need the server say once that the app is offline', async ({ app }) => {
  // The ranks make several calls at once, all of them fail
  await app.visit('/ranks');
  await expect.poll(() => offlineNotices(app)).toBe(1);
  expect(await errorToasts(app)).toBe(0);
  // Not again on the next page while offline, once it went away
  await expect.poll(() => offlineNotices(app)).toBe(0);
  await app.click({ css: 'a', name: /Groups/ });
  await expect.poll(() => app.url()).toMatch(/\/groups$/);
  await app.idle();
  expect(await app.count(TOASTS)).toBe(0);
  // Once the network is back (the window's `online` event), the next call that fails says it again
  await app.evaluate(() => void window.dispatchEvent(new Event('online')));
  await app.click({ css: 'a', name: /Ranks/ });
  await expect.poll(() => app.url()).toMatch(/\/ranks$/);
  await expect.poll(() => offlineNotices(app)).toBe(1);
});

/** Safari in front, then the app again: the app is where it was. */
async function openedInSafari(app: App, what: string) {
  // Safari's first start on a busy runner took 50 s once
  await expect.poll(() => inFront(app.session), { message: `Safari in front for ${what}`, timeout: 90_000 }).toBe(SAFARI);
  await app.session.mobile('activateApp', { bundleId: BUNDLE });
  await expect.poll(() => inFront(app.session), { message: 'the app in front again' }).toBe(BUNDLE);
  expect(await app.url()).toBe(`${ORIGIN}/rules`);
  expect(await app.text('h1')).toBe('How to play Hashiwokakero');
}

test('links out of the app open in Safari', async ({ app }) => {
  // Waits for Safari twice
  test.slow();
  await app.visit('/rules');
  await app.click({ css: 'footer a[href="https://github.com/NoelHuibers/Hashiwokakero"]' });
  await openedInSafari(app, 'the link');

  // A page of another site the WebView would go to by itself: refused, and opened outside too
  await app.evaluate(() => void setTimeout(() => (location.href = 'https://example.com/'), 0));
  await openedInSafari(app, 'the address');
});

/**
 * The safe areas WKWebView reports (`env(safe-area-inset-*)`, which app/index.html passes on as
 * `--app-inset-*`) and where the header, the tab bar's labels and the body's sides are, in CSS px.
 */
const geometry = (app: App) =>
  app.evaluate(() => {
    const probe = document.createElement('div');
    probe.style.cssText =
      'position:fixed;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)';
    document.body.append(probe);
    const style = getComputedStyle(probe);
    const env = { top: style.paddingTop, right: style.paddingRight, bottom: style.paddingBottom, left: style.paddingLeft };
    probe.remove();
    const body = getComputedStyle(document.body);
    const labels = [...document.querySelectorAll('.tab-label')].filter((l) => l.getClientRects().length).map((l) => l.getBoundingClientRect());
    return {
      env: Object.fromEntries(Object.entries(env).map(([side, px]) => [side, parseFloat(px)])) as Record<'top' | 'right' | 'bottom' | 'left', number>,
      header: document.querySelector('header > div')!.getBoundingClientRect().top,
      labels: labels.map((r) => ({ left: r.left, right: r.right, bottom: r.bottom })),
      body: { left: parseFloat(body.paddingLeft), right: parseFloat(body.paddingRight) },
      width: innerWidth,
      height: innerHeight,
    };
  });

/** Turns the simulator (WebDriver's orientation, a command of the native context). */
async function turn(app: App, orientation: 'LANDSCAPE' | 'PORTRAIT') {
  const context = await app.session.context();
  await app.session.setContext(NATIVE);
  try {
    await app.session.command('POST', '/orientation', { orientation });
  } finally {
    await app.session.setContext(context);
  }
}

// The header below the status bar and the Dynamic Island, the tab bar above the home indicator
// (step 13c). The status bar's icons follow the page's theme, not the phone's (light here): the
// screenshots show them.
for (const theme of ['light', 'dark'] as const) {
  test(`the practice board clears the safe areas, ${theme}`, async ({ app }) => {
    await app.evaluate((t) => localStorage.setItem('hashi.theme', t), theme);
    await app.visit('/practice');
    await practiceBoard(app);
    await app.evaluate(() => window.scrollTo(0, 0));
    await expect.poll(() => app.count(TOASTS)).toBe(0);
    await app.screenshot(`practice-${theme}.png`);
    const at = await geometry(app);
    test.info().annotations.push({ type: 'insets', description: `env(safe-area-inset-*) ${JSON.stringify(at.env)}, header at ${at.header} of ${at.height} px` });
    expect(at.env.top, 'the status bar').toBeGreaterThan(0);
    expect(at.env.bottom, 'the home indicator').toBeGreaterThan(0);
    expect(at.header, 'the header below the status bar').toBeGreaterThanOrEqual(at.env.top);
    expect(at.labels, 'the tab bar').toHaveLength(6);
    for (const label of at.labels) expect(label.bottom, 'a tab above the home indicator').toBeLessThanOrEqual(at.height - at.env.bottom + 0.5);
  });
}

test('turned sideways, the page clears the Dynamic Island and the home indicator', async ({ app }) => {
  await app.visit('/practice');
  await practiceBoard(app);
  await turn(app, 'LANDSCAPE');
  try {
    await expect.poll(() => app.evaluate(() => innerWidth > innerHeight), { message: 'the page sideways' }).toBe(true);
    // UIKit reports the landscape insets only on the scene lifecycle (Info.plist): without it the
    // portrait ones stayed, the top's and none at the sides
    await expect.poll(async () => (await geometry(app)).env.top, { message: 'no status bar sideways' }).toBe(0);
    const at = await geometry(app);
    test.info().annotations.push({ type: 'insets sideways', description: JSON.stringify(at.env) });
    expect(Math.min(at.env.left, at.env.right), 'the sides').toBeGreaterThan(0);
    expect(at.body, "the body's sides").toEqual({ left: at.env.left, right: at.env.right });
    expect(at.labels, 'the tab bar').toHaveLength(6);
    for (const label of at.labels) {
      expect(label.left, 'a tab right of the Dynamic Island').toBeGreaterThanOrEqual(at.env.left - 0.5);
      expect(label.right, 'a tab left of the other side').toBeLessThanOrEqual(at.width - at.env.right + 0.5);
      expect(label.bottom, 'a tab above the home indicator').toBeLessThanOrEqual(at.height - at.env.bottom + 0.5);
    }
    await app.screenshot('landscape.png');
  } finally {
    await turn(app, 'PORTRAIT');
  }
});

test("the app's own plugin answers on iOS", async ({ app }) => {
  // The commands of api::native: the status bar for the page's theme, and the share sheet with a
  // file, as the CNF's download opens it. Without the Swift side Tauri refuses them ("Plugin hashi
  // not initialized"), and the page only logs a warning.
  const invoke = (command: string, args: Record<string, unknown>) =>
    app.evaluate(
      ({ command, args }) =>
        (window as any).__TAURI_INTERNALS__.invoke(command, args).then(
          () => 'ok',
          (e: unknown) => `refused: ${String(e)}`,
        ),
      { command, args },
    );
  expect(await invoke('plugin:hashi|system_bars', { dark: true })).toBe('ok');
  expect(await invoke('plugin:hashi|system_bars', { dark: false })).toBe('ok');
  expect(await invoke('plugin:hashi|share', { text: 'p cnf 1 1\n1 0\n', file: 'hashi.cnf', mime: 'text/plain' })).toBe('ok');
  // The sheet over the app, Save to Files among its actions
  await new Promise((resolve) => setTimeout(resolve, 1500));
  await app.screenshot('share.png');
});
