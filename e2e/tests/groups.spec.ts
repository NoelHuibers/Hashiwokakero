// Groups 仲間: a group made, its link shared, a friend joining as a guest, and the group's board
// of today, which shows no times before you finished the daily yourself.
import type { Page } from '@playwright/test';

import { solveDaily, startDaily } from '../daily';
import { expect, hydrated, signIn, test, visit } from '../fixtures';
import { SEEDED_GROUP } from '../seed';

/** A card of the page, by its heading. */
const card = (page: Page, heading: RegExp) =>
  page.locator('[data-name="Card"]').filter({ has: page.getByRole('heading', { name: heading }) });

/** The rows of a group's board of today, by member name. */
const row = (page: Page, name: string) =>
  card(page, /^Today/).locator('tr').filter({ hasText: name });

test('a group is made, a friend joins with the link as a guest, times show after the finish', async ({
  page,
  context,
  browser,
  baseURL,
}) => {
  // Both play today's daily
  test.slow();
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  page.on('dialog', (d) => void d.accept());

  // A visitor without a user makes a group: they become a guest, and get the invite step
  await visit(page, '/groups');
  await expect(page.getByText('No groups yet.', { exact: false })).toBeVisible();
  await page.getByLabel('Group name').fill('  Lunch   Bridges ');
  await page.getByRole('button', { name: 'Create the group' }).click();
  const ready = page.getByTestId('group-ready');
  await expect(ready.getByRole('heading', { name: 'Your group is ready!' })).toBeVisible();
  await expect(ready).toContainText('Lunch Bridges');
  const link = ready.getByTestId('invite-link');
  await expect(link).toHaveValue(/\/g\/[a-z2-9_]{10}$/);
  const url = await link.inputValue();
  await ready.getByRole('button', { name: 'Copy link' }).click();
  await expect(page.getByText('Invite link copied to the clipboard.')).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(url);
  // In the list too, with the owner's badge
  await expect(page.locator('a[data-group]').filter({ hasText: 'Lunch Bridges' })).toContainText('Owner');
  // Creating made them a guest
  await expect(page.locator('header')).toContainText('Guest');

  // A friend in another browser opens the link: nothing happens until they click
  const friendContext = await browser.newContext({ baseURL });
  const friend = await friendContext.newPage();
  await friend.goto(new URL(url).pathname);
  await hydrated(friend);
  await expect(friend.getByRole('heading', { name: 'Join “Lunch Bridges”' })).toBeVisible();
  await expect(friend.locator('svg g[data-member]')).toHaveCount(1);
  await expect(friend.getByText('No account needed', { exact: false })).toBeVisible();
  await friend.getByRole('button', { name: /^Join the group/ }).click();
  await expect(friend).toHaveURL(/\/groups\/\d+$/);
  await expect(friend.getByRole('heading', { level: 1, name: 'Lunch Bridges' })).toBeVisible();
  const members = card(friend, /^Members/).locator('li[data-member]');
  await expect(members).toHaveCount(2);
  await expect(friend.locator('svg g.nakama-island')).toHaveCount(2);
  // The link again: a member goes straight to the group
  await friend.goto(new URL(url).pathname);
  await expect(friend).toHaveURL(/\/groups\/\d+$/);

  // The owner sees the friend
  await ready.getByRole('link', { name: /Open the group/ }).click();
  await expect(card(page, /^Members/).locator('li[data-member]')).toHaveCount(2);
  const friendName = (await members.nth(1).locator('a').first().innerText()).trim();

  // The friend solves the daily
  await friend.goto('/');
  await hydrated(friend);
  const { puzzle, solution } = await startDaily(friend);
  await solveDaily(friend, puzzle, solution);
  await expect(friend.getByRole('heading', { name: /Well done!/ })).toBeVisible();
  // On the daily page their group has a tab next to the world's ranking, with their time
  const tab = friend.getByRole('tab', { name: /Lunch Bridges/ });
  await expect(tab).toContainText('1/2');
  await tab.click();
  await expect(friend.locator('aside [data-open="true"] tr[data-state="ranked"]')).toContainText(friendName);

  // The owner hasn't played: who's done, but no times
  await page.reload();
  await hydrated(page);
  await expect(card(page, /^Today/)).toContainText('1 of 2 done – your turn!');
  await expect(row(page, friendName)).toHaveAttribute('data-state', 'done');
  await expect(row(page, friendName)).not.toContainText(/\d:\d\d/);
  await expect(page.locator('svg g.nakama-island:not(.waiting)')).toHaveCount(1);
  // The week grid has the friend's stamp today
  await expect(card(page, /^Week/).locator('td[data-cell="solved"]')).toHaveCount(1);

  // Giving up finishes it too: now the ranking, the friend's time, the owner unranked
  await page.goto('/');
  await hydrated(page);
  await page.getByRole('button', { name: /^Start/ }).click();
  await page.getByRole('button', { name: /^Give up/ }).click();
  await expect(page.getByRole('heading', { name: 'Solved by the SAT solver' })).toBeVisible();
  await page.goBack();
  await hydrated(page);
  await expect(row(page, friendName)).toHaveAttribute('data-state', 'ranked');
  await expect(row(page, friendName)).toContainText(/\d:\d\d/);
  await expect(card(page, /^Today/).locator('tr[data-state="unranked"]')).toHaveCount(1);

  // The friend leaves: one member left
  await friend.goto(new URL(page.url()).pathname);
  await hydrated(friend);
  friend.on('dialog', (d) => void d.accept());
  await friend.getByRole('button', { name: 'Leave the group' }).click();
  await expect(friend).toHaveURL(/\/groups$/);
  await friendContext.close();
  await page.reload();
  await hydrated(page);
  await expect(card(page, /^Members/).locator('li[data-member]')).toHaveCount(1);
});

