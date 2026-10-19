// The account page's forms, as a player fills them in.
import { expect, type Page } from '@playwright/test';

/** The form of the open tab: the other one stays in the page, hidden. */
const field = (page: Page, label: string) => page.getByLabel(label, { exact: true }).filter({ visible: true });

export async function fillAuthForm(page: Page, username: string, password: string) {
  await field(page, 'Username').fill(username);
  await field(page, 'Password').fill(password);
}

/** Creates an account on /account (already open), signed in afterwards. */
export async function register(page: Page, username: string, password: string) {
  await fillAuthForm(page, username, password);
  await page.getByRole('button', { name: /^Create account 登録/ }).click();
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
}

export async function signOut(page: Page) {
  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page.getByText('Signed out.')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Your account' })).toBeVisible();
}

/** Fills in the sign in tab and sends it; the caller checks what came of it. */
export async function signInWith(page: Page, username: string, password: string) {
  // The tabs are buttons
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await fillAuthForm(page, username, password);
  await page.getByRole('button', { name: /^Sign in 入る/ }).click();
}
