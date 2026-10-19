// Practice: a generated puzzle (a server function) and a bridge built on it (the board's
// pointer handling in the WASM).
import type { Page } from '@playwright/test';

import { expect, test, visit } from '../fixtures';

// The first board, the legend below it draws small ones
const board = (page: Page) => page.locator('svg.board-svg').first();
const bridges = (page: Page) => board(page).locator('g.bridge:not(.ghost)');

/** Where the islands are, to tell one puzzle from another. */
const layout = (page: Page) =>
  board(page)
    .locator('g.island')
    .evaluateAll((islands) => islands.map((g) => `${g.getAttribute('transform')}:${g.textContent}`).join(' '));

/** The middle of the water between an island and its right neighbour, in page coordinates. */
async function gap(page: Page) {
  await board(page).scrollIntoViewIfNeeded();
  const point = await board(page)
    .locator('g.island')
    .evaluateAll((islands) => {
      const rects = islands.map((g) => g.getBoundingClientRect());
      for (const a of rects) {
        const right = rects
          .filter((b) => Math.abs(b.y - a.y) < 2 && b.x > a.x)
          .sort((p, q) => p.x - q.x)[0];
        // At least one empty cell between them, so the point is on water and not an island
        if (right && right.x - (a.x + a.width) > a.width / 2) {
          return { x: (a.x + a.width + right.x) / 2, y: a.y + a.height / 2 };
        }
      }
      return null;
    });
  expect(point, 'two islands in a row with water between them').not.toBeNull();
  return point!;
}

async function newPuzzle(page: Page) {
  await visit(page, '/practice');
  // The first visit generates a puzzle by itself
  await expect(board(page).locator('g.island').first()).toBeVisible();
  const first = await layout(page);
  await page.getByRole('button', { name: /^Generate/ }).click();
  await expect.poll(() => layout(page), { message: 'a new puzzle' }).not.toBe(first);
  await expect(page.getByText('Growing islands')).toBeHidden();
  await expect(bridges(page)).toHaveCount(0);
}

test('generate a puzzle and build a bridge with the mouse', async ({ page }) => {
  await newPuzzle(page);
  const { x, y } = await gap(page);
  await page.mouse.click(x, y);
  await expect(bridges(page)).toHaveCount(1);
});

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('generate a puzzle and build a bridge with a tap', async ({ page }) => {
    await newPuzzle(page);
    const { x, y } = await gap(page);
    await page.touchscreen.tap(x, y);
    await expect(bridges(page)).toHaveCount(1);
  });
});
