// Community benchmarks ("bring your own benchmark") without a worker: the rules and the pool,
// the credits in the guide and the season archive, and a submission that waits for its check.
import { expect, signIn, test, visit } from '../fixtures';
import { COMMUNITY_FAMILY, DOJO_SEASON } from '../seed';

test('the pool and its authors are credited', async ({ page }) => {
  await visit(page, '/dojo');
  const community = page.getByTestId('community-family');
  await expect(community).toContainText('Contributed by: aiko');
  await community.getByRole('link', { name: 'Bring your own benchmark' }).click();
  await expect(page).toHaveURL(/\/dojo\/benchmarks$/);
  await expect(page.getByRole('heading', { name: 'Bring your own benchmark', level: 1 })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'What makes it in' })).toBeVisible();
  const pool = page.locator('#pool');
  await expect(pool).toContainText(COMMUNITY_FAMILY);
  await expect(pool).toContainText('by aiko');
  // Guests are asked for an account first
  await expect(page.getByText("Submitting needs an account with a username, guests can't submit.")).toBeVisible();

  await visit(page, '/dojo/seasons');
  const archived = page.locator(`#season-${DOJO_SEASON - 1}`).getByTestId('season-community');
  await expect(archived).toContainText(COMMUNITY_FAMILY);
  await expect(archived).toContainText('by aiko');
});

test('a submitted benchmark waits for verification', async ({ page, context }) => {
  await signIn(context, 'kenji');
  await visit(page, '/dojo/benchmarks');
  await expect(page.getByText(/^Benchmarks left today: \d+$/)).toBeVisible();
  // A formula of its own every run: the database is shared and duplicates are refused. Below
  // dojo::community::MAX_VARS (200,000), or it is refused as too large
  const vars = 2 + Math.floor(Math.random() * 190_000);
  await page.getByLabel(/^DIMACS file/).setInputFiles({
    name: 'units.cnf',
    mimeType: 'text/plain',
    buffer: Buffer.from(`p cnf ${vars} 1\n${vars} 0\n`),
  });
  await page.getByLabel('Family', { exact: true }).fill('e2e-units');
  await page.getByLabel(/^I made this formula/).check();
  await page.getByRole('button', { name: /^Submit benchmark/ }).click();
  await expect(page).toHaveURL(/\/dojo\/benchmarks\?submitted=\d+#mine$/);
  await expect(page.getByText('Thanks! Your benchmark is queued for verification.')).toBeVisible();
  const mine = page.getByTestId('my-benchmark').filter({ hasText: 'e2e-units' }).first();
  await expect(mine).toContainText('Waiting for verification');
  await expect(mine).toContainText(`${vars} variables · 1 clauses`);
});
