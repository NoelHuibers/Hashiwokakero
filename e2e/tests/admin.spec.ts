// The admin dashboard of a seeded admin: the overview, and every tab with what the seed put there.
import { expect, signIn, test, visit } from '../fixtures';
import { CONTACT_MESSAGES } from '../seed';

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

  await tab('Messages').click();
  await expect(page.getByText('Contact messages')).toBeVisible();
  for (const m of CONTACT_MESSAGES) await expect(page.getByText(m.message)).toBeVisible();
  await expect(page.getByText('mirai@example.com')).toBeVisible();
  await expect(page.getByText('(no email)')).toBeVisible();
  const mirai = page.locator('article').filter({ hasText: 'mirai@example.com' });
  await expect(mirai.getByRole('link', { name: 'Reply' })).toHaveAttribute('href', /^mailto:mirai@example\.com/);
});
