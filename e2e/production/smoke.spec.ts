// The deployed site's main pages, a few of them in other languages (one right to left): each
// answers 200, hydrates and reacts, loads its scripts, styles and fonts, and logs no error. The
// build it serves (`x-hashi-build`, the hash of its WASM) is the one the page loads, and the one
// CI just deployed when SMOKE_BUILD names it.
//
// Read-only: production's database is the real one. A visitor without a session becomes a guest
// (a row in users, a session) only through the server functions that call `auth::ensure_user`, the
// SESSION_MAKERS of src/api/app.rs; nothing here starts a puzzle, posts a form or signs in. To be
// sure, every call of a server function that writes is stopped in the browser and fails the test,
// and so does a session cookie in any answer.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { test as base, expect, type BrowserContext, type Page } from '@playwright/test';

/** The pages, and the language each is in. */
const PAGES: [string, string][] = [
  ['/', 'en'],
  ['/practice', 'en'],
  ['/ranks', 'en'],
  ['/rules', 'en'],
  ['/dojo', 'en'],
  ['/impressum', 'en'],
  ['/datenschutz', 'en'],
  ['/de', 'de'],
  ['/ja/rules', 'ja'],
  ['/ar', 'ar'],
];
const RTL = new Set(['ar']);

/** The server functions that make a guest or sign in, read from the source like the app does. */
const SESSION_MAKERS: string[] = (() => {
  const source = readFileSync(path.join(import.meta.dirname, '../../src/api/app.rs'), 'utf8');
  const list = source.match(/const SESSION_MAKERS: \[&str; \d+\] = \[([^\]]*)\]/);
  if (!list) throw new Error('SESSION_MAKERS not found in src/api/app.rs');
  return [...list[1].matchAll(/"(\w+)"/g)].map((m) => m[1]);
})();
/** Every other server function that changes something, by the verb its name starts with. */
const WRITES =
  /^(start|submit|set|create|join|leave|send|post|delete|update|rotate|remove|withdraw|change|logout|open|seen|admin)_|^daily_(hint|check|reveal|paid_action|recover_actions)$|^(login|register|logout)$/;

/** Whether a call of this URL may write to the database. */
function writes(url: string) {
  const name = new URL(url).pathname.match(/\/api\/(\w+)/)?.[1];
  return name !== undefined && (SESSION_MAKERS.includes(name) || WRITES.test(name));
}

/** Stops every server function call that writes, before it leaves the browser. */
async function guard(context: BrowserContext, problems: string[]) {
  await context.route('**/api/**', async (route) => {
    if (writes(route.request().url())) {
      problems.push(`blocked a call that writes: ${route.request().method()} ${route.request().url()}`);
      await route.abort('blockedbyclient');
    } else {
      await route.fallback();
    }
  });
}

const test = base.extend<{ problems: string[] }>({
  // Collected while the test runs and checked after it, like the browser tests' (e2e/fixtures.ts)
  problems: [
    async ({ page, context }, use) => {
      const problems: string[] = [];
      await guard(context, problems);
      page.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
      // The guard's own aborts are already in the list, the browser logs them too
      const blocked = (text = '') => text.includes('ERR_BLOCKED_BY_CLIENT');
      page.on('console', (m) => {
        if (m.type() === 'error' && !blocked(m.text())) problems.push(`console error: ${m.text()}`);
      });
      page.on('requestfailed', (r) => {
        if (!blocked(r.failure()?.errorText)) {
          problems.push(`failed: ${r.method()} ${r.url()} (${r.failure()?.errorText})`);
        }
      });
      page.on('response', async (r) => {
        if (r.status() >= 400) problems.push(`${r.status()} ${r.request().method()} ${r.url()}`);
        // A new session: as a cookie for the website, in a header for the app (src/server/auth.rs)
        const cookie = await r.headerValue('set-cookie').catch(() => null);
        const header = await r.headerValue('x-hashi-session').catch(() => null);
        if (cookie?.includes('hashi_session') || header) problems.push(`a new session from ${r.url()}`);
      });
      await use(problems);
      expect(problems, 'errors in the browser').toEqual([]);
      expect((await context.cookies()).map((c) => c.name), 'cookies').not.toContain('hashi_session');
    },
    { auto: true },
  ],
});

/** Opens a page and waits until it has hydrated and loaded what it loads. */
async function visit(page: Page, url: string) {
  const response = await page.goto(url);
  expect(response?.status(), `HTTP status of ${url}`).toBe(200);
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await page.waitForLoadState('networkidle');
  return response!;
}

for (const [url, locale] of PAGES) {
  test(`${url} works`, async ({ page }) => {
    const response = await visit(page, url);
    const html = page.locator('html');
    await expect(html).toHaveAttribute('lang', locale);
    await expect(html).toHaveAttribute('dir', RTL.has(locale) ? 'rtl' : 'ltr');
    await expect(page.locator('main')).not.toBeEmpty();

    // The build the server names is the one the page loads, and the one just deployed
    const build = await response.headerValue('x-hashi-build');
    expect(build, 'x-hashi-build').toMatch(/^[\w-]{22}$/);
    await expect(page.locator('link[rel=preload][as=fetch]')).toHaveAttribute('href', `/pkg/hashi.${build}.wasm`);
    if (process.env.SMOKE_BUILD) expect(build, 'the deployed build').toBe(process.env.SMOKE_BUILD);

    // The fonts the page uses loaded, none failed
    const fonts = await page.evaluate(async () => {
      await document.fonts.ready;
      const faces = [...document.fonts];
      return {
        loaded: faces.filter((f) => f.status === 'loaded').length,
        failed: faces.filter((f) => f.status === 'error').map((f) => f.family),
      };
    });
    expect(fonts.failed, 'fonts that failed to load').toEqual([]);
    expect(fonts.loaded, 'fonts loaded').toBeGreaterThan(0);

    // Only works once hydrated: the 昼/夜 button in the header switches to night mode (kept in
    // the browser, nothing is sent)
    await expect(html).not.toHaveClass(/\bdark\b/);
    await page.locator('header button').filter({ hasText: /^[昼夜]$/ }).click();
    await expect(html).toHaveClass(/\bdark\b/);
  });
}

// The guard itself, on a made-up origin: what it lets through goes nowhere
test('the guard stops the calls that write', async ({ page, problems }) => {
  expect(SESSION_MAKERS).toContain('start_daily');
  for (const name of [...SESSION_MAKERS, 'submit_daily', 'daily_hint', 'send_message', 'set_hanko']) {
    expect(writes(`https://hashi.rs/api/${name}`), name).toBe(true);
  }
  for (const name of ['daily_today', 'current_user', 'generate_puzzle', 'load_banzuke', 'month_leaderboard']) {
    expect(writes(`https://hashi.rs/api/${name}`), name).toBe(false);
  }

  const origin = 'https://smoke.invalid';
  await page.route(`${origin}/`, (route) => route.fulfill({ contentType: 'text/html', body: '<p>smoke</p>' }));
  await page.goto(`${origin}/`);
  const sent = await page.evaluate(() =>
    fetch('/api/start_daily', { method: 'POST', body: '{}' }).then(
      () => true,
      () => false,
    ),
  );
  expect(sent).toBe(false);
  expect(problems).toEqual([`blocked a call that writes: POST ${origin}/api/start_daily`]);
  // Expected
  problems.length = 0;
});
