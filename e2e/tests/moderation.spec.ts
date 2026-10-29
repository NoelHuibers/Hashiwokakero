// Reporting and blocking: a new visitor reports a seeded player on their profile, which the admin
// sees and dismisses, and blocks them: the boards show "blocked player" in their place, without a
// link, until they're unblocked on the account page.
import { expect, signIn, test, visit } from '../fixtures';
import { player } from '../seed';

test('report a player, block them and unblock them', async ({ page, browser, baseURL }) => {
  const chen = player('chen');
  const profile = `/player/${chen.id}`;
  // The tests share the database and may run more than once: this run's report
  const note = `Too fast to be true ${Date.now().toString(36)}`;

  // On someone else's profile, also for a visitor without a session
  await visit(page, profile);
  const actions = page.locator('[data-profile-actions]');
  await expect(actions.getByRole('button', { name: 'Report' })).toBeVisible();

  // Report: the dialog with the reason and a note; a visitor becomes a guest by it
  await actions.getByRole('button', { name: 'Report' }).click();
  const dialog = page.getByRole('dialog', { name: 'Report chen' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel('Offensive or inappropriate name')).toBeChecked();
  await dialog.getByLabel('Cheating').check();
  await dialog.getByLabel('Note (optional)').fill(note);
  await dialog.getByRole('button', { name: 'Send report' }).click();
  await expect(page.getByText('Thanks for your report. We look at every report within 24 hours.')).toBeVisible();
  await expect(dialog).toHaveCount(0);

  // The admin sees it, and dismisses it
  const admin = await browser.newContext({ baseURL });
  await signIn(admin, 'admin');
  const dashboard = await admin.newPage();
  await visit(dashboard, '/admin?tab=reports');
  const report = dashboard.locator('article').filter({ hasText: note });
  await expect(report).toContainText('chen');
  await expect(report).toContainText('cheating');
  dashboard.on('dialog', (d) => void d.accept());
  await report.getByRole('button', { name: 'Dismiss' }).click();
  await expect(report).toHaveCount(0);
  await admin.close();

  // Block, after a question; no rival button for a blocked player
  await actions.getByRole('button', { name: 'Block' }).click();
  const question = page.getByRole('dialog');
  await expect(question).toContainText("Block chen? Their name won't be shown to you anywhere on Hashi any more");
  // Cancel changes nothing
  await question.getByRole('button', { name: 'Cancel' }).click();
  await expect(question).toHaveCount(0);
  await actions.getByRole('button', { name: 'Block' }).click();
  await page.getByTestId('block-confirm').click();
  await expect(page.getByText('chen is blocked.')).toBeVisible();
  await expect(actions.getByRole('button', { name: 'Unblock' })).toBeVisible();
  await expect(page.locator('[data-blocked-note]')).toBeVisible();
  await expect(page.getByRole('button', { name: /rival/i })).toHaveCount(0);

  // On the boards: their place without their name or a link to them. chen played today
  // (seed.ts), on the podium
  await visit(page, '/ranks');
  const main = page.locator('main');
  await expect(main.locator('li.podium-step').filter({ hasText: 'blocked player' }).first()).toBeVisible();
  await expect(main.getByText('chen', { exact: true })).toHaveCount(0);
  await expect(main.locator(`a[href$="${profile}"]`)).toHaveCount(0);
  // Others still see them
  const other = await browser.newPage({ baseURL });
  await visit(other, '/ranks');
  await expect(other.locator('main li.podium-step').filter({ hasText: 'chen' }).first()).toBeVisible();
  await other.close();

  // Unblocked on the account page
  await visit(page, '/account');
  const blocked = page.locator('[data-blocked-players]');
  const row = blocked.locator(`[data-blocked-player="${chen.id}"]`);
  await expect(row).toContainText('chen');
  await row.getByRole('button', { name: 'Unblock' }).click();
  await expect(page.getByText('chen is unblocked.')).toBeVisible();
  await expect(blocked).toContainText("You haven't blocked anyone.");
  await visit(page, '/ranks');
  await expect(main.locator('li.podium-step').filter({ hasText: 'chen' }).first()).toBeVisible();
  await expect(main.getByText('blocked player')).toHaveCount(0);
});

test('signing up accepts the community rules', async ({ page }) => {
  await visit(page, '/account');
  const line = page.locator('[data-accept-rules]');
  await expect(line).toHaveText('By creating an account you accept the community rules.');
  await line.getByRole('link', { name: 'community rules' }).click();
  await expect(page).toHaveURL(/\/regeln$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Community rules' })).toBeVisible();
  await expect(page.getByText('There is zero tolerance for objectionable content and abusive users.')).toBeVisible();
});
