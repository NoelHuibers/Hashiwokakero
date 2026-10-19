// The daily puzzle from the cover to the leaderboard, as a guest and with an account made on the
// way. Every test plays as a new player: there is one daily a day per player.
import type { Locator, Page } from '@playwright/test';

import { register, signInWith, signOut } from '../account';
import { DAILY_KEY, saved } from '../board';
import { expectResult, solveDaily, startDaily } from '../daily';
import { expect, newUsername, test, visit } from '../fixtures';

test('a guest solves the daily and is on the leaderboard', async ({ page, context }) => {
  await visit(page, '/');
  await expect(page.getByText("No account needed, you'll play as a guest.")).toBeVisible();
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);

  const card = await expectResult(page);
  const name = (await card.locator('b').textContent())!;
  expect(name).toMatch(/^\p{Lu}\p{Ll}+ \p{Lu}\p{Ll}+ \d+$/u);
  const time = (await card.locator('p.font-pixel.text-3xl').textContent())!;

  // Today's ranking next to the board, with the penalties
  const mine = (root: Locator) => root.locator('tr').filter({ hasText: name });
  await expect(mine(page.locator('aside'))).toContainText(/you/i);
  await expect(mine(page.locator('aside'))).toContainText(/\d+h 1c/);
  await expect(mine(page.locator('aside'))).toContainText(time);

  // Finished is finished: after a reload, and in another tab
  const finished = async (p: Page) => {
    await expect(p.getByRole('heading', { name: /Well done!/ })).toBeVisible();
    await expect(p.locator('p.font-pixel.text-3xl')).toHaveText(time);
    await expect(p.getByRole('button', { name: /^Start/ })).toHaveCount(0);
    for (const button of [/^Hint/, /^Check/, /^Give up/, /Undo/]) {
      await expect(p.getByRole('button', { name: button })).toBeDisabled();
    }
    expect((await saved(p, DAILY_KEY)).bridges).toEqual(solution);
  };
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await finished(page);
  const other = await context.newPage();
  await visit(other, '/');
  await finished(other);

  // The ranks page has the same row in today's board, the first one
  await visit(other, '/ranks');
  const today = other.locator('main table').first();
  await expect(mine(today)).toContainText(/you/i);
  await expect(mine(today)).toContainText(time);
  await other.close();
});

test('sign up, sign out and in again, then the daily counts for the account', async ({ page }) => {
  const username = newUsername('daily');
  const password = 'bridges-all-the-way';
  await visit(page, '/account');
  await register(page, username, password);
  await expect(page.getByText(`Welcome, ${username}!`)).toBeVisible();
  await signOut(page);
  await signInWith(page, username, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();

  await page.getByRole('link', { name: /Daily/ }).first().click();
  await expect(page.getByText(`Playing as ${username}`, { exact: true })).toBeVisible();
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);
  const card = await expectResult(page);
  // Not a guest any more: nothing to claim
  await expect(card.getByText(/Claim your name/)).toHaveCount(0);
  await expect(page.getByText('New stamp: First bridge!')).toBeVisible();

  // The account has the result and the stamp
  await visit(page, '/account');
  await expect(page.getByText('No dailies yet.')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Stamp book' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'First bridge' })).toBeVisible();
  await expect(page.getByText(/^\d+ of \d+ stamps collected\./)).not.toHaveText(/^0 of/);

  // And the leaderboard has the name, without the guest label
  await visit(page, '/ranks');
  const row = page.locator('main table').first().locator('tr').filter({ hasText: username });
  await expect(row).toContainText(/you/i);
  await expect(row).not.toContainText('guest');
});
