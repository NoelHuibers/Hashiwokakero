// The app's client-side rendered bundle in a browser: it starts, renders pages from the server
// functions, routes on its own, and takes the language from the path. It talks to the server as
// the app: its version in a header, its session in its own storage (src/api/app.rs).
import { addBridge, board, DAILY_KEY, saved } from '../board';
import { startDaily } from '../daily';
import { expect, hydrated, test, visit } from '../fixtures';

const PLAY_STORE = 'https://play.google.com/store/apps/details?id=rs.hashi';

test('the practice board shows a generated puzzle', async ({ page }) => {
  await visit(page, '/practice');
  await expect(board(page).locator('g.island').first()).toBeVisible();
});

test('the router opens the rules without a page load', async ({ page }) => {
  await visit(page, '/');
  await page.evaluate(() => Object.assign(window, { samePage: true }));
  await page.locator('header').getByRole('link', { name: 'How to play' }).click();
  await expect(page).toHaveURL(/\/rules$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');
  expect(await page.evaluate(() => 'samePage' in window)).toBe(true);
});

test('a language from the path, also after a reload', async ({ page }) => {
  await visit(page, '/ja/rules');
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('橋をかけろの遊び方');
  // Every path is index.html, the bundle renders the page of the URL
  await page.reload();
  await hydrated(page);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('橋をかけろの遊び方');
});

test('the saved theme applies before the app starts', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('hashi.theme', 'dark'));
  await visit(page, '/rules');
  await expect(page.locator('html')).toHaveClass(/\bdark\b/);
});

test('a guest keeps their session in the app, sent in a header instead of a cookie', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const calls: { url: string; headers: Promise<Record<string, string>> }[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname.startsWith('/api/')) calls.push({ url: r.url(), headers: r.allHeaders() });
  });
  // Making a group makes a guest
  await visit(page, '/groups');
  await page.getByLabel('Group name').fill('Header Club');
  await page.getByRole('button', { name: 'Create the group' }).click();
  const ready = page.getByTestId('group-ready');
  // The invite link is the website's, not the app's own origin
  await expect(ready.getByTestId('invite-link')).toHaveValue(/^https:\/\/hashi\.rs\/g\/[a-z2-9_]{10}$/);
  // A toast after an answer: the app's contexts outlive the mount (hashi::mount)
  await ready.getByRole('button', { name: 'Copy link' }).click();
  await expect(page.getByText('Invite link copied to the clipboard.')).toBeVisible();

  const token = await page.evaluate(() => localStorage.getItem('hashi.session'));
  expect(token).toMatch(/^[0-9a-f]{64}$/);
  expect((await context.cookies()).map((c) => c.name)).not.toContain('hashi_session');
  // Every call says it is the app, the Pixel's Android with the version of app/Cargo.toml; the
  // calls after the guest was made send its token
  const sent = await Promise.all(calls.map(async (c) => ({ url: c.url, headers: await c.headers })));
  const create = sent.findIndex((c) => c.url.endsWith('/api/create_group'));
  expect(create).toBeGreaterThan(0);
  for (const { headers } of sent) expect(headers['x-hashi-app']).toMatch(/^android\/\d+\.\d+\.\d+$/);
  for (const { headers } of sent.slice(0, create + 1)) expect(headers.authorization).toBeUndefined();
  expect(sent.length).toBeGreaterThan(create + 1);
  for (const { headers } of sent.slice(create + 1)) expect(headers.authorization).toBe(`Bearer hs1.${token}`);

  // A reload keeps the guest: the group is still theirs
  await page.reload();
  await hydrated(page);
  await expect(page.locator('a[data-group]').filter({ hasText: 'Header Club' })).toContainText('Owner');
  expect(await page.evaluate(() => localStorage.getItem('hashi.session'))).toBe(token);
  expect((await context.cookies()).map((c) => c.name)).not.toContain('hashi_session');
});

