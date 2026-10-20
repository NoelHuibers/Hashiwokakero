// Every page in every language loads, hydrates and reacts, without an error in the browser:
// what a broken Leptos, wasm-bindgen or leptos_i18n update breaks first.
import type { Page } from '@playwright/test';

import { allow, expect, signIn, test, visit } from '../fixtures';
import { player } from '../seed';
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
    // The router's fallback is outside the routes of the languages: LocaleFromPath (src/app.rs)
    // takes the language from the URL, so the 404 page is in the URL's language too
    await visit(page, path, { status: 404 });
    await expectLanguage(page, locale);
    await expect(page.locator('main h1')).toBeVisible();
    // Chrome logs the status of the page itself
    allow(problems, /404/);
    await expectInteractive(page);
  });
}

// The server writes each page's <title> once (seo::title_text), the browser keeps it up to date:
// after navigating without a page load, and once a page has loaded what its title names
test('the title follows the page', async ({ page }) => {
  await visit(page, '/de');
  await expect(page).toHaveTitle('Hashi · Tägliches Hashiwokakero-Brückenrätsel, gratis');
  // Gone after a page load
  await page.evaluate(() => Object.assign(window, { samePage: true }));
  await page.locator('header').getByRole('link', { name: 'Spielanleitung' }).click();
  await expect(page).toHaveURL(/\/de\/rules$/);
  await expect(page).toHaveTitle('Hashiwokakero spielen · Regeln und Lösungstipps · Hashi');
  expect(await page.evaluate(() => 'samePage' in window)).toBe(true);
  // The server doesn't know the player's name, the browser loads it
  await visit(page, `/player/${player('aiko').id}`);
  await expect(page).toHaveTitle('aiko · Hashi');
});
