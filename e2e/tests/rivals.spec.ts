// Rivals: a new player marks two of the seeded ones, from today's podium and from a profile,
// and after the daily sees how the duels went. Nothing about the rivals before the finish.
import { expectResult, solveDaily, startDaily } from '../daily';
import { expect, test, visit } from '../fixtures';
import { player } from '../seed';

test('mark rivals on the leaderboard and a profile, then duel them in the daily', async ({ page }) => {
  // A visitor without a session: marking one makes them a guest
  await visit(page, '/ranks');
  const rivalsCard = page.locator('[data-rivals-board]');
  await expect(rivalsCard).toContainText('No rivals yet');
  // chen played today (seed.ts), on today's podium
  const podium = page.locator('li.podium-step').filter({ hasText: 'chen' }).first();
  await podium.hover();
  const swords = podium.getByRole('button', { name: 'Mark chen as your rival' });
  await swords.click();
  await expect(page.getByText('chen is your rival now. Every daily is a duel!')).toBeVisible();
  await expect(podium.getByRole('button', { name: 'chen is your rival, unmark' })).toHaveAttribute('aria-pressed', 'true');
  // The same player is marked on every board of the page, and compared in the rivals card
  await expect(page.getByRole('button', { name: 'chen is your rival, unmark' }).nth(1)).toBeVisible();
  await expect(rivalsCard.locator('tr').filter({ hasText: 'chen' })).toBeVisible();
  await expect(rivalsCard.locator('tr.rank-mark')).toContainText('you');

  // aiko from her profile: the head-to-head shows up, empty yet
  await visit(page, `/player/${player('aiko').id}`);
  await page.getByRole('button', { name: /^Mark as rival/ }).click();
  await expect(page.getByRole('button', { name: /^Your rival/ })).toHaveAttribute('aria-pressed', 'true');
  const record = page.locator('[data-head-to-head]');
  await expect(record).toContainText('Head-to-head');
  await expect(record).toContainText('No day you both solved yet');

  // The daily: no rivals card while it runs, the server sends nothing before the finish
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  await expect(page.locator('[data-rivals]')).toHaveCount(0);
  await solveDaily(page, puzzle, solution);
  await expectResult(page);

  const duels = page.locator('[data-rivals]');
  await expect(duels).toContainText('Rivals today');
  const chen = duels.locator('li').filter({ hasText: 'chen' });
  await expect(chen).toHaveText(/You beat chen by [\d.:]+ (s|min)|chen was [\d.:]+ (s|min) faster/);
  // chen's time today is in the line (1:38.0, seed.ts), and the first duel in the record
  await expect(chen).toContainText('1:38.0');
  await expect(chen).toContainText(/^.*(1–0|0–1)$/s);
  // aiko hasn't played today
  await expect(duels.locator('li').filter({ hasText: 'aiko' })).toContainText("aiko hasn't played yet");

  // Unmarked from the ranks page, gone from the comparison
  await visit(page, '/ranks');
  await page.getByRole('button', { name: 'chen is your rival, unmark' }).first().click();
  await expect(page.getByText('chen is no longer your rival.')).toBeVisible();
  await expect(rivalsCard.locator('tr').filter({ hasText: 'chen' })).toHaveCount(0);
  await expect(rivalsCard.locator('tr').filter({ hasText: 'aiko' })).toBeVisible();
});