test("a guest's daily starts in the app with the session the start sent, the board at once", async ({ page, context }) => {
  const calls: { path: string; headers: Promise<Record<string, string>> }[] = [];
  page.on('request', (r) => {
    const { pathname } = new URL(r.url());
    if (pathname.startsWith('/api/')) calls.push({ path: pathname, headers: r.allHeaders() });
  });
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  await addBridge(page, DAILY_KEY, puzzle, solution.findIndex((b) => b > 0));

  const token = await page.evaluate(() => localStorage.getItem('hashi.session'));
  expect(token).toMatch(/^[0-9a-f]{64}$/);
  expect((await context.cookies()).map((c) => c.name)).not.toContain('hashi_session');
  // The start's reply names the guest: nobody asks who that is or for the daily again, and the
  // calls after it send the guest's token
  const start = calls.findIndex((c) => c.path === '/api/start_daily');
  expect(start).toBeGreaterThan(0);
  const after = calls.slice(start + 1);
  expect(after.map((c) => c.path)).not.toContain('/api/current_user');
  expect(after.map((c) => c.path)).not.toContain('/api/daily_today');
  for (const c of after) expect((await c.headers).authorization).toBe(`Bearer hs1.${token}`);

  // A reload keeps the guest and their running daily
  await page.reload();
  await hydrated(page);
  await expect(board(page)).toHaveClass(/board-interactive/);
  expect((await saved(page, DAILY_KEY)).moves).toBe(1);
});

test('an app the server no longer answers asks for an update, once', async ({ page, problems }) => {
  // Every call answered like the server answers apps below its minimum version (server::apps)
  let refused = 0;
  await page.route('**/api/**', (route) => {
    refused++;
    return route.fulfill({ status: 500, contentType: 'text/plain', body: 'ServerError|E:app_outdated' });
  });
  await visit(page, '/');
  const update = page.getByRole('link', { name: 'Update', exact: true });
  await expect(update).toBeVisible();
  await expect(update).toHaveAttribute('href', PLAY_STORE);
  await expect(page.getByText('This version of the app is too old.', { exact: false })).toHaveCount(1);
  await expect(update).toHaveCount(1);
  expect(refused).toBeGreaterThan(1);
  // The refused calls (and the browser's note of each), nothing else
  const refusal = /^(500 POST .*\/api\/\w+|console error: Failed to load resource: .* status of 500 .*)$/;
  expect(problems.filter((p) => !refusal.test(p))).toEqual([]);
  problems.length = 0;
});

test('the server’s files are linked on the website', async ({ page }) => {
  await visit(page, '/dojo');
  const set = page.locator('a[download]').first();
  await expect(set).toHaveAttribute('href', /^https:\/\/hashi\.rs\/dojo-dev-\d+\.tar\.gz$/);
  await expect(set).toHaveAttribute('target', '_blank');
  await expect(page.getByRole('link', { name: '/dojo/board.json' })).toHaveAttribute('href', 'https://hashi.rs/dojo/board.json');
});

test('the header, the tab bar and the sides clear the safe areas the WebView reports', async ({ page }) => {
  // What iOS's WKWebView reports as env(safe-area-inset-*) on an iPhone 17, held upright: the
  // status bar with the Dynamic Island, the home indicator. Chromium takes them from DevTools.
  const cdp = await page.context().newCDPSession(page);
  const report = (insets: { top: number; right: number; bottom: number; left: number }) =>
    cdp.send('Emulation.setSafeAreaInsetsOverride', { insets });
  await report({ top: 62, right: 0, bottom: 34, left: 0 });
  await visit(page, '/rules');
  const header = page.locator('header');
  const tabBar = page.locator('nav').filter({ has: page.locator('.tab-label') });
  const body = page.locator('body');
  await expect(header).toHaveCSS('padding-top', '62px');
  await expect(tabBar).toHaveCSS('padding-bottom', '34px');
  await expect(body).toHaveCSS('padding-left', '0px');

  // Held sideways: the camera's side and the other one, the home indicator, no status bar
  await report({ top: 0, right: 62, bottom: 21, left: 62 });
  await expect(header).toHaveCSS('padding-top', '0px');
  await expect(tabBar).toHaveCSS('padding-bottom', '21px');
  for (const side of ['left', 'right']) {
    await expect(body).toHaveCSS(`padding-${side}`, '62px');
    await expect(tabBar).toHaveCSS(`padding-${side}`, '62px');
  }

  // Android's WebView reports none of them (env() is 0 in WebView 124); MainActivity.kt sets the
  // variables on the element, which win over env() where a newer WebView reports something
  await report({ top: 62, right: 0, bottom: 0, left: 0 });
  await page.evaluate(() => {
    const insets = { top: 24, right: 0, bottom: 48, left: 0 };
    for (const [side, px] of Object.entries(insets)) document.documentElement.style.setProperty(`--app-inset-${side}`, `${px}px`);
  });
  await expect(header).toHaveCSS('padding-top', '24px');
  await expect(tabBar).toHaveCSS('padding-bottom', '48px');
});
