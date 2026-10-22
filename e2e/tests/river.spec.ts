// The river 川 of a group: the seeded pair's bridge reached across its stream since anyone last
// looked. The first visit lays the new planks, completes the level and celebrates it once; the
// crossed stream is a hanko on the group page and the crew's stamp in the stamp book.
import type { Page } from '@playwright/test';

import { expect, hydrated, signIn, test, visit } from '../fixtures';
import { RIVER_GROUP, SEEDED_GROUP } from '../seed';

const river = (page: Page) => page.getByTestId('river-card');

test('the seeded group crosses its stream, celebrated once', async ({ page, context }) => {
  await signIn(context, 'Quiet Crane 5');
  await visit(page, `/groups/${RIVER_GROUP.id}`);
  await expect(page.getByRole('heading', { level: 1, name: RIVER_GROUP.name })).toBeVisible();

  // The level was found complete on the way in: petals and the stream's hanko
  const crossed = page.getByRole('dialog', { name: 'The stream is bridged!' });
  await expect(crossed).toBeVisible();
  await expect(crossed).toContainText('Together you laid 400 bridges. Next: The river.');
  await expect(crossed).toContainText('Bridge builder');
  // The next water, half built: every plank new to the crane, laid before their eyes
  const scene = river(page).locator('svg[data-waters]');
  await expect(scene).toHaveAttribute('data-waters', 'river');
  await expect(river(page).getByTestId('river-progress')).toHaveText('425 / 850 bridges');
  const planks = Number(await scene.getAttribute('data-planks'));
  await expect(scene.locator('g.plank')).toHaveCount(Math.floor(planks / 2));
  await expect(scene.locator('g.plank-new')).toHaveCount(Math.floor(planks / 2));
  await expect(river(page).getByText('+825 since you last looked')).toBeVisible();
  // The crossed stream as a hanko, the river under construction
  const row = river(page).getByTestId('river-crossed-row');
  await expect(row.locator('[data-level]')).toHaveCount(1);
  await expect(row.getByRole('img', { name: /^The stream: crossed on .*, 400 bridges$/ })).toBeVisible();
  await expect(row.getByRole('img', { name: 'The river: under construction' })).toBeVisible();

  await crossed.getByRole('button', { name: /^Onward!/ }).click();
  await expect(crossed).toHaveCount(0);

  // Back again: told once, nothing new
  await page.reload();
  await hydrated(page);
  await expect(river(page).getByTestId('river-progress')).toHaveText('425 / 850 bridges');
  await expect(page.getByTestId('river-crossed')).toHaveCount(0);
  await expect(river(page).locator('g.plank')).toHaveCount(Math.floor(planks / 2));
  await expect(river(page).locator('g.plank-new')).toHaveCount(0);
  await expect(river(page).getByText(/since you last looked/)).toHaveCount(0);

  // The crane laid planks of it: their stamp book has the river's stamp
  await visit(page, '/account');
  const stamp = page.locator('li').filter({ hasText: 'Bridge builder' });
  await expect(stamp).toBeVisible();
  await expect(stamp).not.toHaveClass(/opacity-60/);
});

test('a river nobody has laid planks of this week asks for the first one', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  // The seeded dailies have no runs: an empty stream for the Bridge Club, six members
  await expect(river(page).getByTestId('river-progress')).toHaveText('0 / 900 bridges');
  await expect(river(page).locator('svg[data-waters="stream"] g.plank')).toHaveCount(0);
  await expect(river(page)).toContainText('Nobody has laid a plank this week yet.');
  await expect(page.getByTestId('river-crossed')).toHaveCount(0);
});
