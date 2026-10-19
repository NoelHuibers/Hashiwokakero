// The account page: a wrong password, and deleting the account for good.
import { register, signInWith, signOut } from '../account';
import { allow, expect, newUsername, test, visit } from '../fixtures';

const PASSWORD = 'eight-planks-wide';

test('a wrong password is turned away, the right one signs in', async ({ page, problems }) => {
  const username = newUsername('login');
  await visit(page, '/account');
  await register(page, username, PASSWORD);
  await signOut(page);

  await signInWith(page, username, 'not-the-password');
  await expect(page.getByText('Wrong username or password.')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Your account' })).toBeVisible();
  // The server function answers the refusal with an error status
  allow(problems, /\/api\/login/, /status of 4\d\d|status of 5\d\d/);

  await signInWith(page, username, PASSWORD);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
  await expect(page.getByText('Wrong username or password.')).toHaveCount(0);
});

test('delete the account, behind its password', async ({ page, problems }) => {
  const username = newUsername('gone');
  await visit(page, '/account');
  await register(page, username, PASSWORD);

  await page.getByRole('button', { name: 'Delete account…' }).click();
  await expect(page.getByText("This can't be undone. Enter your password to confirm.")).toBeVisible();
  // Cancel keeps everything
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('button', { name: 'Delete for good' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Delete account…' }).click();
  await page.locator('#delete-password').fill('not-the-password');
  await page.getByRole('button', { name: 'Delete for good' }).click();
  await expect(page.getByText('Wrong password.')).toBeVisible();
  allow(problems, /\/api\/delete_account/, /status of 4\d\d|status of 5\d\d/);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();

  await page.locator('#delete-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Delete for good' }).click();
  await expect(page.getByText('Your account is deleted.')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Your account' })).toBeVisible();
  await expect(page.getByRole('link', { name: /Sign in/ })).toBeVisible();

  // Gone for good: the name doesn't sign in any more, after a reload either
  await page.reload();
  await signInWith(page, username, PASSWORD);
  await expect(page.getByText('Wrong username or password.')).toBeVisible();
  allow(problems, /\/api\/login/, /status of 4\d\d|status of 5\d\d/);
});
