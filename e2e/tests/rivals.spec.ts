// Rivals: a new player marks two of the seeded ones on their profiles, and after the daily
// sees how the duels went. The boards only mark them, nothing about the rivals before the finish.
import { expectResult, solveDaily, startDaily } from '../daily';
import { expect, test, visit } from '../fixtures';
import { player } from '../seed';

test('mark rivals on their profiles, then duel them in the daily', async ({ page }) => {
  // A visitor without a session: no rivals card, and the boards have nothing to mark anyone
  await visit(page, '/ranks');
  const rivalsCard = page.locator('[data-rivals-board]');
  // chen played today (seed.ts), on today's podium
  const podium = page.locator('li.podium-step').filter({ hasText: 'chen' }).first();
  await expect(podium).toBeVisible();
  await expect(rivalsCard).toHaveCount(0);
  await expect(page.getByRole('button', { name: /rival/i })).toHaveCount(0);
  await expect(podium.locator('.rival-mark')).toHaveCount(0);

  // chen from their profile: marking one makes the visitor a guest
  await visit(page, `/player/${player('chen').id}`);
  await page.getByRole('button', { name: /^Mark as rival/ }).click();
  await expect(page.getByText('chen is your rival now. Every daily is a duel!')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Your rival/ })).toHaveAttribute('aria-pressed', 'true');

  // On the ranks a small mark, no button, and the rivals card with you and chen
  await visit(page, '/ranks');
  await expect(podium.getByRole('img', { name: 'Your rival' })).toBeVisible();
  await expect(page.getByRole('button', { name: /rival/i })).toHaveCount(0);
  await expect(rivalsCard.locator('tr').filter({ hasText: 'chen' })).toBeVisible();
  await expect(rivalsCard.locator('tr.rank-mark')).toContainText('you');
  // Only on the daily and monthly boards, not among your rivals themselves
  await expect(rivalsCard.locator('.rival-mark')).toHaveCount(0);

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

  // Unmarked on chen's profile, gone from the comparison and the boards
  await visit(page, `/player/${player('chen').id}`);
  await page.getByRole('button', { name: /^Your rival/ }).click();
  await expect(page.getByText('chen is no longer your rival.')).toBeVisible();
  await visit(page, '/ranks');
  await expect(rivalsCard.locator('tr').filter({ hasText: 'aiko' })).toBeVisible();
  await expect(rivalsCard.locator('tr').filter({ hasText: 'chen' })).toHaveCount(0);
  await expect(podium.locator('.rival-mark')).toHaveCount(0);
});

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  // The number columns took the row, so the names were cut down to their first letter
  test('the rivals card leaves the names their room', async ({ page }) => {
    await visit(page, `/player/${player('chen').id}`);
    await page.getByRole('button', { name: /^Mark as rival/ }).click();
    await expect(page.getByRole('button', { name: /^Your rival/ })).toHaveAttribute('aria-pressed', 'true');

    await visit(page, '/ranks');
    const rivalsCard = page.locator('[data-rivals-board]');
    const name = (row: string) => rivalsCard.locator(row).locator('a[href*="/player/"]');
    const chen = name('tr:has-text("chen")');
    await expect(chen).toBeVisible();
    // chen in full, not truncated
    expect(await chen.evaluate((a) => a.scrollWidth <= a.clientWidth)).toBe(true);
    // Your own guest name ("Sleepy Tanuki 27") may truncate, but keeps a word's room: the
    // marked row says it's you, the "you" badge is only for screen readers
    const mine = rivalsCard.locator('tr.rank-mark');
    expect((await name('tr.rank-mark').boundingBox())!.width).toBeGreaterThan(70);
    await expect(mine).toContainText('you');
    expect((await mine.getByText('you', { exact: true }).boundingBox())!.width).toBeLessThanOrEqual(1);
    // And nothing runs out of the card
    expect(await rivalsCard.evaluate((c) => c.scrollWidth <= c.clientWidth)).toBe(true);
  });
});
