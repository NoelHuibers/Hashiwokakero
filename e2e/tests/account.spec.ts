// The account page: a wrong password, changing the username, and deleting the account for good.
import { fillAuthForm, register, signInWith, signOut } from '../account';
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

test('change the username, then not again for a month', async ({ page, problems }) => {
  const username = newUsername('old');
  const renamed = newUsername('new');
  await visit(page, '/account');
  await register(page, username, PASSWORD);

  // Cancel keeps the name
  await page.getByRole('button', { name: 'Change username…' }).click();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.locator('#rename-username')).toHaveCount(0);

  // The form starts with the current name, which is no change
  await page.getByRole('button', { name: 'Change username…' }).click();
  await expect(page.locator('#rename-username')).toHaveValue(username);
  const save = page.getByRole('button', { name: 'Change username', exact: true });
  await expect(save).toBeDisabled();
  await expect(page.getByText('You can change it once every 30 days.', { exact: false })).toBeVisible();

  await page.locator('#rename-username').fill(renamed);
  await page.locator('#rename-password').fill('not-the-password');
  await save.click();
  await expect(page.getByText('Wrong password.')).toBeVisible();
  allow(problems, /\/api\/change_username/, /status of 4\d\d|status of 5\d\d/);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();

  await page.locator('#rename-password').fill(PASSWORD);
  await save.click();
  await expect(page.getByText(`You're now ${renamed}.`)).toBeVisible();
  await expect(page.getByRole('heading', { name: `こんにちは, ${renamed}` })).toBeVisible();
  // Not again for a month, after a reload either
  const wait = page.getByText(/^You can change your username again from \d+ \w+ \d{4}\.$/);
  await expect(wait).toBeVisible();
  await expect(page.getByRole('button', { name: 'Change username…' })).toHaveCount(0);
  await page.reload();
  await expect(page.getByRole('heading', { name: `こんにちは, ${renamed}` })).toBeVisible();
  await expect(wait).toBeVisible();

  // The new name signs in, the old one doesn't
  await signOut(page);
  await signInWith(page, username, PASSWORD);
  await expect(page.getByText('Wrong username or password.')).toBeVisible();
  allow(problems, /\/api\/login/, /status of 4\d\d|status of 5\d\d/);
  await signInWith(page, renamed, PASSWORD);
  await expect(page.getByRole('heading', { name: `こんにちは, ${renamed}` })).toBeVisible();
});

test('a name the boards do not want is refused, in the player\'s language', async ({ page, problems }) => {
  await visit(page, '/account');
  await fillAuthForm(page, 'Big_Ass_Bridges', PASSWORD);
  await page.getByRole('button', { name: /^Create account 登録/ }).click();
  await expect(page.getByText("This name isn't allowed here, please choose another one.")).toBeVisible();
  allow(problems, /\/api\/register/, /status of 4\d\d|status of 5\d\d/);
  await expect(page.getByRole('heading', { name: 'Your account' })).toBeVisible();

  // Another one goes through
  const username = newUsername('clean');
  await register(page, username, PASSWORD);
});
