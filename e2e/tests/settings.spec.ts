// The settings in the header: day or night, sound and music, the language. All kept by the
// browser, so a reload keeps them.
import type { Page } from '@playwright/test';

import { expect, test, visit } from '../fixtures';

const reload = async (page: Page) => {
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
};

test('night mode stays on after a reload', async ({ page }) => {
  await visit(page, '/rules');
  const html = page.locator('html');
  const toggle = page.getByRole('button', { name: 'Toggle dark mode' });
  await expect(toggle).toHaveText('昼');
  await toggle.click();
  await expect(html).toHaveClass(/\bdark\b/);
  await expect(toggle).toHaveText('夜');
  await reload(page);
  await expect(html).toHaveClass(/\bdark\b/);
  await expect(toggle).toHaveText('夜');
  await toggle.click();
  await expect(html).not.toHaveClass(/\bdark\b/);
});

test('sound off and music on stay so after a reload', async ({ page }) => {
  await visit(page, '/rules');
  const open = () => page.getByRole('button', { name: 'Settings' }).click();
  const sound = page.getByRole('switch', { name: 'Sound effects' });
  const music = page.getByRole('switch', { name: 'Music' });
  await open();
  await expect(sound).toHaveAttribute('aria-checked', 'true');
  await expect(music).toHaveAttribute('aria-checked', 'false');
  await sound.click();
  await music.click();
  await expect(sound).toHaveAttribute('aria-checked', 'false');
  await expect(music).toHaveAttribute('aria-checked', 'true');
  // Escape closes the pop-up
  await page.keyboard.press('Escape');
  await expect(sound).toBeHidden();

  await reload(page);
  await open();
  await expect(sound).toHaveAttribute('aria-checked', 'false');
  await expect(music).toHaveAttribute('aria-checked', 'true');
});

test('the language picker opens the page in that language', async ({ page }) => {
  await visit(page, '/rules?from=test');
  await page.getByRole('button', { name: 'Language' }).click();
  await page.getByRole('link', { name: /^Deutsch/ }).click();
  await expect(page).toHaveURL(/\/de\/rules\?from=test$/);
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  await expect(page.getByRole('button', { name: 'Sprache' })).toBeVisible();
  await reload(page);
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');

  // And on to another script
  await page.getByRole('button', { name: 'Sprache' }).click();
  await page.getByRole('link', { name: /^日本語/ }).click();
  await expect(page).toHaveURL(/\/ja\/rules\?from=test$/);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
});
