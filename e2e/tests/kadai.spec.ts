// Group puzzles 課題: a puzzle drawn in the workshop, proven unique and posted to the seeded group,
// solved by another member like a daily, the ranking for both; and a closed one in the archive.
import type { Page } from '@playwright/test';

import { board, build, cellCenter, saved } from '../board';
import { expect, hydrated, signIn, test, visit } from '../fixtures';
import { CLOSED_PUZZLE, SEEDED_GROUP, player } from '../seed';

/** Where the page keeps the board of the group puzzle being played (src/pages/kadai/state.rs). */
const KADAI_KEY = 'hashi.kadai.v1';

/** A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it. */
const ISLANDS = [
  { x: 0, y: 0, n: 2 },
  { x: 2, y: 0, n: 3 },
  { x: 2, y: 2, n: 1 },
];
const SOLUTION = [2, 1];

/** The ranking's row of a member. */
const ranked = (page: Page, name: string) => page.getByTestId('kadai-ranking').locator(`tr[data-user="${player(name).id}"]`);

test('a puzzle is posted to the group, a member solves it, both see the ranking', async ({ page, context, browser }) => {
  test.slow();
  const title = `Tiny ${Date.now().toString(36)}`;
  await signIn(context, 'aiko');
  await visit(page, `/create?group=${SEEDED_GROUP.id}`);
  for (const { x, y, n } of ISLANDS) {
    const { x: px, y: py } = await cellCenter(page, x, y);
    await page.mouse.move(px, py);
    await page.keyboard.press(String(n));
  }
  const challenge = page.getByTestId('challenge-card');
  // Not proven unique yet: the card asks for the check first
  await challenge.getByRole('button', { name: 'Check uniqueness' }).click();
  await expect(challenge.getByText('Exactly one solution: ready to post!')).toBeVisible();
  await expect(challenge.getByLabel('Group')).toHaveValue(String(SEEDED_GROUP.id));
  await challenge.getByLabel('Title (optional)').fill(title);
  await challenge.getByRole('button', { name: /^Challenge the group/ }).click();

  // The author's view: no start button, everyone waiting
  await expect(page).toHaveURL(new RegExp(`/groups/${SEEDED_GROUP.id}/puzzles/\\d+$`));
  await hydrated(page);
  const puzzleUrl = new URL(page.url()).pathname;
  await expect(page.getByTestId('kadai-title')).toHaveText(title);
  await expect(page.getByTestId('kadai-author')).toBeVisible();
  await expect(page.getByTestId('kadai-start')).toHaveCount(0);
  await expect(page.getByTestId('kadai-solution')).toBeVisible();
  await expect(ranked(page, 'Drowsy Otter 21')).toHaveAttribute('data-state', 'waiting');
  await expect(page.getByTestId('kadai-stats')).toContainText(`0 / ${SEEDED_GROUP.members.length - 1}`);

  // The guest finds it on the group page and plays it
  const otterContext = await browser.newContext();
  await signIn(otterContext, 'Drowsy Otter 21');
  const otter = await otterContext.newPage();
  await visit(otter, `/groups/${SEEDED_GROUP.id}`);
  const row = otter.getByTestId('kadai-open').locator('a').filter({ hasText: title });
  await expect(row).toContainText(`0 of ${SEEDED_GROUP.members.length - 1} solved`);
  await row.click();
  await hydrated(otter);
  // Before the start: the cover, no puzzle in the page yet
  await expect(otter.getByTestId('kadai-cover')).toBeVisible();
  await expect(otter.locator('svg.board-svg g.island')).toHaveCount(0);
  await otter.getByTestId('kadai-start').click();
  await expect(board(otter).locator('g.island text')).toHaveText(['2', '3', '1']);
  const game = await saved(otter, KADAI_KEY);
  // A check costs 10 s and finds nothing wrong on an empty board
  await otter.getByRole('button', { name: /^Check/ }).click();
  await expect(otter.getByText('So far so good: every bridge belongs to the solution.')).toBeVisible();
  // The penalty above the board, not the price on the button
  await expect(otter.locator('.font-pixel.text-xl', { hasText: '+10s' })).toBeVisible();
  // A person's pace: three bridges in a fifth of a second would be too fast to be ranked
  await otter.waitForTimeout(1_000);
  await build(otter, KADAI_KEY, game.puzzle, SOLUTION);
  const result = otter.getByTestId('kadai-result');
  await expect(result.getByRole('heading', { name: /Well done!/ })).toBeVisible();
  await expect(result.getByTestId('kadai-place')).toHaveText(`Place 1 of 1 in ${SEEDED_GROUP.name}`);
  await expect(ranked(otter, 'Drowsy Otter 21')).toHaveAttribute('data-state', 'ranked');
  await expect(ranked(otter, 'Drowsy Otter 21')).toContainText('0h 1c');
  await otterContext.close();

  // The author sees the time too
  await visit(page, puzzleUrl);
  await expect(ranked(page, 'Drowsy Otter 21')).toHaveAttribute('data-state', 'ranked');
  await expect(ranked(page, 'Drowsy Otter 21')).toContainText(/\d:\d\d/);
  await expect(page.getByTestId('kadai-stats')).toContainText(`1 / ${SEEDED_GROUP.members.length - 1}`);

  // And takes it back: gone from the group
  await page.getByTestId('kadai-withdraw').click();
  await page.getByRole('dialog').getByRole('button', { name: 'Withdraw the puzzle' }).click();
  await expect(page).toHaveURL(new RegExp(`/groups/${SEEDED_GROUP.id}$`));
  await hydrated(page);
  await expect(page.getByTestId('kadai-card')).toBeVisible();
  await expect(page.getByTestId('kadai-card').getByText(title)).toHaveCount(0);
});

test('a closed puzzle keeps its final board and ranking in the archive', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  const archive = page.getByTestId('kadai-archive');
  await archive.locator('summary').click();
  const item = archive.locator(`a[data-closed="${CLOSED_PUZZLE.id}"]`);
  await expect(item).toContainText(CLOSED_PUZZLE.title);
  await expect(item).toContainText('aiko');
  await item.click();
  await hydrated(page);
  await expect(page.getByTestId('kadai-title')).toHaveText(CLOSED_PUZZLE.title);
  // The solution with the bridge that took the most help marked, and how it went
  await expect(page.getByTestId('kadai-solution')).toBeVisible();
  await expect(page.getByTestId('kadai-summary')).toContainText('2 / 5');
  await expect(page.getByTestId('kadai-hardest')).toBeVisible();
  await expect(page.getByTestId('kadai-place')).toHaveText(`Place 1 of 2 in ${SEEDED_GROUP.name}`);
  await expect(ranked(page, 'aiko')).toHaveAttribute('data-state', 'ranked');
  await expect(ranked(page, 'bram')).toContainText('1h 1c');
  await expect(ranked(page, 'fatima')).toHaveAttribute('data-state', 'unranked');
  await expect(ranked(page, 'Drowsy Otter 21')).toContainText("didn't finish");
  // Nobody can start it any more
  await expect(page.getByTestId('kadai-start')).toHaveCount(0);
});

test('a stranger sees no group puzzle', async ({ page }) => {
  await visit(page, `/groups/${SEEDED_GROUP.id}/puzzles/${CLOSED_PUZZLE.id}`);
  await expect(page.getByTestId('kadai-missing')).toBeVisible();
  await expect(page.getByText(CLOSED_PUZZLE.title)).toHaveCount(0);
});
