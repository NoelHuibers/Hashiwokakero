// The solver lab: a fresh puzzle raced through the solver presets, every one finds a solution.
import { expect, test, visit } from '../fixtures';

test('race the default solvers on a fresh puzzle', async ({ page }) => {
  await visit(page, '/lab');
  // Without a game in this browser the lab generates a Medium one
  await expect(page.getByText(/^10×10, \d+ islands$/)).toBeVisible();
  await page.getByRole('button', { name: 'Easy', exact: true }).click();
  await expect(page.getByText(/^7×7, \d+ islands$/)).toBeVisible();

  await page.getByRole('button', { name: /^Start the race/ }).click();
  await expect(page.getByRole('button', { name: 'Racing…' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Start the race/ })).toBeVisible({ timeout: 30_000 });

  const rows = page.locator('table tbody tr');
  // The presets selected by default (src/pages/lab/mod.rs)
  await expect(rows).toHaveCount(8);
  for (const row of await rows.all()) {
    await expect(row).toContainText('SAT');
    await expect(row).toContainText(/\d+(\.\d+)? (ms|s)/);
  }
});
