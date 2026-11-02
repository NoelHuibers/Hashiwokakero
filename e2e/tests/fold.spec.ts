// A game on the iPhone Duo, folded and unfolded while it runs: the page goes from the outer
// display (a compact phone) to the inner one (regular width and height, like an iPad) and back,
// in portrait and in landscape. Nothing reloads, the bridges and the clock go on, and the board
// fits each screen without the page running out sideways.
//
// The displays in points, from the iPhone Duo's device type in Xcode 27.1's simulator (iPhone19,4,
// capabilities.plist in /Library/Developer/CoreSimulator/Profiles/DeviceTypes/iPhone
// Duo.simdevicetype, read on GitHub's xcode-27 runner image): the outer display ("primary") is
// 1398 × 2034 px at scale 3, the inner one ("primary-1", landscape by nature) 2007 × 2853 px at
// scale 3. Apple's specs page (apple.com/iphone-duo/specs) has the same outer pixels and
// 1878 × 2670 for the inner panel: the 2007 × 2853 rendered at 3x, scaled down 1.07 times, as on
// the Plus iPhones. Apple publishes no point sizes.
import type { Page } from '@playwright/test';

import { addBridge, board, DAILY_KEY, edges, PRACTICE_KEY, saved, seedGame } from '../board.ts';
import { startDaily } from '../daily.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION, ladderRow } from '../puzzles.ts';

const OUTER = { width: 1398 / 3, height: 2034 / 3 };
const INNER_PORTRAIT = { width: 2007 / 3, height: 2853 / 3 };
const INNER_LANDSCAPE = { width: INNER_PORTRAIT.height, height: INNER_PORTRAIT.width };

/** Unfolded in portrait, turned sideways, folded again. */
const SIZES = [
  ['inner display, portrait', INNER_PORTRAIT],
  ['inner display, landscape', INNER_LANDSCAPE],
  ['outer display', OUTER],
] as const;

test.use({ viewport: OUTER, isMobile: true, hasTouch: true });

/** The game's clock in seconds, from "01:23" (or "1:01:23"). */
async function clock(page: Page) {
  const value = page
    .getByText(/^Time( · 時間)?$/)
    .first()
    .locator('xpath=preceding-sibling::span[1]');
  const text = (await value.textContent())!.trim();
  return text.split(':').reduce((total, part) => total * 60 + Number(part), 0);
}

/**
 * The page at the screen's size: nothing wider than the screen, the board inside it, and the
 * whole board on the screen at once, between the sticky header and the phones' tab bar.
 */
async function fits(page: Page, size: { width: number; height: number }, name: string) {
  expect(
    await page.evaluate(() => [innerWidth, innerHeight]),
    `${name}: the window's size`,
  ).toEqual([size.width, size.height]);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
    `${name}: nothing out sideways`,
  ).toBeLessThanOrEqual(size.width);
  await board(page).scrollIntoViewIfNeeded();
  const { top, bottom } = await page.evaluate(() => {
    const header = document.querySelector('header')!.getBoundingClientRect().bottom;
    const bar = document.elementFromPoint(innerWidth / 2, innerHeight - 4)?.closest('nav');
    const end =
      bar && getComputedStyle(bar).position === 'fixed'
        ? bar.getBoundingClientRect().top
        : innerHeight;
    return { top: header, bottom: end };
  });
  const frame = (await board(page).boundingBox())!;
  expect(frame.x, `${name}: the board starts on the screen`).toBeGreaterThanOrEqual(0);
  expect(frame.x + frame.width, `${name}: the board ends on the screen`).toBeLessThanOrEqual(
    size.width,
  );
  expect(frame.height, `${name}: the board fits between the bars`).toBeLessThanOrEqual(
    bottom - top,
  );
  // Scrolled to, it lies there whole: not under the header nor under the tab bar
  await page.evaluate(([y, top]) => window.scrollBy(0, y - top), [frame.y, top] as const);
  const shown = (await board(page).boundingBox())!;
  expect(shown.y, `${name}: the board's top below the header`).toBeGreaterThanOrEqual(top - 1);
  expect(
    shown.y + shown.height,
    `${name}: the board's bottom above the tab bar`,
  ).toBeLessThanOrEqual(bottom + 1);
  // The game's actions under the board are on the screen too
  for (const name of [/Undo/, /^Check/, /^Hint/]) {
    const box = (await page.getByRole('button', { name }).first().boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(size.width);
  }
}

/**
 * Folds and unfolds with the game on the page: at every size the page is the same document, the
 * board shows the same bridges (also as drawn), the saved game is the same and the clock went on.
 */
async function foldAndUnfold(page: Page, key: string) {
  await page.evaluate(() => Object.assign(window, { samePage: true }));
  const before = await saved(page, key);
  const drawn = await board(page).innerHTML();
  let time = await clock(page);
  expect(time, 'the clock runs').toBeGreaterThan(0);
  for (const [name, size] of SIZES) {
    await page.setViewportSize(size);
    await page.evaluate(
      () =>
        new Promise((resolve) => {
          requestAnimationFrame(resolve);
        }),
    );
    expect(await page.evaluate(() => 'samePage' in window), `${name}: no reload`).toBe(true);
    await fits(page, size, name);
    const now = await saved(page, key);
    expect(now.bridges, `${name}: the bridges`).toEqual(before.bridges);
    expect(now.moves, `${name}: the moves`).toBe(before.moves);
    expect(await board(page).innerHTML(), `${name}: the board as drawn`).toBe(drawn);
    // Right away the clock shows at least the time before (it didn't start over), and two
    // seconds later two seconds more (it didn't stop). The test's clock also runs on its own.
    expect(await clock(page), `${name}: the clock kept its time`).toBeGreaterThanOrEqual(time);
    await page.clock.runFor(2_000);
    await expect
      .poll(() => clock(page), `${name}: the clock goes on`)
      .toBeGreaterThanOrEqual(time + 2);
    time = await clock(page);
  }
}

test('a practice game goes on as the phone unfolds, turns and folds again', async ({ page }) => {
  await page.clock.install();
  await seedGame(page, PRACTICE_KEY, {
    puzzle: LADDER,
    bridges: edges(LADDER).map(() => 0),
    solution: LADDER_SOLUTION,
    status: 'Playing',
    moves: 0,
    hints: 0,
  });
  await visit(page, '/practice');
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(1), true);
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(3), true);
  await page.clock.runFor(5_000);
  await expect.poll(() => clock(page)).toBeGreaterThanOrEqual(5);
  await foldAndUnfold(page, PRACTICE_KEY);
  // Still playable on the outer display: the next bridge goes in
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(5), true);
});

test("today's daily goes on as the phone unfolds, turns and folds again", async ({ page }) => {
  await page.clock.install();
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  const built = solution.flatMap((b, e) => (b > 0 ? [e] : []));
  await addBridge(page, DAILY_KEY, puzzle, built[0]!, true);
  await addBridge(page, DAILY_KEY, puzzle, built[1]!, true);
  await page.clock.runFor(5_000);
  await expect.poll(() => clock(page)).toBeGreaterThanOrEqual(5);
  await foldAndUnfold(page, DAILY_KEY);
  await addBridge(page, DAILY_KEY, puzzle, built[2]!, true);
});
