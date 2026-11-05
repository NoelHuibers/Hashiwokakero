// The badge next to the title of Play about the number of solutions: none for the generated
// puzzles, which always have one, and a warning with a tooltip for a puzzle with several, whose
// Check and hints follow only one of them.
import { board, PLAY_KEY, saved, seedGame } from '../board.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION } from '../puzzles.ts';

test('a generated puzzle has no badge about its solutions', async ({ page }) => {
  await visit(page, '/play');
  // The first visit generates a puzzle by itself, proven unique
  await expect(board(page).locator('g.island').first()).toBeVisible();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).unique).toBe(true);
  // The header is there, with the island count next to where the badge was
  await expect(page.getByText(/^\d+ islands$/)).toBeVisible();
  await expect(page.getByText(/unique solution/i)).toHaveCount(0);
  await expect(page.getByText('several solutions', { exact: true })).toHaveCount(0);
});

test('a puzzle with several solutions says so, and why it matters in a tooltip', async ({
  page,
}) => {
  // Not true of the ladder, but the page only goes by what was saved, as for a puzzle handed
  // over from the workshop
  await seedGame(page, PLAY_KEY, {
    puzzle: LADDER,
    bridges: LADDER_SOLUTION.map(() => 0),
    solution: LADDER_SOLUTION,
    unique: false,
    status: 'Playing',
    moves: 0,
    hints: 0,
  });
  await visit(page, '/play');
  const badge = page.getByText('several solutions', { exact: true });
  await expect(badge).toBeVisible();
  await expect(badge).not.toHaveAttribute('title');
  const text =
    'This puzzle has more than one solution. Check and hints follow one of them, so a correct bridge can still be marked wrong.';
  const tooltip = page.getByRole('tooltip');
  await badge.hover();
  await expect(tooltip).toBeVisible();
  await expect(tooltip).toHaveText(text);
  // From the keyboard too
  await page.mouse.move(0, 0);
  await expect(tooltip).toBeHidden();
  await badge.focus();
  await expect(tooltip).toHaveText(text);
});
