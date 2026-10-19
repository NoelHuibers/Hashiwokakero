// The SAT Dojo without its worker (pull requests have none): the guide, the seeded house entry
// on the leaderboard, and an upload the site itself refuses.
import { expect, signIn, test, visit } from '../fixtures';
import { DOJO_SUBMISSION } from '../seed';

test('the guide and the leaderboard with its entry', async ({ page }) => {
  await visit(page, '/dojo');
  await expect(page.getByRole('heading', { name: 'SAT Dojo', level: 1 })).toBeVisible();
  for (const heading of ['How it works', 'The formulas', 'Fair play']) {
    await expect(page.getByText(new RegExp(`^${heading}`)).first()).toBeVisible();
  }
  await expect(page.getByText('Season 1', { exact: false }).first()).toBeVisible();
  // Guests are asked for an account first
  await expect(page.getByText("Submitting needs an account with a username, guests can't submit.")).toBeVisible();

  const entry = page.getByRole('link', { name: 'Plain DPLL' });
  await expect(entry).toBeVisible();
  await expect(page.locator('tr').filter({ has: entry })).toContainText('29/34');
  await entry.click();
  await expect(page).toHaveURL(new RegExp(`/dojo/${DOJO_SUBMISSION}$`));
  await expect(page.getByRole('heading', { name: 'Plain DPLL' })).toBeVisible();
  await expect(page.getByText('Unit propagation and nothing else.')).toBeVisible();
});

test('an upload that is no WebAssembly is refused', async ({ page, context }) => {
  await signIn(context, 'kenji');
  await visit(page, '/dojo');
  await expect(page.getByText(/^Submissions left today: \d+$/)).toBeVisible();
  await page.getByLabel('Name of your solver').fill('Not a solver');
  await page.getByLabel(/^WebAssembly file/).setInputFiles({
    name: 'solver.wasm',
    mimeType: 'application/wasm',
    buffer: Buffer.from('#!/bin/sh\necho "s SATISFIABLE"\n'),
  });
  await page.getByRole('button', { name: /^Submit 入門/ }).click();
  await expect(page).toHaveURL(/\/dojo\?error=not_wasm#submit$/);
  await expect(page.getByText('That is not a WebAssembly file.')).toBeVisible();
  // Nothing was stored
  await expect(page.getByText('Nothing submitted yet.')).toBeVisible();
});
