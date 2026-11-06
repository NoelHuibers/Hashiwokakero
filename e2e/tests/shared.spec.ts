// Shared puzzles (/p/<code>): a puzzle solved on Play shared as a link, opened by someone else in
// another browser, solved there, and both on its board. The seeded link (seed.ts) for what the
// server renders.
import { build, PLAY_KEY, SHARED_KEY, seedGame } from '../board.ts';
import { allow, expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION } from '../puzzles.ts';
import { SHARED_PUZZLE } from '../seed.ts';

test('a puzzle shared after solving it is played through its link and ranked', async ({
  page,
  browser,
  baseURL,
}) => {
  // Solved on Play in 42 s: the win screen shares the puzzle itself
  await seedGame(page, PLAY_KEY, {
    puzzle: LADDER,
    bridges: LADDER_SOLUTION,
    solution: LADDER_SOLUTION,
    status: 'Won',
    moves: 9,
    hints: 0,
    elapsed: 42,
  });
  await visit(page, '/play');
  await expect(page.getByRole('heading', { name: /Well done!/ })).toBeVisible();
  // The result alone can still be shared before there is a link
  await expect(page.getByRole('button', { name: 'Share result', exact: true })).toBeVisible();
  await page.getByRole('button', { name: /^Share this puzzle/ }).click();
  const link = page.getByTestId('share-link');
  await expect(link).toHaveText(/\/p\/[a-z2-9]{8}$/);
  const url = (await link.getAttribute('href'))!;
  await expect(page.getByRole('button', { name: 'Send the link', exact: true })).toBeVisible();
  // Players reached through it count toward the sharing stamps
  await expect(page.getByTestId('share-stamps')).toHaveText(
    'Everyone who solves it through your link counts toward three stamps: 道 帯 師',
  );
  // Sharing made them a guest
  await expect(page.locator('header')).toContainText('Guest');

  // Someone else opens it in another browser
  const friendContext = await browser.newContext(baseURL ? { baseURL } : {});
  const friend = await friendContext.newPage();
  await visit(friend, new URL(url).pathname);
  await expect(friend.getByTestId('shared-by')).toContainText('Shared by');
  await expect(friend.getByTestId('shared-to-beat')).toContainText('00:42');
  const sharerRow = friend.locator('[data-testid="shared-board"] tr[data-sharer]');
  await expect(sharerRow).toContainText('00:42');
  await expect(friend.getByTestId('shared-result')).toHaveCount(0);

  // Solved there: the time goes on the board, the friend's row marked as theirs
  await build(friend, SHARED_KEY, LADDER, LADDER_SOLUTION);
  const result = friend.getByTestId('shared-result');
  await expect(result).toContainText('Solved!');
  await expect(result).toContainText(/Your time: \d\d:\d\d, place \d+ of \d+\./);
  const mine = friend.locator('[data-testid="shared-board"] tr.rank-mark');
  await expect(mine).toHaveCount(1);
  await expect(sharerRow).toHaveCount(1);
  await expect(friend.locator('header')).toContainText('Guest');

  // Again: only the first solve counts
  await friend.getByRole('button', { name: 'Play it again' }).click();
  await build(friend, SHARED_KEY, LADDER, LADDER_SOLUTION);
  await expect(result).toContainText('Only your first solve counts');
  await expect(mine).toHaveCount(1);

  // The sharer sees the friend on the board of their link
  await visit(page, new URL(url).pathname);
  await expect(page.locator('[data-testid="shared-board"] tr[data-user]')).not.toHaveCount(1);
  await friendContext.close();

  // Someone else solved a puzzle they shared: the stamp 道, and one of ten for 帯. The friend's
  // solve syncs the sharer's stamps in the background, so the book may need another look
  const tile = (name: string) => page.locator('li').filter({ hasText: name });
  await expect(async () => {
    await visit(page, '/account');
    await expect(tile('Show the way')).not.toHaveClass(/opacity-60/, { timeout: 1_000 });
  }).toPass({ timeout: 15_000 });
  await expect(tile('Black belt')).toContainText('1 / 10');
  await expect(tile('Sensei')).toContainText('1 / 25');
});

test('the seeded link shows its sharer, time and board, a wrong one is lost', async ({
  page,
  problems,
}) => {
  await visit(page, `/p/${SHARED_PUZZLE.code}`);
  await expect(page.getByTestId('shared-title')).toHaveText("aiko's puzzle");
  // bram's 1:35 and a hint are 2:05, behind aiko's 1:42
  await expect(page.getByTestId('shared-to-beat')).toContainText('01:42');
  const rows = page.locator('[data-testid="shared-board"] tbody tr');
  await expect(rows.first()).toContainText('aiko');
  await expect(rows.nth(1)).toContainText('02:05');
  await expect(page.getByTestId('shared-solvers')).toContainText(/Solvers: \d+/);
  await expect(page).toHaveTitle("aiko's Hashi puzzle · Hashi");

  await visit(page, '/p/aaaaaaaa', { status: 404 });
  await expect(page.getByTestId('shared-missing')).toBeVisible();
  // Chrome logs the status of the page itself
  allow(problems, /404/);
});
