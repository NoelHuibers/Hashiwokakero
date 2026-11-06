// Races 勝負: two players of a group of their own (other tests' races can't get in the way, one
// race at a time per group) open a lobby, join it, start, count down on the server's clock and
// both solve the same fresh puzzle; then the results, the replay and the rematch.
import { board, saved } from '../board.ts';
import { expect, hydrated, signIn, test, visit } from '../fixtures.ts';
import { RACE_KEY, raceId, solve, stored } from '../race.ts';
import { player, SEEDED_GROUP, SEEDED_RACE } from '../seed.ts';

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

  // Ranked like the daily: never a pause, no new puzzle and no difficulty chips while racing
  for (const p of [page, friend]) {
    await expect(board(p).locator('g.island').first()).toBeVisible();
    await expect(p.getByRole('button', { name: 'Pause', exact: true })).toHaveCount(0);
    await expect(p.getByRole('button', { name: 'New puzzle', exact: true })).toHaveCount(0);
    await expect(p.getByRole('group', { name: 'New puzzle' })).toHaveCount(0);
  }
  const before = (await saved(friend, RACE_KEY)).puzzle;
  await friend.keyboard.press('p');
  await friend.keyboard.press('n');
  await expect(friend.getByTestId('paused')).toHaveCount(0);
  await expect(board(friend)).toBeVisible();
  expect((await saved(friend, RACE_KEY)).puzzle).toEqual(before);
  expect((await saved(friend, RACE_KEY)).paused).toBeUndefined();

  // The host solves it first, the friend sees the host's bar at the finish while still racing
  await solve(page, solution, true);
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

test("a race that is over shows its places and replay to the group's members only", async ({
  page,
  context,
}) => {
  const path = `/groups/${SEEDED_GROUP.id}/races/${SEEDED_RACE.id}`;
  // A stranger: no such race
  await visit(page, path);
  await expect(page.getByTestId('race-missing')).toBeVisible();
  await signIn(context, 'aiko');
  await visit(page, path);
  const rows = page.getByTestId('race-results').locator('tbody tr');
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toHaveAttribute('data-user', String(player('aiko').id));
  await expect(rows.nth(1)).toHaveAttribute('data-user', String(player('bram').id));
  await expect(rows.nth(1)).toContainText('1h 1c');
  await expect(rows.nth(2)).toHaveAttribute('data-state', 'gave-up');
  await expect(page.getByTestId('race-replay').locator('svg.board-svg')).toBeVisible();
  // In the group's history, won by aiko
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  await expect(
    page.getByTestId('race-history').locator(`a[data-past="${SEEDED_RACE.id}"]`),
  ).toContainText('aiko');
});
