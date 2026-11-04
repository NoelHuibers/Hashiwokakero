// Races 勝負: two players of a group of their own (other tests' races can't get in the way, one
// race at a time per group) open a lobby, join it, start, count down on the server's clock and
// both solve the same fresh puzzle; then the results, the replay and the rematch.
import { DatabaseSync } from 'node:sqlite';

import type { Page } from '@playwright/test';

import { addBridge, board, missing, type Puzzle, saved } from '../board.ts';
import { hintUntil } from '../daily.ts';
import { DB_PATH } from '../env.ts';
import { expect, hydrated, test, visit } from '../fixtures.ts';

/** Where the race page keeps the board (src/pages/race/state.rs). */
const RACE_KEY = 'hashi.race.v1';

/** A race's solution and players (the first to join first), from the server's database: the
 * page only gets the solution once the race is over. */
function stored(race: number) {
  const db = new DatabaseSync(DB_PATH, { readOnly: true, timeout: 5_000 });
  try {
    const row = db.prepare('SELECT solution FROM group_races WHERE id = ?').get(race) as
      | { solution: string | null }
      | undefined;
    if (!row?.solution) throw new Error(`race ${race} has no puzzle yet`);
    const players = db
      .prepare(
        'SELECT user_id FROM group_race_players WHERE race_id = ? ORDER BY joined_at, user_id',
      )
      .all(race) as { user_id: number }[];
    return {
      solution: JSON.parse(row.solution) as number[],
      players: players.map((p) => p.user_id),
    };
  } finally {
    db.close();
  }
}

const raceId = (page: Page) => Number(new URL(page.url()).pathname.split('/').pop());

/** Waits for the puzzle after the countdown, then solves it: hints for all but one bridge of the
 * solution, the last one by hand. */
async function solve(page: Page, solution: number[]) {
  await expect(board(page).locator('g.island').first()).toBeVisible();
  await hintUntil(page, solution, 1, RACE_KEY);
  const { puzzle, bridges } = await saved(page, RACE_KEY);
  const [last] = missing(bridges, solution);
  if (last === undefined) throw new Error('hints left no bridge to build');
  for (let n = bridges[last] ?? 0; n < solution[last]!; n++) {
    await addBridge(page, RACE_KEY, puzzle as Puzzle, last);
  }
}

test('two players race on one puzzle, see the results and meet again in the rematch', async ({
  page,
  browser,
  baseURL,
}) => {
  test.setTimeout(180_000);
  // The host makes a group (as a guest) and a friend joins it with the link
  await visit(page, '/groups');
  await page.getByLabel('Group name').fill(`Lunch race ${Date.now().toString(36)}`);
  await page.getByRole('button', { name: 'Create the group' }).click();
  const ready = page.getByTestId('group-ready');
  const invite = await ready.getByTestId('invite-link').inputValue();
  const friendContext = await browser.newContext(baseURL ? { baseURL } : {});
  const friend = await friendContext.newPage();
  await friend.goto(new URL(invite).pathname);
  await hydrated(friend);
  await friend.getByRole('button', { name: /^Join the group/ }).click();
  await expect(friend).toHaveURL(/\/groups\/\d+$/);
  const groupPath = new URL(friend.url()).pathname;

  // The host opens a race, an easy one
  await visit(page, groupPath);
  const card = page.getByTestId('race-card');
  await expect(card.getByText('No races yet', { exact: false })).toBeVisible();
  await card.getByRole('button', { name: 'Easy' }).click();
  await card.getByTestId('race-open-new').click();
  await expect(page).toHaveURL(/\/groups\/\d+\/races\/\d+$/);
  await hydrated(page);
  const lobby = page.getByTestId('race-lobby');
  await expect(lobby.locator('[data-testid="race-players"] li')).toHaveCount(1);
  await expect(page.getByTestId('race-start')).toBeDisabled();
  await expect(lobby).toContainText('Waiting for at least one more player');

  // The friend sees it on the group page and joins with one tap
  await visit(friend, groupPath);
  const open = friend.getByTestId('race-open');
  await expect(open).toContainText('Race open · 1 joined');
  await open.getByTestId('race-card-join').click();
  await expect(friend).toHaveURL(new URL(page.url()).pathname);
  await hydrated(friend);
  await expect(friend.locator('[data-testid="race-players"] li')).toHaveCount(2);
  await expect(friend.getByTestId('race-start')).toHaveCount(0);

  // The host sees the friend in the lobby and starts: 用意, 3, 2, 1, 始め
  await expect(lobby.locator('[data-testid="race-players"] li')).toHaveCount(2);
  await page.getByTestId('race-start').click();
  for (const p of [page, friend]) {
    await expect(p.getByTestId('race-countdown')).toBeVisible();
  }
  const race = raceId(page);
  const { solution, players } = stored(race);
  const [host, guest] = players;

  // The host solves it first, the friend sees the host's bar at the finish while still racing
  await solve(page, solution);
  await expect(page.getByTestId('race-done')).toContainText('Finished!');
  await expect(
    friend.locator(`[data-testid="race-tracks"] li[data-user="${host}"]`),
  ).toHaveAttribute('data-state', 'solved');
  await solve(friend, solution);

  // Over for both: the host first, the friend second, the replay of both
  for (const p of [page, friend]) {
    const results = p.getByTestId('race-results');
    await expect(results).toBeVisible();
    const rows = results.locator('tbody tr');
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toHaveAttribute('data-user', String(host));
    await expect(rows.nth(1)).toHaveAttribute('data-user', String(guest));
    await expect(rows.nth(0)).toHaveAttribute('data-state', 'placed');
    await expect(rows.nth(1)).toHaveAttribute('data-state', 'placed');
    await expect(p.getByTestId('race-replay').locator('svg.board-svg')).toBeVisible();
  }

  // The host asks for a rematch; the friend, still on the results, joins it with one tap
  await page.getByTestId('race-rematch').click();
  await expect(page).not.toHaveURL(new RegExp(`/races/${race}$`));
  await expect(page.getByTestId('race-lobby')).toBeVisible();
  await friend.getByTestId('race-rematch-join').click();
  await expect(friend).toHaveURL(new URL(page.url()).pathname);
  await expect(friend.locator('[data-testid="race-players"] li')).toHaveCount(2);
  await expect(page.locator('[data-testid="race-players"] li')).toHaveCount(2);

  // The finished race is in the group's history, won by the host
  await visit(friend, groupPath);
  await expect(friend.getByTestId('race-history').locator(`a[data-past="${race}"]`)).toBeVisible();
  await friendContext.close();
});

test('a stranger sees no race', async ({ page }) => {
  await visit(page, '/groups/1/races/1');
  await expect(page.getByTestId('race-missing')).toBeVisible();
});
