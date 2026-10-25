// The practice page on a phone: the board and its actions above the tab bar, one row of actions
// in every language, and the rarer ones behind the "more" button.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import type { Page } from '@playwright/test';

import { board, edges, PRACTICE_KEY, saved, seedGame } from '../board';
import { ROOT } from '../env';
import { expect, test, visit } from '../fixtures';
import { LADDER, LADDER_SOLUTION } from '../puzzles';
import { localized } from '../site';

const playing = {
  puzzle: LADDER, bridges: edges(LADDER).map(() => 0), solution: LADDER_SOLUTION,
  status: 'Playing' as const, moves: 0, hints: 0,
};
const messages = (locale: string, namespace: string) =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, `${namespace}.json`), 'utf8'));

/** The game's actions under the board, by their names in `locale`. */
function actions(page: Page, locale: string) {
  const game = messages(locale, 'common').game;
  const button = (name: string | RegExp) => page.getByRole('button', { name, exact: typeof name === 'string' });
  return {
    undo: button(game.undo),
    redo: button(game.redo),
    check: button(new RegExp(`^${game.check}`)),
    hint: button(new RegExp(`^${game.hint}`)),
    more: button(messages(locale, 'practice').more),
  };
}

/** Where the screen ends for the page: the top of the phones' tab bar, or the window's bottom. */
const fold = (page: Page) =>
  page.evaluate(() => {
    const bar = document.elementFromPoint(innerWidth / 2, innerHeight - 4)?.closest('nav');
    return bar && getComputedStyle(bar).position === 'fixed' ? bar.getBoundingClientRect().top : innerHeight;
  });

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  for (const locale of ['en', 'ru']) {
    test(`the board and every game action fit above the tab bar without scrolling in ${locale}`, async ({ page }) => {
      await seedGame(page, PRACTICE_KEY, playing);
      await visit(page, localized(locale, '/practice'));
      expect(await page.evaluate(() => scrollY)).toBe(0);
      const bottom = await fold(page);
      const frame = (await board(page).boundingBox())!;
      // The title and the clock above the board took 160 px (board at 248 px) before they were
      // compacted
      expect(frame.y, 'little above the board').toBeLessThanOrEqual(215);
      expect(frame.y + frame.height).toBeLessThanOrEqual(bottom);
      const buttons = Object.values(actions(page, locale));
      const boxes = await Promise.all(buttons.map(async (b) => (await b.boundingBox())!));
      for (const box of boxes) {
        expect(box.y, 'below the board').toBeGreaterThan(frame.y + frame.height);
        expect(box.y + box.height, 'above the tab bar').toBeLessThanOrEqual(bottom);
        expect(box.height, 'a tap target').toBeGreaterThanOrEqual(44);
        expect(box.width, 'a tap target').toBeGreaterThanOrEqual(44);
        expect(box.y, 'one row').toBe(boxes[0].y);
      }
    });
  }
});

// The longest names of check and hint (ru, uk, pl) and of the difficulties (uk, nl, ja, vi), long
// German ones, Arabic from right to left
for (const locale of ['en', 'de', 'ru', 'uk', 'pl', 'nl', 'vi', 'ja', 'ar']) {
  test(`the practice page doesn't overflow 320 px and keeps its actions in one row in ${locale}`, async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 640 });
    await seedGame(page, PRACTICE_KEY, playing);
    await visit(page, localized(locale, '/practice'));
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
    const buttons = Object.values(actions(page, locale));
    const boxes = await Promise.all(buttons.map(async (b) => (await b.boundingBox())!));
    for (const [i, box] of boxes.entries()) {
      expect(box.y, 'one row').toBe(boxes[0].y);
      expect(box.x).toBeGreaterThanOrEqual(16);
      expect(box.x + box.width).toBeLessThanOrEqual(304);
      // The names are whole, not cut off
      expect(await buttons[i].evaluate((b) => b.scrollWidth <= b.clientWidth)).toBe(true);
    }
    // Undo, redo, check, hint and more from the left in English, from the right in Arabic
    const xs = boxes.map((b) => b.x);
    expect(xs).toEqual([...xs].sort((a, b) => (locale === 'ar' ? b - a : a - b)));
    // The stats and the difficulty names fit too
    for (const label of await page.locator('main .truncate').filter({ visible: true }).all()) {
      expect(await label.evaluate((l) => l.scrollWidth <= l.clientWidth), await label.innerText()).toBe(true);
    }
  });
}

test('the more button discloses restart and a new puzzle, by keyboard and pointer', async ({ page }) => {
  await seedGame(page, PRACTICE_KEY, { ...playing, bridges: LADDER_SOLUTION.map((b, e) => (e === 0 ? b : 0)), moves: 1 });
  await visit(page, '/practice');
  const { more, undo, redo, check, hint } = actions(page, 'en');
  const restart = page.getByRole('button', { name: 'Restart' });
  const fresh = page.getByRole('button', { name: /^New puzzle/ });
  // Icon-only buttons are named and have a tooltip
  for (const button of [undo, redo, more]) {
    await expect(button).toHaveAttribute('data-tooltip', /.+/);
    await expect(button).not.toHaveAttribute('title');
  }
  await expect(check).toBeVisible();
  await expect(hint).toBeVisible();
  await expect(more).toHaveAttribute('aria-expanded', 'false');
  await expect(restart).toBeHidden();
  const panel = page.locator(`#${await more.getAttribute('aria-controls')}`);
  await expect(panel).toBeHidden();

  // Keyboard: open, Tab into it, Escape closes it and gives the focus back
  await more.focus();
  await more.press('Enter');
  await expect(more).toHaveAttribute('aria-expanded', 'true');
  await expect(panel).toBeVisible();
  await expect(fresh).toContainText('Medium · 10×10');
  await more.press('Tab');
  await expect(fresh).toBeFocused();
  await fresh.press('Tab');
  await expect(restart).toBeFocused();
  await restart.press('Escape');
  await expect(more).toHaveAttribute('aria-expanded', 'false');
  await expect(more).toBeFocused();

  // A click elsewhere closes it too
  await more.click();
  await expect(restart).toBeVisible();
  await page.getByRole('heading', { level: 1 }).click();
  await expect(restart).toBeHidden();

  // Restart clears the board and closes the panel
  await more.click();
  await restart.click();
  await expect(restart).toBeHidden();
  await expect(board(page).locator('g.bridge:not(.ghost)')).toHaveCount(0);
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges.every((b) => b === 0)).toBe(true);

  // A new puzzle of the difficulty picked in the card
  await page.getByRole('group', { name: 'New puzzle' }).getByRole('button', { name: /Easy/ }).click();
  await more.click();
  await expect(fresh).toContainText('Easy · 7×7');
  await fresh.click();
  await expect(fresh).toBeHidden();
  await expect.poll(async () => JSON.stringify((await saved(page, PRACTICE_KEY)).puzzle.islands)).not.toBe(JSON.stringify(LADDER.islands));
  await expect(page.getByText(/^Easy 7×7$/)).toBeVisible();
});
