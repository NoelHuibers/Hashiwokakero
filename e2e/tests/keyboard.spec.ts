import { addBridge, board, edges, PRACTICE_KEY, saved, seedGame } from '../board';
import { expect, test, visit } from '../fixtures';
import { LADDER } from '../puzzles';

test('build and remove bridges with the keyboard and hear island degrees', async ({ page }) => {
  await seedGame(page, PRACTICE_KEY, {
    puzzle: LADDER, bridges: edges(LADDER).map(() => 0), solution: null,
    status: 'Playing', moves: 0, hints: 0,
  });
  await visit(page, '/practice');
  await expect(board(page)).toHaveAttribute('tabindex', '0');
  await board(page).focus();
  await expect(page.getByRole('status').filter({ hasText: /^Bridge / })).toContainText('0/2');
  await board(page).press('Enter');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[0]).toBe(1);
  await expect(page.getByRole('status').filter({ hasText: /^Bridge / })).toContainText('1/2');
  await board(page).press('Shift+Enter');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[0]).toBe(0);
  await board(page).press('Shift+=');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[0]).toBe(1);
  await board(page).press('-');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[0]).toBe(0);
  await board(page).press('ArrowRight');
  await board(page).press('Space');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[1]).toBe(1);
});

/**
 * A click focuses the board too. That focus is no keyboard use: the click builds the bridge it
 * aims at, not the one the keyboard selected (it did, and a first click went to edge 0).
 */
test('clicks build their bridge whether or not the keyboard was used first', async ({ page }) => {
  await seedGame(page, PRACTICE_KEY, {
    puzzle: LADDER, bridges: edges(LADDER).map(() => 0), solution: null,
    status: 'Playing', moves: 0, hints: 0,
  });
  await visit(page, '/practice');
  await addBridge(page, PRACTICE_KEY, LADDER, 1);
  expect((await saved(page, PRACTICE_KEY)).bridges[0]).toBe(0);
  await board(page).press('Enter');
  await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[0]).toBe(1);
  await addBridge(page, PRACTICE_KEY, LADDER, 3);
  const bridges = (await saved(page, PRACTICE_KEY)).bridges;
  expect([bridges[0], bridges[1], bridges[3]]).toEqual([1, 1, 1]);
});

test('draw a puzzle using the focused board without a mouse', async ({ page }) => {
  await visit(page, '/create');
  await board(page).focus();
  await board(page).press('2');
  await board(page).press('ArrowRight');
  await board(page).press('ArrowRight');
  await board(page).press('3');
  await board(page).press('ArrowDown');
  await board(page).press('ArrowDown');
  await board(page).press('1');
  await expect(board(page).locator('g.island text')).toHaveText(['2', '3', '1']);
  await expect(page.getByRole('status').filter({ hasText: /^Cell / })).toContainText('(3, 3): 1');
  await board(page).press('Delete');
  await expect(board(page).locator('g.island text')).toHaveText(['2', '3']);
});
