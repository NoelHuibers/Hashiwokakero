// The Lab: one section with a tab per page, the race of solver presets on a puzzle and the SAT
// Dojo's pages. The tabs are links (deep links, the back button), the Dojo's old addresses
// redirect to them, and the players' pages lead there.
import type { Page } from '@playwright/test';

import { expect, test, visit } from '../fixtures.ts';
import { DOJO_SUBMISSION } from '../seed.ts';

/** The Lab's tabs, in English. */
const tabs = (page: Page) => page.getByRole('navigation', { name: 'Lab sections', exact: true });

test('race the default solvers on a fresh puzzle', async ({ page }) => {
  await visit(page, '/lab');
  // Without a game in this browser the lab generates a Medium one
  await expect(page.getByText(/^10×10, \d+ islands$/)).toBeVisible();
  await page.getByRole('button', { name: 'Easy', exact: true }).click();
  await expect(page.getByText(/^7×7, \d+ islands$/)).toBeVisible();

  await page.getByRole('button', { name: /^Start the race/ }).click();
  await expect(page.getByRole('button', { name: 'Racing…' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Start the race/ })).toBeVisible({
    timeout: 30_000,
  });

  const rows = page.locator('table tbody tr');
  // The presets selected by default (src/pages/lab/race.rs)
  await expect(rows).toHaveCount(8);
  for (const row of await rows.all()) {
    await expect(row).toContainText('SAT');
    await expect(row).toContainText(/\d+(\.\d+)? (ms|s)/);
  }
});

test('the tabs are pages: one header, deep links and the back button', async ({ page }) => {
  await visit(page, '/lab');
  const nav = tabs(page);
  await expect(nav.getByRole('link')).toHaveText([
    /Race/,
    /Leaderboard/,
    /Submit/,
    /Benchmarks/,
    /Seasons/,
  ]);
  await expect(nav.getByRole('link', { name: /Race/ })).toHaveAttribute('aria-current', 'page');

  await nav.getByRole('link', { name: /Leaderboard/ }).click();
  await expect(page).toHaveURL(/\/lab\/leaderboard$/);
  await expect(page.getByRole('heading', { name: /^SAT Dojo/, level: 2 })).toBeVisible();
  await expect(nav.getByRole('link', { name: /Leaderboard/ })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(nav.getByRole('link', { name: /Race/ })).not.toHaveAttribute('aria-current');
  await nav.getByRole('link', { name: /Seasons/ }).click();
  await expect(page).toHaveURL(/\/lab\/seasons$/);
  await expect(page.getByRole('heading', { name: /^Season archive/, level: 2 })).toBeVisible();
  // One header for the whole section, the same on every tab
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Solver lab');
  await expect(page).toHaveTitle('Season archive · SAT Dojo');

  await page.goBack();
  await expect(page).toHaveURL(/\/lab\/leaderboard$/);
  await expect(page.getByRole('heading', { name: /^SAT Dojo/, level: 2 })).toBeVisible();
  await page.goBack();
  await expect(page).toHaveURL(/\/lab$/);
  await expect(page.getByRole('button', { name: /^Start the race/ })).toBeVisible();
  await page.goForward();
  await expect(page).toHaveURL(/\/lab\/leaderboard$/);

  // A link to a tab in another language opens it, as the page search engines know
  await visit(page, '/de/lab/submit');
  const german = page.getByRole('navigation', { name: 'Bereiche des Labors', exact: true });
  await expect(german.getByRole('link', { name: /Einreichen/ })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByRole('heading', { name: /^So funktioniert/ })).toBeVisible();
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
    'href',
    'https://hashi.rs/de/lab/submit',
  );
  await expect(page).toHaveTitle(/^SAT-Solver einreichen/);
});

test('on a phone the tabs scroll sideways, the selected one in view', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 640 });
  await visit(page, '/lab/seasons');
  await expect(tabs(page).getByRole('link', { name: /Seasons/ })).toBeInViewport({ ratio: 1 });
  // The tabs scroll, not the page
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('the solver panels of Play and the workshop lead to the race', async ({ page }) => {
  await visit(page, '/create');
  await page.getByTestId('lab-link').click();
  await expect(page).toHaveURL(/\/lab\?puzzle=workshop$/);
  // The workshop's puzzle, not a fresh one
  await expect(page.getByRole('button', { name: 'Workshop', exact: true })).toHaveClass(
    /bg-primary/,
  );
  await visit(page, '/rules');
  await page.getByRole('link', { name: 'Race solvers in the Lab →' }).click();
  await expect(page).toHaveURL(/\/lab$/);
});

test.describe('the Dojo’s old addresses', () => {
  test('redirect for good on the server, with the query', async ({ page, request }) => {
    for (const [from, to] of [
      ['/dojo', '/lab/leaderboard'],
      ['/dojo?budget=4', '/lab/leaderboard?budget=4'],
      ['/ja/dojo/', '/ja/lab/leaderboard'],
      ['/de/dojo/seasons', '/de/lab/seasons'],
      ['/dojo/benchmarks?submitted=3', '/lab/benchmarks?submitted=3'],
      [`/dojo/${DOJO_SUBMISSION}`, `/lab/solver/${DOJO_SUBMISSION}`],
      [`/zh-Hant/dojo/${DOJO_SUBMISSION}`, `/zh-Hant/lab/solver/${DOJO_SUBMISSION}`],
    ] as const) {
      const res = await request.get(from, { maxRedirects: 0 });
      expect(res.status(), from).toBe(301);
      expect(res.headers().location, from).toBe(to);
    }
    // The Dojo's files stay where the CLI and the worker find them
    expect((await request.get('/dojo/board.json', { maxRedirects: 0 })).status()).toBe(200);
    await visit(page, '/ja/dojo?budget=4');
    await expect(page).toHaveURL(/\/ja\/lab\/leaderboard\?budget=4$/);
    await expect(page.locator('a[data-budget="4"]')).toHaveAttribute('aria-current', 'true');
  });

  test('lead to the Lab in the router of a page, from a link of an older build', async ({
    page,
  }) => {
    await visit(page, '/rules');
    await page.evaluate((id) => {
      const link = Object.assign(document.createElement('a'), {
        href: `/dojo/${id}`,
        id: 'old-link',
        textContent: 'Plain DPLL',
      });
      document.querySelector('main')!.append(link);
      Object.assign(window, { sameDocument: true });
    }, DOJO_SUBMISSION);
    await page.locator('#old-link').click();
    await expect(page).toHaveURL(new RegExp(`/lab/solver/${DOJO_SUBMISSION}$`));
    await expect(page.getByRole('heading', { name: 'Plain DPLL' })).toBeVisible();
    // The router's, not a page load
    expect(
      await page.evaluate(
        () => (window as unknown as { sameDocument?: boolean }).sameDocument === true,
      ),
    ).toBe(true);
    // In place of the old address: back is the page before it
    await page.goBack();
    await expect(page).toHaveURL(/\/rules$/);
  });
});
