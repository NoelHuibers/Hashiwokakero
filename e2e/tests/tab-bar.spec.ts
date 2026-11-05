// The tab bar at the bottom of a phone: five tabs share a 320 px screen, the players' pages; the
// Lab, for people who write solvers, is in the desktop header and linked from elsewhere on a
// phone. Every tab lies on the screen whole, a big enough target, its kanji and its translated
// name not cut off, in every language (Russian's names are the longest, Arabic is right to left).
import { readFileSync } from 'node:fs';
import path from 'node:path';

import type { Page } from '@playwright/test';

import { ROOT } from '../env.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LOCALES, localized } from '../site.ts';

const navLabel = (locale: string): string =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, 'common.json'), 'utf8')).nav.main;

/** The tabs (links) of the bar, the page in `locale` at `width`. */
async function tabsAt(page: Page, locale: string, width: number) {
  await page.setViewportSize({ width, height: 640 });
  // The groups tab is the current page: its kanji is inked, the widest of the five
  await visit(page, localized(locale, '/groups'));
  await page.evaluate(() => document.fonts.ready);
  const bar = page
    .getByRole('navigation', { name: navLabel(locale), exact: true })
    .filter({ visible: true });
  const tabs = bar.getByRole('link');
  await expect(tabs).toHaveCount(5);
  return tabs;
}

for (const locale of LOCALES) {
  test(`all five tabs fit a 320 px phone in ${locale}`, async ({ page }) => {
    const tabs = await tabsAt(page, locale, 320);
    for (const tab of await tabs.all()) {
      const parts = tab.locator(':scope > span');
      const [kanji, label] = [parts.nth(0), parts.nth(1)];
      const name = `${await kanji.textContent()} ${await label.textContent()}`;
      const box = (await tab.boundingBox())!;
      expect(box.x, `${name} starts on the screen`).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, `${name} ends on the screen`).toBeLessThanOrEqual(320);
      expect(box.height, `${name} is tall enough to tap`).toBeGreaterThanOrEqual(44);
      expect(box.width, `${name} is wide enough to tap`).toBeGreaterThanOrEqual(48);
      const kanjiBox = (await kanji.boundingBox())!;
      expect(kanjiBox.x, `the kanji of ${name} starts in its tab`).toBeGreaterThanOrEqual(box.x);
      expect(
        kanjiBox.x + kanjiBox.width,
        `the kanji of ${name} ends in its tab`,
      ).toBeLessThanOrEqual(box.x + box.width);
      for (const [part, el] of [
        ['kanji', kanji],
        ['name', label],
      ] as const) {
        expect(
          await el.evaluate((e) => e.scrollWidth <= e.clientWidth),
          `the ${part} of ${name} is whole`,
        ).toBe(true);
      }
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
}

test('the tabs share a 390 px phone evenly', async ({ page }) => {
  const tabs = await tabsAt(page, 'en', 390);
  const widths = await Promise.all(
    (await tabs.all()).map(async (tab) => (await tab.boundingBox())!.width),
  );
  for (const width of widths) expect(width).toBeCloseTo(390 / 5, 0);
});

test('the Lab is in the header on a desktop, not in the tab bar on a phone', async ({ page }) => {
  const main = page.getByRole('navigation', { name: 'Main navigation', exact: true });
  await page.setViewportSize({ width: 390, height: 800 });
  await visit(page, '/play');
  const bar = main.filter({ visible: true });
  await expect(bar.getByRole('link')).toHaveText([/Daily/, /Play/, /Ranks/, /Groups/, /Create/]);
  await expect(bar.getByRole('link', { name: /Lab/ })).toHaveCount(0);
  // But easy to find: the solver panel, the help menu and the footer lead there
  await expect(page.getByTestId('lab-link')).toHaveAttribute('href', '/lab');
  await page.locator('header').getByRole('button', { name: 'Help' }).click();
  await page.locator('[data-help-menu]').getByRole('link', { name: 'Solver lab' }).click();
  await expect(page).toHaveURL(/\/lab$/);
  await expect(page.getByRole('heading', { name: 'Solver lab', level: 1 })).toBeVisible();
  await expect(page.locator('footer').getByRole('link', { name: /^Solver lab/ })).toHaveAttribute(
    'href',
    '/lab',
  );

  await page.setViewportSize({ width: 1280, height: 800 });
  const header = page.locator('header').getByRole('navigation', { name: 'Main navigation' });
  await expect(header.getByRole('link')).toHaveCount(6);
  const lab = header.getByRole('link', { name: /Lab/ });
  await expect(lab).toBeVisible();
  // Every tab of the Lab is in its section
  for (const path of ['/lab', '/lab/leaderboard', '/lab/seasons']) {
    await visit(page, path);
    await expect(lab).toHaveAttribute('aria-current', 'page');
  }
});
