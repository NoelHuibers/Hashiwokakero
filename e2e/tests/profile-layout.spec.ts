// The account page and the players' profiles leave no empty holes between their cards, on a
// computer, a tablet and a phone, whoever looks: signed out, a guest, a player without dailies
// and one with a record and stamps. The account page's cards once stood in a grid whose rows
// were as tall as their taller card, which left the space beside the shorter one empty.
// German has some of the longest texts, which put the columns furthest out of balance.
import type { Page } from '@playwright/test';
import { expect, signIn, test, visit } from '../fixtures.ts';
import { expectNoHoles } from '../layout.ts';
import { player } from '../seed.ts';
import { localized } from '../site.ts';

const WIDTHS = [
  ['computer', 1280],
  ['small computer', 1024],
  ['tablet', 820],
  ['phone', 390],
] as const;

/** The page at every width, after it has resized. */
async function atEveryWidth(page: Page, path: string) {
  for (const [device, width] of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    // The layout follows the new width before the next frame
    await page.evaluate(() => new Promise(requestAnimationFrame));
    await expectNoHoles(
      page.locator('main section').first(),
      `${path} on a ${device} (${width} px)`,
    );
  }
}

test('the account page, signed out', async ({ page }) => {
  await visit(page, '/account');
  await expect(page.getByRole('heading', { name: 'Your account' })).toBeVisible();
  await atEveryWidth(page, '/account');
});

for (const [who, name] of [
  ['a guest', 'Drowsy Otter 21'],
  ['a player without dailies', 'kenji'],
  ['a player with a record and stamps', 'aiko'],
] as const) {
  test(`the account page and own profile of ${who}`, async ({ page, context }) => {
    await signIn(context, name);
    await visit(page, '/account');
    await expect(page.locator('#notifications')).toBeVisible();
    await atEveryWidth(page, '/account');

    await visit(page, `/player/${player(name).id}`);
    await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible();
    await atEveryWidth(page, '/player');
  });
}

test("another player's profile", async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, `/player/${player('chen').id}`);
  await expect(page.getByRole('heading', { name: 'chen', level: 1 })).toBeVisible();
  await atEveryWidth(page, '/player');
});

for (const [who, name] of [
  ['a guest', 'Drowsy Otter 21'],
  ['a player with a record and stamps', 'aiko'],
] as const) {
  test(`the account page of ${who} in German`, async ({ page, context }) => {
    await signIn(context, name);
    await visit(page, localized('de', '/account'));
    await expect(page.locator('#notifications')).toBeVisible();
    await atEveryWidth(page, '/de/account');
  });
}
