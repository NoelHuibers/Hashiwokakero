// The SAT Dojo without its worker (pull requests have none): the guide, the seeded house entry
// on the leaderboard, and an upload the site itself refuses.
import { expect, signIn, test, visit } from '../fixtures';
import { ARCHIVED_COMMITMENT, ARCHIVED_SEED, DOJO_SEASON, DOJO_SUBMISSION } from '../seed';

test('the guide and the leaderboard with its entry', async ({ page }) => {
  await visit(page, '/dojo');
  await expect(page.getByRole('heading', { name: 'SAT Dojo', level: 1 })).toBeVisible();
  for (const heading of ['How it works', 'The formulas', 'Fair play']) {
    await expect(page.getByText(new RegExp(`^${heading}`)).first()).toBeVisible();
  }
  await expect(page.getByText(`Season ${DOJO_SEASON}`, { exact: false }).first()).toBeVisible();
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

test('the season archive with its seed, set and results', async ({ page, request }) => {
  await visit(page, '/dojo');
  await page.getByRole('link', { name: 'Season archive' }).click();
  await expect(page).toHaveURL(/\/dojo\/seasons$/);
  await expect(page.getByRole('heading', { name: 'Season archive', level: 1 })).toBeVisible();

  // The running season shows its commitment, not its seed
  const current = page.locator('#current-season');
  await expect(current).toContainText(`Season ${DOJO_SEASON} · running`);
  await expect(current).toContainText('The seed stays secret until the season ends');

  const archived = page.locator(`#season-${DOJO_SEASON - 1}`);
  await expect(archived).toContainText(ARCHIVED_COMMITMENT);
  await expect(archived).toContainText(ARCHIVED_SEED);
  await expect(archived.getByRole('link', { name: 'Plain DPLL' })).toBeVisible();
  await expect(archived.locator('tr').filter({ hasText: 'Plain DPLL' })).toContainText('29/34');
  for (const [name, path] of [
    [/^Hidden set/, 'set.tar.gz'],
    ['Results per formula (CSV)', 'results.csv'],
    ['Results per formula (JSON)', 'results.json'],
  ] as const) {
    await expect(archived.getByRole('link', { name })).toHaveAttribute(
      'href',
      `/dojo/seasons/${DOJO_SEASON - 1}/${path}`,
    );
  }
  await expect(page.getByRole('heading', { name: /^How to verify/ })).toBeVisible();

  const csv = await request.get(`/dojo/seasons/${DOJO_SEASON - 1}/results.csv`);
  expect(csv.status()).toBe(200);
  expect((await csv.text()).split('\n')[1]).toBe(
    `${DOJO_SEASON - 1},${DOJO_SUBMISSION},Plain DPLL,Hashi,1,1,1,hashi-01,hashi,SAT,solved,1200000`,
  );
  // Nothing of the running season
  expect((await request.get(`/dojo/seasons/${DOJO_SEASON}/results.csv`)).status()).toBe(404);
});
