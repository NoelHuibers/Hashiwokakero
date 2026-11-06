// Play on a phone: the board, its actions and the controls above it (difficulty chips, pause,
// a new puzzle) on the screen, one row of each in every language, and restart behind the
// "more" button.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import type { Page } from '@playwright/test';

import { board, edges, PLAY_KEY, saved, seedGame } from '../board.ts';
import { ROOT } from '../env.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION } from '../puzzles.ts';
import { localized } from '../site.ts';

const playing = {
  puzzle: LADDER,
  bridges: edges(LADDER).map(() => 0),
  solution: LADDER_SOLUTION,
  status: 'Playing' as const,
  moves: 0,
  hints: 0,
};
/**
 * The lowest the board's top may be on a 390 px phone, under the heading, the title with pause
 * and a new puzzle, the clock and the difficulty chips (282 px with a badge under the title).
 */
const BOARD_TOP = 285;
const messages = (locale: string, namespace: string) =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, `${namespace}.json`), 'utf8'));

/** The game's actions under the board, by their names in `locale`. */
function actions(page: Page, locale: string) {
  const game = messages(locale, 'common').game;
  const button = (name: string | RegExp) =>
    page.getByRole('button', { name, exact: typeof name === 'string' });
  return {
    undo: button(game.undo),
    redo: button(game.redo),
    check: button(new RegExp(`^${game.check}`)),
    hint: button(new RegExp(`^${game.hint}`)),
    more: button(messages(locale, 'play').more),
  };
}

/** The controls above the board: pause, a new puzzle and the five difficulty chips. */
function controls(page: Page, locale: string) {
  const play = messages(locale, 'play');
  const chips = page.getByRole('group', { name: play.new_puzzle }).getByRole('button');
  return {
    pause: page.getByRole('button', { name: play.pause, exact: true }),
    fresh: page.getByRole('button', { name: play.new_puzzle, exact: true }),
    chips,
  };
}

/** Where the screen ends for the page: the top of the phones' tab bar, or the window's bottom. */
const fold = (page: Page) =>
  page.evaluate(() => {
    const bar = document.elementFromPoint(innerWidth / 2, innerHeight - 4)?.closest('nav');
    return bar && getComputedStyle(bar).position === 'fixed'
      ? bar.getBoundingClientRect().top
      : innerHeight;
  });

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  for (const locale of ['en', 'ru']) {
    test(`the board and every game action fit above the tab bar without scrolling in ${locale}`, async ({
      page,
    }) => {
      await seedGame(page, PLAY_KEY, playing);
      await visit(page, localized(locale, '/play'));
      expect(await page.evaluate(() => scrollY)).toBe(0);
      const bottom = await fold(page);
      const frame = (await board(page).boundingBox())!;
      // The title and the clock above the board took 160 px (board at 248 px) before they were
      // compacted; the difficulty chips came with one row more
      expect(frame.y, 'little above the board').toBeLessThanOrEqual(BOARD_TOP);
      // Pause and a new puzzle next to the title, the chips right above the board
      const { pause, fresh, chips } = controls(page, locale);
      const above = [pause, fresh, ...(await chips.all())];
      expect(above).toHaveLength(7);
      for (const control of above) {
        const box = (await control.boundingBox())!;
        expect(box.y + box.height, 'above the board').toBeLessThanOrEqual(frame.y);
        expect(box.y, 'below the header').toBeGreaterThan(0);
        expect(box.height, 'a tap target').toBeGreaterThanOrEqual(44);
      }
      expect(frame.y + frame.height).toBeLessThanOrEqual(bottom);
      const buttons = Object.values(actions(page, locale));
      const boxes = await Promise.all(buttons.map(async (b) => (await b.boundingBox())!));
      for (const box of boxes) {
        expect(box.y, 'below the board').toBeGreaterThan(frame.y + frame.height);
        expect(box.y + box.height, 'above the tab bar').toBeLessThanOrEqual(bottom);
        expect(box.height, 'a tap target').toBeGreaterThanOrEqual(44);
        expect(box.width, 'a tap target').toBeGreaterThanOrEqual(44);
        expect(box.y, 'one row').toBe(boxes[0]!.y);
      }
    });
  }
});

