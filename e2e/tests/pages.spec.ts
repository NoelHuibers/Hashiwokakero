// Every page in every language loads, hydrates and reacts, without an error in the browser:
// what a broken Leptos, wasm-bindgen or leptos_i18n update breaks first.
import type { Page } from '@playwright/test';

import { allow, expect, signIn, test, visit } from '../fixtures';
import { LOCALES, localized, ROUTES, RTL_LOCALES } from '../site';

async function expectLanguage(page: Page, locale: string) {
  const html = page.locator('html');
  await expect(html).toHaveAttribute('lang', locale);
  await expect(html).toHaveAttribute('dir', RTL_LOCALES.has(locale) ? 'rtl' : 'ltr');
}

/** Only works once hydrated: the 昼/夜 button in the header switches to night mode. */
async function expectInteractive(page: Page) {
  const html = page.locator('html');
  await expect(html).not.toHaveClass(/\bdark\b/);
  await page.locator('header button').filter({ hasText: /^[昼夜]$/ }).click();
  await expect(html).toHaveClass(/\bdark\b/);
}

for (const [who, name] of [
  ['guest', null],
  ['admin', 'admin'],
] as const) {
  test.describe(who, () => {
    if (name) test.beforeEach(({ context }) => signIn(context, name));
    for (const locale of LOCALES) {
      for (const route of ROUTES) {
        const path = localized(locale, route);
        test(`${path} works`, async ({ page }) => {
          await visit(page, path);
          await expectLanguage(page, locale);
          await expect(page.locator('main')).not.toBeEmpty();
          await expectInteractive(page);
        });
      }
    }
  });
}

test('the admin dashboard is only for admins', async ({ page, context }) => {
  await visit(page, '/admin');
  await expect(page.getByRole('heading', { name: 'Lost at sea' })).toBeVisible();
  await signIn(context, 'admin');
  await visit(page, '/admin');
  await expect(page.getByRole('heading', { name: 'Admin', exact: true })).toBeVisible();
});

for (const locale of [LOCALES[0], 'de', 'ar']) {
  const path = localized(locale, '/no-such-page');
  test(`${path} is a 404 page`, async ({ page, problems }) => {
    // The router's fallback is outside the routes of the languages, so the 404 page is in the
    // browser's language, not the URL's
    test.fixme(locale !== LOCALES[0], '404 pages ignore the language of the URL');
    await visit(page, path, { status: 404 });
    await expectLanguage(page, locale);
    await expect(page.locator('main h1')).toBeVisible();
    // Chrome logs the status of the page itself
    allow(problems, /404/);
    await expectInteractive(page);
  });
}
