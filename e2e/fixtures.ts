// The `test` of every spec: it fails a test on any error in the browser (a panic in the WASM,
// a console error, a failed request to the site), with helpers to open pages and sign in.
import { test as base, expect, type BrowserContext, type Page } from '@playwright/test';

import { BASE_URL } from './env';
import { player } from './seed';

export { expect };

export const test = base.extend<{ problems: string[] }>({
  // Collected while the test runs and checked after it. A test that expects an error removes
  // it, see `allow`.
  problems: [
    async ({ page }, use) => {
      const problems: string[] = [];
      page.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
      page.on('console', (m) => {
        if (m.type() === 'error') problems.push(`console error: ${m.text()}`);
      });
      // Pages are checked by `visit`, this catches what they load: scripts, styles, server functions
      page.on('response', (r) => {
        if (r.url().startsWith(BASE_URL) && r.status() >= 400 && !r.request().isNavigationRequest()) {
          problems.push(`${r.status()} ${r.request().method()} ${r.url()}`);
        }
      });
      await use(problems);
      expect(problems, 'errors in the browser').toEqual([]);
    },
    { auto: true },
  ],
});

/** Removes the expected errors from the problems, failing if one of them didn't happen. */
export function allow(problems: string[], ...expected: RegExp[]) {
  for (const pattern of expected) {
    const i = problems.findIndex((p) => pattern.test(p));
    expect(i, `expected an error like ${pattern}`).toBeGreaterThanOrEqual(0);
    problems.splice(i, 1);
  }
}

/** Opens a page and waits until it has hydrated and loaded its data. */
export async function visit(page: Page, path: string, { status = 200 } = {}) {
  const response = await page.goto(path);
  expect(response?.status(), `HTTP status of ${path}`).toBe(status);
  await hydrated(page);
  return response;
}

/**
 * Waits for the mark App sets once the WASM has hydrated the page (src/app.rs), then for the
 * requests it starts (server functions, fonts) to finish.
 */
export async function hydrated(page: Page) {
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await page.waitForLoadState('networkidle');
}

/** Signs a seeded player in (see seed.ts), with their session cookie. */
export async function signIn(context: BrowserContext, name: string) {
  const session = player(name).session;
  if (!session) throw new Error(`${name} has no seeded session`);
  await context.addCookies([{ name: 'hashi_session', value: session, url: BASE_URL, httpOnly: true, sameSite: 'Lax' }]);
}