test('the seeded group: islands, bridges, a closed board and the week', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, '/groups');
  const item = page.locator(`a[data-group="${SEEDED_GROUP.id}"]`);
  await expect(item).toContainText(SEEDED_GROUP.name);
  await item.click();
  await expect(page.getByRole('heading', { level: 1, name: SEEDED_GROUP.name })).toBeVisible();
  await hydrated(page);
  const islands = page.locator('svg g.nakama-island');
  await expect(islands).toHaveCount(SEEDED_GROUP.members.length);
  // Neighbours who solved the same days have bridges
  expect(await page.locator('svg g[data-tooltip] > g.bridge').count()).toBeGreaterThan(0);
  // aiko hasn't played today (seed.ts): who's done, no times
  await expect(card(page, /^Today/).locator('[data-open="false"]')).toBeVisible();
  await expect(card(page, /^Today/).locator('tr[data-state="ranked"]')).toHaveCount(0);
  await expect(card(page, /^Week/).locator('tbody tr')).toHaveCount(SEEDED_GROUP.members.length);
  await card(page, /^Week/).getByRole('button', { name: 'Earlier week' }).click();
  await expect(card(page, /^Week/).locator('td[data-cell="open"]')).toHaveCount(0);
  await expect(card(page, /^Week/).locator('td[data-cell="solved"]').first()).toBeVisible();
});

test('strangers see no group, and a broken link says so', async ({ page }) => {
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  await expect(page.getByTestId('group-not-found')).toBeVisible();
  await expect(page.getByText(SEEDED_GROUP.name)).toHaveCount(0);
  await visit(page, '/g/nosuchcode');
  await expect(page.getByTestId('invite-invalid')).toBeVisible();
  // The real link shows the group, and joins nobody by being opened
  await visit(page, `/g/${SEEDED_GROUP.invite}`);
  await expect(page.getByRole('heading', { name: `Join “${SEEDED_GROUP.name}”` })).toBeVisible();
  await expect(page.locator('svg g.nakama-island')).toHaveCount(SEEDED_GROUP.members.length);
});
