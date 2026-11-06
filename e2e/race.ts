// What the race specs share: the race page's saved board, the solution from the test database,
// solving across the page's polls, and a fresh group of two to race in.
import { DatabaseSync } from 'node:sqlite';

import type { Browser, Page } from '@playwright/test';

import {
  addBridge,
  board,
  cellCenter,
  edges,
  missing,
  type Puzzle,
  press,
  saved,
} from './board.ts';
import { hintUntil } from './daily.ts';
import { DB_PATH } from './env.ts';
import { expect, hydrated, visit } from './fixtures.ts';

/** Where the race page keeps the board (src/pages/race/state.rs). */
export const RACE_KEY = 'hashi.race.v1';

/** A race's solution and players (the first to join first), from the server's database: the
 * page only gets the solution once the race is over. */
export function stored(race: number) {
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

export const raceId = (page: Page) => Number(new URL(page.url()).pathname.split('/').pop());

/** Waits for the next answer of the race page's poll. */
export const pollAnswer = (page: Page) =>
  page.waitForResponse((r) => r.url().includes('poll_group_race') && r.ok());

/**
 * Builds one bridge with a tap on each island and two of the page's polls in between: the island
 * tapped first stays selected while the answers come in. (Each answer used to build the race's
 * body anew, board and all, so a tap just before one was lost.)
 */
export async function addBridgeAcrossPolls(page: Page, puzzle: Puzzle, edge: number) {
  const pair = edges(puzzle)[edge];
  if (!pair) throw new Error(`the puzzle has no edge ${edge}`);
  const [a, b] = pair;
  const before = (await saved(page, RACE_KEY)).bridges[edge] ?? 0;
  const first = puzzle.islands[a]!.pos;
  await press(page, await cellCenter(page, first.x, first.y), false);
  const selected = board(page).locator('g.island.selected');
  await expect(selected).toHaveCount(1);
  await pollAnswer(page);
  await pollAnswer(page);
  await expect(selected).toHaveCount(1);
  const second = puzzle.islands[b]!.pos;
  await press(page, await cellCenter(page, second.x, second.y), false);
  await expect
    .poll(async () => (await saved(page, RACE_KEY)).bridges[edge], {
      message: `bridge ${a}-${b} built`,
    })
    .toBe((before + 1) % 3);
}

/** Waits for the puzzle after the countdown, then solves it: hints for all but one bridge of the
 * solution, the last one by hand (with polls between the taps when `slow`). */
export async function solve(page: Page, solution: number[], slow = false) {
  await expect(board(page).locator('g.island').first()).toBeVisible();
  await hintUntil(page, solution, 1, RACE_KEY);
  const { puzzle, bridges } = await saved(page, RACE_KEY);
  const [last] = missing(bridges, solution);
  if (last === undefined) throw new Error('hints left no bridge to build');
  for (let n = bridges[last] ?? 0; n < solution[last]!; n++) {
    if (slow) await addBridgeAcrossPolls(page, puzzle as Puzzle, last);
    else await addBridge(page, RACE_KEY, puzzle as Puzzle, last);
  }
}

/**
 * A group of its own (other tests' races can't get in the way, one race at a time per group):
 * `page` makes it as a guest, a friend in another browser joins it with the link. Returns the
 * friend's page and the group's path; close `friend.context()` at the end.
 */
export async function groupOfTwo(page: Page, browser: Browser, baseURL: string | undefined) {
  await visit(page, '/groups');
  await page.getByLabel('Group name').fill(`Race share ${Date.now().toString(36)}`);
  await page.getByRole('button', { name: 'Create the group' }).click();
  const invite = await page.getByTestId('group-ready').getByTestId('invite-link').inputValue();
  const friendContext = await browser.newContext(baseURL ? { baseURL } : {});
  const friend = await friendContext.newPage();
  await friend.goto(new URL(invite).pathname);
  await hydrated(friend);
  await friend.getByRole('button', { name: /^Join the group/ }).click();
  await expect(friend).toHaveURL(/\/groups\/\d+$/);
  return { friend, groupPath: new URL(friend.url()).pathname };
}
