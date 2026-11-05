// Play's controls above the board: a new puzzle with one tap on a difficulty, the button next to
// the clock and N, the toast that brings the puzzle left back, and pause (P), which stops the
// clock and hides the board. Only on Play: the daily is ranked and never pauses.
import type { Page } from '@playwright/test';

import { addBridge, board, edges, PLAY_KEY, saved, seedGame } from '../board.ts';
import { startDaily } from '../daily.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION } from '../puzzles.ts';

/** The ladder, two bridges into it, with a hint and 75 seconds on the clock. */
const begun = {
  puzzle: LADDER,
  bridges: LADDER_SOLUTION.map((b, e) => (e < 2 ? b : 0)),
  solution: LADDER_SOLUTION,
  status: 'Playing' as const,
  moves: 2,
  hints: 1,
  elapsed: 75,
};

const islands = (page: Page) =>
  board(page)
    .locator('g.island')
    .evaluateAll((all) => all.map((g) => `${g.getAttribute('transform')}:${g.textContent}`));

/** The puzzle on the board isn't the ladder any more: a new one was generated. */
async function expectNewPuzzle(page: Page) {
  await expect
    .poll(async () => JSON.stringify((await saved(page, PLAY_KEY)).puzzle), {
      message: 'a new puzzle',
    })
    .not.toBe(JSON.stringify(LADDER));
  await expect(page.getByText('Growing islands')).toBeHidden();
}

/** The clock's value, by its label (desktop shows the kanji with it). */
const clock = (page: Page) =>
  page.getByText('Time · 時間', { exact: true }).locator('..').locator('span').first();

/**
 * Stops the clock before the page loads: its timer only fires when the test runs it on
 * (`runFor`), however long loading takes. See the screenshot of Play in visual.spec.ts.
 */
async function freezeClock(page: Page) {
  await page.clock.install({ time: new Date('2026-11-05T09:00:00Z') });
  await page.clock.pauseAt(new Date('2026-11-05T09:00:01Z'));
}

const chips = (page: Page) => page.getByRole('group', { name: 'New puzzle' });
const newPuzzle = (page: Page) => page.getByRole('button', { name: 'New puzzle', exact: true });
const pause = (page: Page) => page.getByRole('button', { name: 'Pause', exact: true });
const cover = (page: Page) => page.getByTestId('paused');
const back = (page: Page) => page.getByRole('button', { name: 'Back to previous puzzle' });

