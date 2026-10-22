// The tab bar at the bottom of a phone: six tabs share a 320 px screen. Every tab lies on the
// screen whole, a big enough target, its kanji and its translated name not cut off, in every
// language (Russian's names are the longest, Arabic is right to left).
import { readFileSync } from 'node:fs';
import path from 'node:path';

import type { Page } from '@playwright/test';

import { ROOT } from '../env';
import { expect, test, visit } from '../fixtures';
import { localized, LOCALES } from '../site';

const navLabel = (locale: string): string =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, 'common.json'), 'utf8')).nav.main;

/** The tabs (links) of the bar, the page in `locale` at `width`. */
async function tabsAt(page: Page, locale: string, width: number) {
  await page.setViewportSize({ width, height: 640 });
  // The groups tab is the current page: its kanji is inked, the widest of the six
  await visit(page, localized(locale, '/groups'));
  await page.evaluate(() => document.fonts.ready);
  const bar = page.getByRole('navigation', { name: navLabel(locale), exact: true }).filter({ visible: true });
  const tabs = bar.getByRole('link');
  await expect(tabs).toHaveCount(6);
  return tabs;
}

for (const locale of LOCALES) {
  test(`all six tabs fit a 320 px phone in ${locale}`, async ({ page }) => {
    const tabs = await tabsAt(page, locale, 320);
    for (const tab of await tabs.all()) {
      const [kanji, label] = await tab.locator(':scope > span').all();
      const name = `${await kanji.textContent()} ${await label.textContent()}`;
      const box = (await tab.boundingBox())!;
      expect(box.x, `${name} starts on the screen`).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, `${name} ends on the screen`).toBeLessThanOrEqual(320);
      expect(box.height, `${name} is tall enough to tap`).toBeGreaterThanOrEqual(44);
      expect(box.width, `${name} is wide enough to tap`).toBeGreaterThanOrEqual(48);
      const kanjiBox = (await kanji.boundingBox())!;
      expect(kanjiBox.x, `the kanji of ${name} starts in its tab`).toBeGreaterThanOrEqual(box.x);
      expect(kanjiBox.x + kanjiBox.width, `the kanji of ${name} ends in its tab`).toBeLessThanOrEqual(box.x + box.width);
      for (const [part, el] of [['kanji', kanji], ['name', label]] as const) {
        expect(await el.evaluate((e) => e.scrollWidth <= e.clientWidth), `the ${part} of ${name} is whole`).toBe(true);
      }
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

test('the tabs share a 390 px phone evenly', async ({ page }) => {
  const tabs = await tabsAt(page, 'en', 390);
  const widths = await Promise.all((await tabs.all()).map(async (tab) => (await tab.boundingBox())!.width));
  for (const width of widths) expect(width).toBeCloseTo(390 / 6, 0);
});
