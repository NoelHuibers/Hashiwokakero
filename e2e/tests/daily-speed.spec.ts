// How quickly the daily answers the player at its two moments that count: the start (the clock
// runs from the server's start, a board that waits for anything else loses the player time) and
// the finish (the win and the stopped clock show at once, the server's reply only brings the rank
// and its own time). Each test holds every request the moment mustn't wait for: one that the page
// waits for shows as a board or win card that never comes.
import type { Page, Route } from '@playwright/test';

import { register } from '../account.ts';
import { addBridge, board, DAILY_KEY, saved } from '../board.ts';
import { dailySolution, expectResult, solveDaily, startDaily } from '../daily.ts';
import { expect, newUsername, test, visit } from '../fixtures.ts';

const winCard = (page: Page) => page.getByRole('dialog', { name: /Well done!/ });
const winTime = (page: Page) => page.getByTestId('daily-win-time');
const headerTime = (page: Page) =>
  page.getByText('Time · 時間', { exact: true }).locator('..').locator('span').first();

/** A time as the page shows it (`format_score`: 1:05.27 or 1:02:05.27), in ms. */
function scoreMs(text: string | null): number {
  const parts = (text ?? '').split(':').map(Number);
  if (parts.length < 2 || parts.some(Number.isNaN)) throw new Error(`not a time: ${text}`);
  return Math.round(parts.reduce((total, part) => total * 60 + part, 0) * 1000);
}

/** Holds every server function but `except` until the test ends, and names the ones asked. */
async function holdServerFunctions(page: Page, except: string) {
  const asked: string[] = [];
  const held = new Promise<void>(() => {
    // Never settles: the page gets these answers only after the test
  });
  await page.route('**/api/**', async (route: Route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith(`/${except}`)) return route.fallback();
    asked.push(path);
    await held;
  });
  return asked;
}

test('a signed-in player gets the board from the start reply alone', async ({ page }) => {
  await visit(page, '/account');
  await register(page, newUsername('quick'), 'bridges-all-the-way');
  await page.getByRole('link', { name: /Daily/ }).first().click();
  const start = page.getByRole('button', { name: /^Start/ });
  await expect(start).toBeEnabled();
  // Nothing but the start may stand between the press and the board: the page refreshes who
  // plays afterwards, the board doesn't wait for that
  await holdServerFunctions(page, 'start_daily');
  await start.click();
  await expect(board(page)).toHaveClass(/board-interactive/);
  await expect(board(page).locator('g.island').first()).toBeVisible();
  const { puzzle } = await saved(page, DAILY_KEY);
  const solution = dailySolution(puzzle);
  const first = solution.findIndex((b) => b > 0);
  await addBridge(page, DAILY_KEY, puzzle, first);
  expect((await saved(page, DAILY_KEY)).bridges[first]).toBe(1);
});

test('pointing at Start warms the server up, pressing it lifts the fog at once', async ({
  page,
}) => {
  await visit(page, '/');
  const start = page.getByRole('button', { name: /^Start/ });
  await expect(start).toBeEnabled();
  let warmUps = 0;
  await page.route('**/api/start_warm_up', async (route) => {
    warmUps++;
    await route.fallback();
  });
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/start_daily', async (route) => {
    await held;
    await route.fallback();
  });
  await start.hover();
  await expect.poll(() => warmUps).toBe(1);
  // Once per while, not for every move over the button
  await page.mouse.move(0, 0);
  await start.hover();
  await start.focus();
  try {
    await start.click();
    // Before the server answers: the press shows
    await expect(page.getByRole('button', { name: /^Preparing/ })).toBeDisabled();
    await expect(page.getByTestId('daily-fog')).toHaveClass(/opacity-90/);
  } finally {
    release();
  }
  await expect(board(page)).toHaveClass(/board-interactive/);
  await expect(page.getByTestId('daily-fog')).toHaveCount(0);
  expect(warmUps).toBe(1);
});

test('the win shows with a stopped clock while the submit is still on its way', async ({
  page,
}) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  // The board is built once: hints, checks and the finish change the daily's info, not the board
  await board(page).evaluate((svg) => svg.setAttribute('data-first-board', ''));
  // The server gets the board right away, its answer is held back
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let answered = false;
  await page.route('**/api/submit_daily_attempt', async (route) => {
    const response = await route.fetch();
    await held;
    answered = true;
    await route.fulfill({ response });
  });
  let solved = '';
  try {
    await solveDaily(page, puzzle, solution);
    const win = winCard(page);
    await expect(win).toBeVisible();
    await expect(win).toBeFocused();
    expect(answered).toBe(false);
    await expect(win.getByText('Saving your result…')).toBeVisible();
    // Sharing and the replay tell the server's time and rank, they wait for them
    await expect(win.getByRole('button', { name: /Share|Copy/ })).toHaveCount(0);
    await expect(page.getByTestId('win-replay')).toHaveCount(0);
    // The clock stopped at the solve: the card and the header say the same, and keep saying it
    solved = (await winTime(page).textContent())!;
    await expect(headerTime(page)).toHaveText(solved);
    // biome-ignore lint/suspicious/noPlaywrightWaitForTimeout: real time has to pass for a running clock to show
    await page.waitForTimeout(1_500);
    await expect(winTime(page)).toHaveText(solved);
    await expect(headerTime(page)).toHaveText(solved);
  } finally {
    release();
  }
  const card = await expectResult(page);
  const win = winCard(page);
  await expect(win.getByText(/^Rank \d+ of \d+ today$/)).toBeVisible();
  await expect(page.getByTestId('win-replay')).toBeVisible();
  // The server's time: when the board reached it, a moment after the solve the card showed
  const official = await card.locator('p.font-pixel.text-3xl').textContent();
  await expect(winTime(page)).toHaveText(official!);
  await expect(headerTime(page)).toHaveText(official!);
  // It differs from the solve the card showed only by the trip to the server: a few ms here, a
  // second would be a clock that ran on, or stopped at the wrong time
  expect(Math.abs(scoreMs(official) - scoreMs(solved))).toBeLessThan(1_000);
  await expect(board(page)).toHaveAttribute('data-first-board', '');
});

test('a held submit reply never holds up the card, and a dismissed card stays away', async ({
  page,
}) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/submit_daily_attempt', async (route) => {
    const response = await route.fetch();
    await held;
    await route.fulfill({ response });
  });
  try {
    await solveDaily(page, puzzle, solution);
    await expect(winCard(page)).toBeVisible();
    await winCard(page).getByRole('button', { name: 'Admire' }).click();
    await expect(winCard(page)).toHaveCount(0);
  } finally {
    release();
  }
  await expectResult(page);
  await expect(winCard(page)).toHaveCount(0);
});