test('a difficulty chip makes a new puzzle of it at once and stays marked', async ({ page }) => {
  await seedGame(page, PLAY_KEY, { ...begun, bridges: begun.bridges.map(() => 0), moves: 0 });
  await visit(page, '/play');
  const easy = chips(page).getByRole('button', { name: 'Easy', exact: true });
  await expect(chips(page).getByRole('button')).toHaveCount(5);
  // A custom puzzle on the board: the difficulty kept is the default one
  await expect(chips(page).getByRole('button', { name: 'Medium' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(easy).toHaveAttribute('aria-pressed', 'false');
  await expect(easy).toHaveAttribute('data-tooltip', 'Easy · 7×7');
  await easy.click();
  await expectNewPuzzle(page);
  await expect(page.getByText(/^Easy 7×7$/)).toBeVisible();
  await expect(easy).toHaveAttribute('aria-pressed', 'true');
  expect((await saved(page, PLAY_KEY)).puzzle.width).toBe(7);
  // Nothing was begun, nothing to go back to
  await expect(back(page)).toHaveCount(0);

  // The button next to the clock: the same difficulty again, and so after a reload
  const before = await islands(page);
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(easy).toHaveAttribute('aria-pressed', 'true');
  await expect(newPuzzle(page)).toHaveAttribute('data-tooltip', /^New puzzle · 初級 Easy · N$/);
  await newPuzzle(page).click();
  await expect.poll(() => islands(page), { message: 'another puzzle' }).not.toEqual(before);
  await expect(page.getByText(/^Easy 7×7$/)).toBeVisible();
});

test('N makes a new puzzle, but not while typing', async ({ page }) => {
  await seedGame(page, PLAY_KEY, { ...begun, bridges: begun.bridges.map(() => 0), moves: 0 });
  await visit(page, '/play');
  // A field of the page's own would take the letter, so does any other
  await page.evaluate(() => {
    const input = document.createElement('input');
    input.id = 'typing';
    document.querySelector('main')!.append(input);
  });
  await page.locator('#typing').press('n');
  await expect(page.locator('#typing')).toHaveValue('n');
  // A new puzzle would be on its way at once: the chips wait for it
  await expect(page.getByText('Growing islands')).toBeHidden();
  await expect(chips(page).getByRole('button').first()).toBeEnabled();
  expect((await saved(page, PLAY_KEY)).puzzle).toEqual(LADDER);

  await page.locator('h1').click();
  await page.keyboard.press('n');
  await expectNewPuzzle(page);
  await expect(page.getByText(/^Medium 10×10$/)).toBeVisible();
});

test('a new puzzle drops the one begun without asking, and the toast brings it back exactly', async ({
  page,
}) => {
  // The clock only moves when the test says so: the times compared stay exact
  await freezeClock(page);
  await seedGame(page, PLAY_KEY, begun);
  await visit(page, '/play');
  await expect(clock(page)).toHaveText('01:15');
  // One more bridge: history the reload doesn't keep, which going back brings along
  const third = edges(LADDER).findIndex((_, e) => e >= 2 && LADDER_SOLUTION[e]! > 0);
  await addBridge(page, PLAY_KEY, LADDER, third);
  const left = await saved(page, PLAY_KEY);
  expect(left.moves).toBe(3);

  await newPuzzle(page).click();
  await expectNewPuzzle(page);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const toast = page.getByRole('log').getByText('New puzzle started.');
  await expect(toast).toBeVisible();
  await expect(clock(page)).toHaveText('00:00');

  await back(page).click();
  await expect(back(page)).toHaveCount(0);
  await expect.poll(async () => (await saved(page, PLAY_KEY)).puzzle).toEqual(LADDER);
  const restored = await saved(page, PLAY_KEY);
  expect(restored.bridges).toEqual(left.bridges);
  expect([restored.moves, restored.hints, restored.status]).toEqual([3, 1, 'Playing']);
  await expect(clock(page)).toHaveText('01:15');
  await expect(page.getByText(/^Your own puzzle$/)).toBeVisible();
  // Undo steps back over the bridge built before the new puzzle
  await page.getByRole('button', { name: /Undo/ }).click();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).bridges[third]).toBe(0);
  // The new puzzle wasn't begun: going back offers nothing in turn
  await expect(page.getByRole('log').getByText('New puzzle started.')).toHaveCount(0);
});

test('pause stops the clock, hides the board and survives a reload', async ({ page }) => {
  await freezeClock(page);
  await seedGame(page, PLAY_KEY, begun);
  await visit(page, '/play');
  await page.clock.runFor(2_000);
  await expect(clock(page)).toHaveText('01:17');
  await expect(pause(page)).toHaveAttribute('data-tooltip', 'Pause · P');
  await pause(page).click();
  await expect(pause(page)).toHaveAttribute('aria-pressed', 'true');
  await expect(cover(page)).toBeVisible();
  await expect(cover(page).getByRole('heading')).toHaveText('Paused · 休憩');
  // Not just dimmed: the board is hidden under an opaque cover
  await expect(board(page)).toBeHidden();
  expect(await cover(page).evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(
    'rgba(0, 0, 0, 0)',
  );
  await page.clock.runFor(5_000);
  await expect(clock(page)).toHaveText('01:17');
  // No moves meanwhile
  for (const name of [/^Hint/, /^Check/]) {
    await expect(page.getByRole('button', { name })).toBeDisabled();
  }
  await page.keyboard.press('h');
  await page.keyboard.press('Control+z');
  await expect.poll(async () => (await saved(page, PLAY_KEY)).paused).toBe(true);
  const paused = await saved(page, PLAY_KEY);
  expect([paused.bridges, paused.hints, paused.elapsed]).toEqual([begun.bridges, 1, 77]);

  // A reload comes back paused, the clock where it stopped
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(cover(page)).toBeVisible();
  await expect(board(page)).toBeHidden();
  await page.clock.runFor(3_000);
  await expect(clock(page)).toHaveText('01:17');

  // P resumes, the clock goes on from there
  await page.keyboard.press('p');
  await expect(cover(page)).toBeHidden();
  await expect(board(page)).toBeVisible();
  await expect(pause(page)).toHaveAttribute('aria-pressed', 'false');
  await page.clock.runFor(1_000);
  await expect(clock(page)).toHaveText('01:18');
  // And P pauses again, the cover's button resumes
  await page.keyboard.press('p');
  await expect(cover(page)).toBeVisible();
  await cover(page)
    .getByRole('button', { name: /^Resume/ })
    .click();
  await expect(board(page)).toBeVisible();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).paused).toBeUndefined();
});

test('a solved puzzle has nothing to pause', async ({ page }) => {
  await seedGame(page, PLAY_KEY, {
    ...begun,
    bridges: LADDER_SOLUTION,
    status: 'Won',
    moves: 9,
  });
  await visit(page, '/play');
  await expect(pause(page)).toBeDisabled();
  await page.keyboard.press('p');
  await expect(cover(page)).toHaveCount(0);
});

test('the daily never pauses and offers no new puzzle', async ({ page }) => {
  await visit(page, '/');
  await startDaily(page);
  await expect(board(page)).toBeVisible();
  await expect(pause(page)).toHaveCount(0);
  await expect(newPuzzle(page)).toHaveCount(0);
  await expect(chips(page)).toHaveCount(0);
  await page.keyboard.press('p');
  await page.keyboard.press('n');
  await expect(cover(page)).toHaveCount(0);
  await expect(board(page)).toBeVisible();
});
