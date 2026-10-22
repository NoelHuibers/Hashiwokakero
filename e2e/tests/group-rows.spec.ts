// The rows of a group's board of today on a phone: with the medal, the time, the seals, the
// penalties and the ▶ of a duel, a short name like "chen" still shows whole, on the group page
// and in the group's tab on the daily page. A name that's cut off has its tooltip.
import type { Locator, Page } from '@playwright/test';

import { expect, test, visit } from '../fixtures';
import { TEA_GROUP } from '../seed';

const PHONE = { width: 390, height: 844 };
const SMALL_PHONE = { width: 320, height: 640 };

/** The name links of a board's rows. */
const names = (board: Locator) => board.locator('tr [data-name-line] a[data-tooltip-clipped]');

/** Whether an element's text is cut off. */
const clipped = (el: Locator) => el.evaluate((a) => a.scrollWidth > a.clientWidth);

/** chen's row (seed.ts: ranked first today, with a check) shows all of its parts, and the name. */
async function chenWhole(page: Page, board: Locator) {
  await expect(board).toHaveAttribute('data-open', 'true');
  const row = board.locator('tr').filter({ has: page.getByRole('link', { name: 'chen', exact: true }) });
  await expect(row).toHaveAttribute('data-state', 'ranked');
  await expect(row.locator('[data-penalties]')).toHaveText('0h 1c');
  await expect(row.locator('a[data-duel]')).toBeVisible();
  await expect(row.locator('button[data-hanko-pick]')).toBeVisible();
  const name = row.getByRole('link', { name: 'chen', exact: true });
  expect(await clipped(name), 'chen is cut off').toBe(false);
  // Room for more than "chen": about a dozen letters
  expect((await row.locator('[data-name-line]').boundingBox())!.width).toBeGreaterThan(100);
  // Nothing sticks out of the row
  const rowBox = (await row.boundingBox())!;
  for (const part of ['[data-penalties]', 'a[data-duel]', 'button[data-hanko-pick]']) {
    const box = (await row.locator(part).boundingBox())!;
    expect(box.x + box.width, part).toBeLessThanOrEqual(rowBox.x + rowBox.width + 0.5);
  }
}

test('short names stay whole on a phone, on the group page and in the daily tab', async ({ page }) => {
  test.slow();
  page.on('dialog', (d) => void d.accept());

  // A visitor joins the seeded tea group (becoming a guest) and gives up today's daily: the
  // board opens, with chen's and bram's times
  await visit(page, `/g/${TEA_GROUP.invite}`);
  await page.getByRole('button', { name: /^Join the group/ }).click();
  await expect(page).toHaveURL(new RegExp(`/groups/${TEA_GROUP.id}$`));
  await visit(page, '/');
  await page.getByRole('button', { name: /^Start/ }).click();
  await page.getByRole('button', { name: /^Give up/ }).click();
  await expect(page.getByRole('heading', { name: 'Solved by the SAT solver' })).toBeVisible();

  // The group page, on a phone and a small one
  const board = page.locator(`[data-board="${TEA_GROUP.id}"]`);
  for (const size of [PHONE, SMALL_PHONE]) {
    await page.setViewportSize(size);
    await visit(page, `/groups/${TEA_GROUP.id}`);
    await chenWhole(page, board);
    // The picker still opens under the row, as wide as the board
    const chen = board.locator('tr').filter({ has: page.getByRole('link', { name: 'chen', exact: true }) });
    await chen.locator('button[data-hanko-pick]').click();
    await expect(page.getByTestId('hanko-tray').getByRole('button')).toHaveCount(6);
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('hanko-tray')).toHaveCount(0);
    // And the board stays on the screen
    const box = (await board.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(size.width);
  }

  // The group's tab on the daily page
  await page.setViewportSize(PHONE);
  await visit(page, '/');
  await page.getByRole('tab', { name: new RegExp(TEA_GROUP.name) }).click();
  await chenWhole(page, board);

  // A name that is cut off shows whole in its tooltip, one that fits has none: narrowed so the
  // guest's (two words and a number) is cut off and chen still fits
  await page.addStyleTag({ content: 'a[data-tooltip-clipped] { max-width: 2.75rem }' });
  const tooltip = page.getByRole('tooltip');
  const [mine] = await names(board).filter({ hasNotText: /^(chen|bram)$/ }).all();
  const myName = (await mine.textContent())!.trim();
  expect(await clipped(mine)).toBe(true);
  await mine.hover();
  await expect(tooltip).toBeVisible();
  await expect(tooltip).toHaveText(myName);
  const chen = page.getByRole('link', { name: 'chen', exact: true });
  expect(await clipped(chen)).toBe(false);
  await chen.hover();
  await expect(tooltip).toBeHidden();
});
