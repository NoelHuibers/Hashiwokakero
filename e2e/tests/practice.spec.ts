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

/**
 * The middle of the water between an island and its right neighbour, in the viewport
 * coordinates the mouse and touchscreen take. Computed from the islands' cells: their boxes
 * also hold the trees and gates on them, and were off centre.
 */
async function gap(page: Page) {
  await board(page).scrollIntoViewIfNeeded();
  const point = await board(page)
    .locator('g.island')
    .evaluateAll((islands) => {
      const svg = (islands[0] as SVGGElement).ownerSVGElement!;
      const toPage = svg.getScreenCTM()!;
      // translate(x,y) in board units, 16 per cell
      const cells = islands.map((g) => g.getAttribute('transform')!.match(/-?[\d.]+/g)!.map(Number));
      for (const [x, y] of cells) {
        const right = cells.filter(([bx, by]) => by === y && bx > x).sort((p, q) => p[0] - q[0])[0];
        // At least one empty cell between them, so the point is on water and not an island
        if (right && right[0] - x >= 32) {
          const p = new DOMPoint((x + right[0]) / 2 + 8, y + 8).matrixTransform(toPage);
          return { x: p.x, y: p.y };
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
