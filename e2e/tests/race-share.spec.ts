// Races 勝負 and shared puzzles 共有: a race's puzzle shared afterwards has the racers' times on
// its board, and a shared puzzle is raced by a group from its page (/p/<code>). Each test races in
// a fresh group of its own (one race at a time per group).
import type { Page } from '@playwright/test';

import { build, SHARED_KEY } from '../board.ts';
import { expect, hydrated, test, visit } from '../fixtures.ts';
import { RUNGS, RUNGS_SOLUTION } from '../puzzles.ts';
import { groupOfTwo, raceId, solve, stored } from '../race.ts';
import { RACE_SHARE } from '../seed.ts';

/** The rows of a shared puzzle's board. */
const boardRow = (page: Page, user: number) =>
  page.locator(`[data-testid="shared-board"] tr[data-user="${user}"]`);

/** The host starts the race in the lobby both are in, both solve it; returns the racers. */
async function race(page: Page, friend: Page) {
  await expect(page.locator('[data-testid="race-players"] li')).toHaveCount(2);
  await page.getByTestId('race-start').click();
  await expect(page.getByTestId('race-countdown')).toBeVisible();
  const { solution, players } = stored(raceId(page));
  await solve(page, solution);
  await solve(friend, solution);
  for (const p of [page, friend]) {
    await expect(p.getByTestId('race-results').locator('tbody tr')).toHaveCount(2);
  }
  const [host, guest] = players;
  if (host === undefined || guest === undefined) throw new Error('two racers');
  return { solution, host, guest };
}

test("a race's puzzle shared afterwards has the racers' times on its board", async ({
  page,
  browser,
  baseURL,
}) => {
  test.setTimeout(180_000);
  const { friend, groupPath } = await groupOfTwo(page, browser, baseURL);
  await visit(page, groupPath);
  const card = page.getByTestId('race-card');
  await card.getByRole('button', { name: 'Easy' }).click();
  await card.getByTestId('race-open-new').click();
  await expect(page).toHaveURL(/\/groups\/\d+\/races\/\d+$/);
  await hydrated(page);
  await visit(friend, new URL(page.url()).pathname);
  await friend.getByTestId('race-join').click();
  const { host, guest } = await race(page, friend);

  // Over: the host shares the puzzle as a link of theirs
  const share = page.getByTestId('race-share');
  await share.getByRole('button', { name: /^Share this puzzle/ }).click();
  const link = share.getByTestId('share-link');
  await expect(link).toHaveText(/\/p\/[a-z2-9]{8}$/);
  const url = (await link.getAttribute('href'))!;
  await expect(share.getByRole('button', { name: 'Send the link', exact: true })).toBeVisible();

  // Its board has both racers' times, the ones to beat
  await visit(friend, new URL(url).pathname);
  await expect(friend.locator('[data-testid="shared-board"] tbody tr')).toHaveCount(2);
  for (const user of [host, guest]) {
    await expect(boardRow(friend, user)).toContainText(/\d\d:\d\d/);
  }
  await expect(friend.getByTestId('shared-to-beat')).toBeVisible();
  await friend.context().close();
});

test('a shared puzzle is raced by a group from its page', async ({ page, browser, baseURL }) => {
  test.setTimeout(180_000);
  const { friend, groupPath } = await groupOfTwo(page, browser, baseURL);
  const path = `/p/${RACE_SHARE.code}`;
  // The friend solved it on its page before
  await visit(friend, path);
  await build(friend, SHARED_KEY, RUNGS, RUNGS_SOLUTION);
  await expect(friend.getByTestId('shared-result')).toContainText('Solved!');

  // The host is in a group: "Race this with my group" opens a lobby on this puzzle
  await visit(page, path);
  const card = page.getByTestId('shared-race');
  await expect(card).toContainText('Race this with my group');
  await card.getByTestId('shared-race-open').click();
  await expect(page).toHaveURL(/\/groups\/\d+\/races\/\d+$/);
  await hydrated(page);
  await expect(page.getByTestId('race-shared')).toContainText(
    `Shared puzzle · by ${RACE_SHARE.sharer}`,
  );
  await expect(page.getByTestId('race-lobby')).toContainText('this shared puzzle');

  // The friend joins from the group page, which says whose puzzle it is; they know it already
  await visit(friend, groupPath);
  const open = friend.getByTestId('race-open');
  await expect(open).toContainText(`Shared puzzle · by ${RACE_SHARE.sharer}`);
  await open.getByTestId('race-card-join').click();
  await expect(friend).toHaveURL(new URL(page.url()).pathname);
  const lobbyRows = page.locator('[data-testid="race-players"] li');
  await expect(lobbyRows.nth(1).getByTestId('race-solved-before')).toBeVisible();
  await expect(lobbyRows.nth(0).getByTestId('race-solved-before')).toHaveCount(0);

  // A normal race on the shared puzzle
  const { solution, host, guest } = await race(page, friend);
  expect(solution).toEqual(RUNGS_SOLUTION);
  const results = page.getByTestId('race-results');
  await expect(
    results.locator(`tr[data-user="${guest}"]`).getByTestId('race-solved-before'),
  ).toBeVisible();
  await expect(
    results.locator(`tr[data-user="${host}"]`).getByTestId('race-solved-before'),
  ).toHaveCount(0);

  // The host's race time is on the puzzle's board, through the link it came from; the friend
  // keeps their first solve
  await visit(page, path);
  await expect(boardRow(page, host)).toContainText(/\d\d:\d\d/);
  await expect(boardRow(page, guest)).toBeVisible();
  await friend.context().close();
});
