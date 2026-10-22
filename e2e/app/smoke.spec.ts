// The app's client-side rendered bundle in a browser: it starts, renders pages from the server
// functions, routes on its own, and takes the language from the path. It talks to the server as
// the app: its version in a header, its session in its own storage (src/api/app.rs).
import { board } from '../board';
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
