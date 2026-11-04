// The recovery email and "Forgot password?": an address added behind the password, confirmed by
// the link mailed to it, then a forgotten password reset by the link mailed there. The server
// writes its mails into a directory (mail.ts) instead of sending them.
import { register, signInWith, signOut } from '../account.ts';
import { expect, newUsername, test, visit } from '../fixtures.ts';
import { linkIn, nthMailTo } from '../mail.ts';

const PASSWORD = 'eight-planks-wide';
const NEW_PASSWORD = 'nine-planks-wide';

test('add a recovery email, forget the password, reset it', async ({ page }) => {
  const username = newUsername('reset');
  const address = `${username}@example.org`;
  await visit(page, '/account');
  await register(page, username, PASSWORD);

  // Added behind the password, waiting for its confirmation
  const card = page.locator('[data-recovery-email]');
  await expect(card.getByText('No recovery email yet.')).toBeVisible();
  await card.getByRole('button', { name: 'Add recovery email…' }).click();
  await page.locator('#recovery-email').fill(address);
  await page.locator('#recovery-password').fill(PASSWORD);
  await card.getByRole('button', { name: 'Send confirmation link' }).click();
  await expect(card.getByText(`Waiting for confirmation: ${address}.`)).toBeVisible();
  await expect(card.getByText('No recovery email yet.')).toBeVisible();

  // The link opens a page that confirms it with a click
  const confirmation = await nthMailTo(address, 1);
  expect(confirmation.subject).toBe('Hashi: Confirm your recovery email');
  await visit(page, linkIn(confirmation));
  await page.getByRole('button', { name: 'Confirm address' }).click();
  await expect(
    page.getByText(`${address} is now the recovery email of your account.`),
  ).toBeVisible();

  await visit(page, '/account');
  await expect(card.getByText(`Your recovery email: ${address}`)).toBeVisible();
  await signOut(page);

  // Forgotten: the sign in form leads to the reset
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.getByRole('link', { name: 'Forgot password?' }).click();
  await expect(page.getByRole('heading', { name: 'Forgot your password?' })).toBeVisible();
  await page.locator('#forgot-login').fill(username);
  await page.getByRole('button', { name: 'Mail me a link' }).click();
  await expect(page.getByRole('status')).toContainText('the link is on its way');

  const reset = await nthMailTo(address, 2);
  expect(reset.subject).toBe('Hashi: Reset your password');
  await visit(page, linkIn(reset));
  await expect(page.getByRole('heading', { name: 'Choose a new password' })).toBeVisible();
  await page.locator('#reset-password').fill(NEW_PASSWORD);
  await page.getByRole('button', { name: 'Save and sign in' }).click();
  // Signed in, on the account page
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();

  // The new password signs in, after signing out
  await signOut(page);
  await signInWith(page, username, NEW_PASSWORD);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
});
