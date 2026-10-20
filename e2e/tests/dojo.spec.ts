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
  // The dev families and the held-out one, and why
  for (const family of ['Factoring', 'Miters', 'Graph coloring', 'Mystery family']) {
    await expect(page.getByRole('heading', { name: family, exact: true })).toBeVisible();
  }
  await expect(page.getByRole('heading', { name: "Generalize, don't memorize" })).toBeVisible();
  await expect(page.getByText(`Season ${DOJO_SEASON}`, { exact: false }).first()).toBeVisible();
  // Guests are asked for an account first
  await expect(page.getByText("Submitting needs an account with a username, guests can't submit.")).toBeVisible();

  const entry = page.getByRole('link', { name: 'Plain DPLL' });
  await expect(entry).toBeVisible();
  await expect(page.locator('tr').filter({ has: entry })).toContainText('35/48');
  await entry.click();
  await expect(page).toHaveURL(new RegExp(`/dojo/${DOJO_SUBMISSION}$`));
  await expect(page.getByRole('heading', { name: 'Plain DPLL' })).toBeVisible();
  await expect(page.getByText('Unit propagation and nothing else.')).toBeVisible();
});

test('the Sprint track, and the Industrial track coming soon', async ({ page, context }) => {
  // Signed in, so the page has the submit form
  await signIn(context, 'kenji');
  await visit(page, '/dojo');
  await expect(page.getByText('Generated formulas of modest size, about half a second per formula.')).toBeVisible();
  const board = page.locator('#leaderboard');
  await expect(board.getByTestId('sprint-track')).toHaveText('Sprint track');

  const industrial = page.getByTestId('industrial-track');
  await expect(industrial.getByRole('heading', { name: /^Industrial track/ })).toBeVisible();
  await expect(industrial.getByTestId('industrial-soon')).toHaveText('Soon');
  await expect(industrial).toContainText('About 200 times the Sprint budget per formula');
  await expect(industrial).toContainText('Entry will cost a small fee that covers the compute.');
  // Nothing to enter yet: no form, field or button, and the only link is the citation
  await expect(industrial.locator('form, input, textarea, select, button')).toHaveCount(0);
  await expect(industrial.getByRole('link')).toHaveAttribute('href', 'https://benchmark-database.de');
  // The only form on the page enters the Sprint
  await expect(page.locator('form')).toHaveCount(1);
  await expect(page.locator('form')).toHaveAttribute('action', '/dojo/submit');

  // The leaderboard's note leads to the announcement
  await board.getByRole('link', { name: 'Industrial track' }).click();
  await expect(page).toHaveURL(/\/dojo#industrial$/);
  await expect(industrial).toBeInViewport();
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
  // The seed reveals the held-out family
  await expect(archived.getByTestId('mystery-family')).toContainText('Mystery family: Quasigroups');
  const archivedEntry = archived.getByRole('link', { name: 'Plain DPLL' });
  await expect(archivedEntry).toBeVisible();
  // `has` is relative to the row, so a locator from the page
  const dpllLink = page.getByRole('link', { name: 'Plain DPLL' });
  await expect(archived.locator('tr').filter({ has: dpllLink })).toContainText('35/48');
  // The same views and plot as the leaderboard, with every family under its own name
  await expect(archived.getByTestId('cactus').locator('polyline[data-series="vbs"]')).toHaveCount(1);
  await archived.getByRole('tab', { name: 'Contribution' }).click();
  await expect(archived.locator('tr').filter({ has: dpllLink })).toContainText('100.0%');
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
    `${DOJO_SEASON - 1},${DOJO_SUBMISSION},Plain DPLL,Hashi,1,1,3,hashi-01,hashi,SAT,solved,1200000`,
  );
  // Nothing of the running season
  expect((await request.get(`/dojo/seasons/${DOJO_SEASON}/results.csv`)).status()).toBe(404);
});

test('the views beyond PAR-2 and the cactus plot', async ({ page }) => {
  await visit(page, '/dojo');
  const board = page.locator('#leaderboard');
  // Relative to the page: `has` looks inside the row
  const dpll = page.getByRole('link', { name: 'Plain DPLL' });
  const cdcl = page.getByRole('link', { name: 'Plain CDCL' });
  await expect(board.getByRole('link', { name: 'Plain CDCL' })).toBeVisible();
  // The official ranking first
  await expect(board.getByRole('tab', { name: 'Ranking' })).toHaveAttribute('aria-selected', 'true');
  await expect(board.locator('tbody tr').first()).toContainText('Plain DPLL');

  // SAT/UNSAT: the mystery family's formulas stay out of the split while the season runs
  await board.getByRole('tab', { name: 'SAT / UNSAT' }).click();
  await expect(board.getByText(/the mystery family only counts in ALL/)).toBeVisible();
  await expect(board.getByRole('columnheader', { name: /^UNSAT$/ })).toBeVisible();

  // Contributions, sortable: each solves formulas the other doesn't
  await board.getByRole('tab', { name: 'Contribution' }).click();
  const shapley = board.getByRole('columnheader', { name: /Shapley/ });
  await expect(shapley).toHaveAttribute('aria-sort', 'none');
  await shapley.getByRole('button').click();
  await expect(shapley).toHaveAttribute('aria-sort', 'descending');
  await expect(board.locator('tr').filter({ has: dpll })).toContainText('%');
  await expect(board.getByTestId('vbs')).toContainText('Virtual best solver of 2 solvers');

  // The plot: a line per solver and the VBS, hovering a row highlights its line
  const cactus = board.getByTestId('cactus');
  await expect(cactus.locator('polyline[data-curve]')).toHaveCount(2);
  await expect(cactus.locator('polyline[data-series="vbs"]')).toHaveCount(1);
  await expect(cactus.getByRole('img')).toHaveAttribute('aria-label', /^Cactus plot of 2 solvers/);
  await board.locator('tr').filter({ has: cdcl }).hover();
  await expect(cactus.locator('polyline[data-series="highlight"]')).toHaveCount(1);
  // The crosshair and its tooltip, by keyboard
  await cactus.getByRole('img').focus();
  await page.keyboard.press('End');
  await expect(cactus.getByText(/formulas solved$/)).toBeVisible();
  await cactus.getByText('Show as a table').click();
  await expect(cactus.locator('tr').filter({ hasText: 'Virtual best solver' })).toBeVisible();

  // The submission's page: its split, its contribution and its line in the plot
  await dpll.click();
  await expect(page).toHaveURL(new RegExp(`/dojo/${DOJO_SUBMISSION}$`));
  await expect(page.getByTestId('answers')).toContainText('All families');
  await expect(page.getByTestId('answers')).toContainText('Mystery family');
  await expect(page.getByTestId('contribution')).toContainText('Shapley');
  await expect(page.getByTestId('cactus').locator('polyline[data-series="highlight"]')).toHaveCount(1);
});