// The longest names of check and hint (ru, uk, pl) and of the difficulties (uk, nl, ja, vi), long
// German ones, Arabic from right to left
for (const locale of ['en', 'de', 'ru', 'uk', 'pl', 'nl', 'vi', 'ja', 'ar']) {
  test(`Play doesn't overflow 320 px and keeps its actions in one row in ${locale}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 320, height: 640 });
    await seedGame(page, PLAY_KEY, playing);
    await visit(page, localized(locale, '/play'));
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      320,
    );
    const buttons = Object.values(actions(page, locale));
    const boxes = await Promise.all(buttons.map(async (b) => (await b.boundingBox())!));
    for (const [i, box] of boxes.entries()) {
      expect(box.y, 'one row').toBe(boxes[0]!.y);
      expect(box.x).toBeGreaterThanOrEqual(16);
      expect(box.x + box.width).toBeLessThanOrEqual(304);
      // The names are whole, not cut off
      expect(await buttons[i]!.evaluate((b) => b.scrollWidth <= b.clientWidth)).toBe(true);
    }
    // Undo, redo, check, hint and more from the left in English, from the right in Arabic
    const xs = boxes.map((b) => b.x);
    expect(xs).toEqual([...xs].sort((a, b) => (locale === 'ar' ? b - a : a - b)));
    // The five chips in one row as well, pause and a new puzzle on the screen
    const { pause, fresh, chips } = controls(page, locale);
    const chipBoxes = await Promise.all(
      (await chips.all()).map(async (c) => (await c.boundingBox())!),
    );
    expect(chipBoxes).toHaveLength(5);
    for (const box of [...chipBoxes, (await pause.boundingBox())!, (await fresh.boundingBox())!]) {
      expect(box.x).toBeGreaterThanOrEqual(16);
      expect(box.x + box.width).toBeLessThanOrEqual(304);
    }
    for (const box of chipBoxes) expect(box.y, 'one row of chips').toBe(chipBoxes[0]!.y);
    // The stats and the difficulty names fit too
    for (const label of await page.locator('main .truncate').filter({ visible: true }).all()) {
      expect(
        await label.evaluate((l) => l.scrollWidth <= l.clientWidth),
        await label.innerText(),
      ).toBe(true);
    }
  });
}

test('the more button discloses restart, by keyboard and pointer', async ({ page }) => {
  await seedGame(page, PLAY_KEY, {
    ...playing,
    bridges: LADDER_SOLUTION.map((b, e) => (e === 0 ? b : 0)),
    moves: 1,
  });
  await visit(page, '/play');
  const { more, undo, redo, check, hint } = actions(page, 'en');
  const restart = page.getByRole('button', { name: 'Restart' });
  // Icon-only buttons are named and have a tooltip
  const { pause, fresh } = controls(page, 'en');
  for (const button of [undo, redo, more, pause, fresh]) {
    await expect(button).toHaveAttribute('data-tooltip', /.+/);
    await expect(button).not.toHaveAttribute('title');
  }
  await expect(check).toBeVisible();
  await expect(hint).toBeVisible();
  await expect(more).toHaveAttribute('aria-expanded', 'false');
  await expect(restart).toBeHidden();
  const panel = page.locator(`#${await more.getAttribute('aria-controls')}`);
  await expect(panel).toBeHidden();
  // A new puzzle is above the board, not in there too
  await expect(panel.locator('button')).toHaveCount(1);

  // Keyboard: open, Tab into it, Escape closes it and gives the focus back
  await more.focus();
  await more.press('Enter');
  await expect(more).toHaveAttribute('aria-expanded', 'true');
  await expect(panel).toBeVisible();
  await more.press('Tab');
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
  await expect
    .poll(async () => (await saved(page, PLAY_KEY)).bridges.every((b) => b === 0))
    .toBe(true);
});
