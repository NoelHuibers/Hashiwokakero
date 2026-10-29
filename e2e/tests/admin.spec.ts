// The admin dashboard of a seeded admin: the overview, and every tab with what the seed put there.
import { expect, signIn, test, visit } from '../fixtures';
import { COMMUNITY_FAMILY, CONTACT_MESSAGES } from '../seed';

test('the dashboard shows the day, the Dojo and the messages', async ({ page, context }) => {
  await signIn(context, 'admin');
  await visit(page, '/admin');
  await expect(page.getByRole('heading', { name: 'Admin', exact: true })).toBeVisible();
  for (const tile of ['Players', "Today's daily", 'Flagged today', 'Dojo']) {
    await expect(page.locator('p', { hasText: new RegExp(`^${tile}$`, 'i') })).toBeVisible();
  }
  // Numbers, not the placeholder
  await expect(page.getByText(/^\d+ registered$/)).toBeVisible();

  const tab = (name: string) => page.getByRole('tab', { name: new RegExp(`^${name}`) });
  await expect(tab('Daily review')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByText('Daily review', { exact: true }).nth(1)).toBeVisible();

  await tab('SAT Dojo').click();
  await expect(page.getByText('SAT Dojo submissions')).toBeVisible();
  await expect(page.getByText('Plain DPLL').first()).toBeVisible();
  // Community benchmarks to moderate, the seeded one in the pool
  await expect(page.getByText('Community benchmarks', { exact: true })).toBeVisible();
  const benchmark = page.locator('li').filter({ hasText: COMMUNITY_FAMILY });
  await expect(benchmark).toContainText('accepted');
  await expect(benchmark.getByRole('button', { name: 'Reject' })).toBeVisible();

  await tab('Messages').click();
  await expect(page.getByText('Feedback', { exact: true })).toBeVisible();
  for (const m of CONTACT_MESSAGES) await expect(page.getByText(m.message)).toBeVisible();
  await expect(page.getByText('mirai@example.com')).toBeVisible();
  await expect(page.getByText('(no email)')).toBeVisible();
  const mirai = page.locator('article').filter({ hasText: 'mirai@example.com' });
  await expect(mirai.getByRole('link', { name: 'Reply' })).toHaveAttribute('href', /^mailto:mirai@example\.com/);
  // Without GITHUB_FEEDBACK_TOKEN (as here) no issues: the button is off and says why
  await expect(page.getByText('Creating issues is off', { exact: false })).toBeVisible();
  await expect(mirai.getByRole('button', { name: 'Create issue' })).toBeDisabled();
});

test('admins ban a player and lift the ban', async ({ page, context, browser, baseURL }) => {
  // A guest of its own, the tests share the database
  const guest = await browser.newContext({ baseURL });
  expect((await guest.request.post('/api/start_daily', { data: { device: null } })).ok()).toBe(true);
  const me = async () => (await guest.request.post('/api/current_user', { data: {} })).json();
  const { id } = await me();

  await signIn(context, 'admin');
  await visit(page, '/admin?tab=bans');
  await expect(page.getByRole('tab', { name: 'Bans' })).toHaveAttribute('aria-selected', 'true');
  page.on('dialog', (d) => void d.accept());
  const ban = async (player: string, reason = '') => {
    await page.getByLabel('Player #').fill(player);
    await page.getByLabel('Reason').fill(reason);
    await page.getByRole('button', { name: 'Ban', exact: true }).click();
  };

  await ban('999999');
  await expect(page.getByText('There is no player #999999.')).toBeVisible();
  // The admin themselves (seed.ts)
  await ban('8');
  await expect(page.getByText("Player #8 is an admin, admins can't be banned.")).toBeVisible();

  await ban(`#${id}`, 'Bot-like clicks on three dailies');
  await expect(page.getByText(`(#${id}) is banned.`)).toBeVisible();
  const row = page.locator('article').filter({ hasText: `guest #${id}` });
  await expect(row).toContainText('Bot-like clicks on three dailies');
  // Signed out
  expect(await me()).toBeNull();

  await row.getByRole('button', { name: 'Lift ban' }).click();
  await expect(row).toHaveCount(0);
  await guest.close();
});
