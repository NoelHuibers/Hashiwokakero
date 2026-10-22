// The app's client-side rendered bundle in a browser: it starts, renders pages from the server
// functions, routes on its own, and takes the language from the path.
import { board } from '../board';
import { expect, hydrated, test, visit } from '../fixtures';

test('the practice board shows a generated puzzle', async ({ page }) => {
  await visit(page, '/practice');
  await expect(board(page).locator('g.island').first()).toBeVisible();
});

test('the router opens the rules without a page load', async ({ page }) => {
  await visit(page, '/');
  await page.evaluate(() => Object.assign(window, { samePage: true }));
  await page.locator('header').getByRole('link', { name: 'How to play' }).click();
  await expect(page).toHaveURL(/\/rules$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('How to play Hashiwokakero');
  expect(await page.evaluate(() => 'samePage' in window)).toBe(true);
});

test('a language from the path, also after a reload', async ({ page }) => {
  await visit(page, '/ja/rules');
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('橋をかけろの遊び方');
  // Every path is index.html, the bundle renders the page of the URL
  await page.reload();
  await hydrated(page);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('橋をかけろの遊び方');
});

test('the saved theme applies before the app starts', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('hashi.theme', 'dark'));
  await visit(page, '/rules');
  await expect(page.locator('html')).toHaveClass(/\bdark\b/);
});
