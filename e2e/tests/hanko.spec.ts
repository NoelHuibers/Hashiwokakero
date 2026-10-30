// Hanko 判子: two members of a group finish the daily, one presses a seal on the other's result,
// and the other finds it on the group page, with an unread dot until they saw it.
import type { BrowserContext, Page } from '@playwright/test';

import { expect, hydrated, test, visit } from '../fixtures';

/** A card of the page, by its heading. */
const card = (page: Page, heading: RegExp) =>
  page.locator('[data-name="Card"]').filter({ has: page.getByRole('heading', { name: heading }) });

/** Finishes today's daily the quick way: giving up. */
async function giveUp(page: Page) {
  await visit(page, '/');
  await page.getByRole('button', { name: /^Start/ }).click();
  await page.getByRole('button', { name: /^Give up/ }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Give up' }).click();
  await expect(page.getByRole('heading', { name: 'Solved by the SAT solver' })).toBeVisible();
}

/** A second visitor, in a browser of their own. */
async function newVisitor(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  return page;
}

test('a member stamps another one’s result, who sees it with an unread dot', async ({
  page,
  browser,
  baseURL,
}) => {
  test.slow();

  // A guest makes a group, a friend joins with its link
  await visit(page, '/groups');
  await page.getByLabel('Group name').fill('Hanko Club');
  await page.getByRole('button', { name: 'Create the group' }).click();
  const url = await page.getByTestId('invite-link').inputValue();
  const friendContext = await browser.newContext({ baseURL });
  const friend = await newVisitor(friendContext);
  await friend.goto(new URL(url).pathname);
  await hydrated(friend);
  await friend.getByRole('button', { name: /^Join the group/ }).click();
  await expect(friend).toHaveURL(/\/groups\/\d+$/);
  const groupPath = new URL(friend.url()).pathname;
  const friendName = (await card(friend, /^Members/).locator('li[data-member] a').nth(1).innerText()).trim();

  // Before finishing, nobody can stamp anything
  await visit(page, groupPath);
  await expect(card(page, /^Today/).locator('[data-open="false"]')).toBeVisible();
  await expect(page.locator('button[data-hanko-pick]')).toHaveCount(0);

  // Both finish
  await giveUp(friend);
  await giveUp(page);

  // The owner stamps the friend's result: the 判 button on their row opens the picker
  await visit(page, groupPath);
  const board = card(page, /^Today/);
  await expect(board.locator('[data-open="true"]')).toBeVisible();
  // Not on one's own result
  await expect(board.locator('button[data-hanko-pick]')).toHaveCount(1);
  const pick = board.getByRole('button', { name: `Stamp ${friendName}'s result with a hanko` });
  await pick.click();
  const tray = page.getByTestId('hanko-tray');
  await expect(tray).toBeVisible();
  await expect(tray.getByRole('button')).toHaveCount(6);
  // Escape closes it, back on the button
  await page.keyboard.press('Escape');
  await expect(tray).toHaveCount(0);
  await expect(pick).toBeFocused();
  // Open again with the keyboard, go along to 凄 and press it
  await page.keyboard.press('Enter');
  await expect(tray.getByRole('button', { name: 'Fast' })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(tray.getByRole('button', { name: 'Amazing' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(tray).toHaveCount(0);
  const seal = board.locator('.hanko-pill[data-hanko="wow"]');
  await expect(seal).toHaveAttribute('aria-pressed', 'true');
  await expect(seal).toHaveAccessibleName('Amazing: you');
  // And 祝 with the mouse
  await pick.click();
  await tray.getByRole('button', { name: 'Congrats' }).click();
  await expect(board.locator('.hanko-pill')).toHaveCount(2);
  // Pressed again it's taken back
  await board.locator('.hanko-pill[data-hanko="cheers"]').click();
  await expect(board.locator('.hanko-pill')).toHaveCount(1);
  // The server kept it
  await page.reload();
  await hydrated(page);
  await expect(card(page, /^Today/).locator('.hanko-pill[data-hanko="wow"]')).toHaveAttribute('aria-pressed', 'true');

  // The friend has a dot on the groups' entry and a badge on the group in the list
  await visit(friend, '/groups');
  await expect(friend.locator('header nav [data-testid="hanko-dot"]')).toHaveCount(1);
  await expect(friend.getByTestId('hanko-badge')).toContainText('1');
  // On the group page: the seal on their row (not a button, it's their own) and the card of
  // the seals they got, new
  await friend.locator('a[data-group]').filter({ hasText: 'Hanko Club' }).click();
  await expect(friend).toHaveURL(/\/groups\/\d+$/);
  const got = friend.getByTestId('hanko-card');
  await expect(got.getByTestId('hanko-received')).toHaveCount(1);
  await expect(got.getByTestId('hanko-received')).toHaveAttribute('data-fresh', 'true');
  await expect(got).toContainText('stamped your result: Amazing');
  await expect(card(friend, /^Today/).locator('span.hanko-pill[data-hanko="wow"]')).toBeVisible();
  // Seen: the dot is gone
  await expect(friend.locator('[data-testid="hanko-dot"]')).toHaveCount(0);
  await friend.reload();
  await hydrated(friend);
  await expect(friend.getByTestId('hanko-received')).toHaveAttribute('data-fresh', 'false');
  await expect(friend.locator('[data-testid="hanko-dot"]')).toHaveCount(0);

  // On their daily result too
  await visit(friend, '/');
  await expect(friend.getByTestId('hanko-on-result')).toContainText('Hanko Club');

  await friendContext.close();
});
