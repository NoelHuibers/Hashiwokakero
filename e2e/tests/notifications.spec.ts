// The account's notification settings: the master switch and one per kind, saved with the
// account, so a reload keeps them. Nothing reaches a device yet, and the card says so.
import type { Page } from '@playwright/test';

import { register } from '../account.ts';
import { expect, hydrated, newUsername, test, visit } from '../fixtures.ts';

const KINDS = ['Streak last call', 'Races', 'Rivals', 'Your shared puzzles', 'Group puzzles'];

const card = (page: Page) => page.getByTestId('notification-settings');
const master = (page: Page) => card(page).getByRole('switch', { name: 'Notifications' });
const kind = (page: Page, name: string) => card(page).getByRole('switch', { name });

/** The switches as `[master, ...kinds]`, on or off. */
async function expectSwitches(page: Page, on: boolean[]) {
  const all = [master(page), ...KINDS.map((name) => kind(page, name))];
  for (const [i, toggle] of all.entries()) {
    await expect(toggle).toHaveAttribute('aria-checked', String(on[i]));
  }
}

/** Reloads and waits until the saved settings are on the switches again. */
async function reload(page: Page) {
  await page.reload();
  await hydrated(page);
  await expect(master(page)).toBeEnabled();
}

test('the switches are saved with the account', async ({ page }) => {
  await visit(page, '/account');
  await register(page, newUsername('notify'), 'bells-over-bridges');
  await expect(card(page)).toBeVisible();
  await expect(card(page).getByTestId('notifications-unavailable')).toBeVisible();
  await expect(master(page)).toBeEnabled();
  // Off until chosen
  await expectSwitches(page, [false, false, false, false, false, false]);

  // The master switch with nothing chosen: everything
  await master(page).click();
  await expectSwitches(page, [true, true, true, true, true, true]);
  await kind(page, 'Races').click();
  await expectSwitches(page, [true, true, false, true, true, true]);
  await reload(page);
  await expectSwitches(page, [true, true, false, true, true, true]);

  // Off keeps the choice, on again brings it back
  await master(page).click();
  await expectSwitches(page, [false, true, false, true, true, true]);
  await reload(page);
  await expectSwitches(page, [false, true, false, true, true, true]);
  await master(page).click();
  await expectSwitches(page, [true, true, false, true, true, true]);

  // The last kind off turns the master off, a kind on turns it on again
  for (const name of ['Streak last call', 'Rivals', 'Your shared puzzles', 'Group puzzles']) {
    await kind(page, name).click();
  }
  await expectSwitches(page, [false, false, false, false, false, false]);
  await kind(page, 'Rivals').click();
  await expectSwitches(page, [true, false, false, true, false, false]);
  await reload(page);
  await expectSwitches(page, [true, false, false, true, false, false]);
});

test('the settings pop-up leads to them', async ({ page }) => {
  await visit(page, '/account');
  await register(page, newUsername('bell'), 'bells-over-bridges');
  await visit(page, '/rules');
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByRole('link', { name: /^Notifications/ }).click();
  await expect(page).toHaveURL(/\/account#notifications$/);
  await expect(card(page)).toBeVisible();
});
