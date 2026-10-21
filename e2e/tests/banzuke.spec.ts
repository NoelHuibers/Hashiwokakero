// The banzuke 番付 of the seeded group: the week's ranking as a sumo scroll with the awards
// beside it, today's results kept back until you've played, earlier weeks final, and last
// week's yokozuna crowned on the archipelago.
import type { Page } from '@playwright/test';

import { expect, hydrated, signIn, test, visit } from '../fixtures';
import { SEEDED_GROUP } from '../seed';

const banzuke = (page: Page) => page.getByTestId('banzuke');

test('the seeded group has a banzuke, awards and a crowned yokozuna', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  await hydrated(page);
  const card = banzuke(page);
  await expect(card.getByRole('heading', { name: /^Banzuke/ })).toBeVisible();
  // Every member has a line, this week is under way
  await expect(card.locator('li[data-member]')).toHaveCount(SEEDED_GROUP.members.length);
  await expect(card.getByTestId('banzuke-state')).toHaveAttribute('data-over', 'false');
  await expect(card.getByTestId('awards-state')).toHaveText('Leading so far');
  // bram and chen played today, aiko hasn't (seed.ts): today doesn't count for her yet
  await expect(card.getByTestId('banzuke-today-hidden')).toBeVisible();

  // Last week is final: a yokozuna on top, ranks below, the awards won
  await card.getByRole('button', { name: 'Earlier week' }).click();
  await expect(card.getByTestId('banzuke-state')).toHaveAttribute('data-over', 'true');
  await expect(card.getByTestId('banzuke-today-hidden')).toHaveCount(0);
  const lines = card.locator('li[data-member]');
  await expect(lines.first()).toHaveAttribute('data-title', 'yokozuna');
  await expect(lines.first().locator('.rank-plate')).toHaveText('横綱');
  await expect(lines.first().locator('[data-points]')).toContainText(/\d+/);
  await expect(card.getByTestId('awards-state')).toHaveText("This week's winners");
  await expect(card.locator('li[data-award]')).toHaveCount(6);
  const fastest = card.locator('li[data-award="fastest"]');
  await expect(fastest).toHaveAttribute('data-won', 'true');
  await expect(fastest).toContainText(/in total, days played: \d/);

  // Last week's yokozuna wears the crown, the tooltip says why
  const yokozuna = await lines.first().getAttribute('data-member');
  const crowned = page.locator(`svg g.nakama-island[data-member="${yokozuna}"]`);
  await expect(crowned.locator('[data-crown]')).toHaveCount(1);
  await expect(crowned).toHaveAttribute('data-tooltip', /last week's yokozuna/);

  // How it works, folded away
  await card.getByText('How the banzuke works').click();
  await expect(card.getByText(/scores points by their place in the group/)).toBeVisible();
});
