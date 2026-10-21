// Semantic regressions and native gestures: deliberately separate from the daily action tests.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { addBridge, board, cellCenter, edges, PRACTICE_KEY, saved, seedGame } from '../board';
import { ROOT } from '../env';
import { expect, signIn, test, visit } from '../fixtures';
import { LADDER, LADDER_SOLUTION, ladderRow } from '../puzzles';
import { localized } from '../site';

const playing = {
  puzzle: LADDER, bridges: edges(LADDER).map(() => 0), solution: LADDER_SOLUTION,
  status: 'Playing' as const, moves: 0, hints: 0,
};
const messages = (locale: string, namespace: string) =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, `${namespace}.json`), 'utf8'));

for (const width of [390, 768]) {
  for (const locale of ['en', 'ar']) {
    test(`header names survive hidden labels at ${width}px in ${locale}`, async ({ page, context }) => {
      await page.setViewportSize({ width, height: 844 });
      const common = messages(locale, 'common');
      const header = page.locator('header');
      await visit(page, localized(locale, '/rules'));
      await expect(header.getByRole('link', { name: `Hashi · ${common.nav.daily}`, exact: true }))
        .toHaveAttribute('href', localized(locale, '/'));
      await expect(header.getByRole('link', { name: common.sign_in, exact: true }))
        .toHaveAttribute('href', localized(locale, '/account'));
      for (const name of ['aiko', 'Drowsy Otter 21']) {
        await signIn(context, name);
        await visit(page, localized(locale, '/rules'));
        const label = `${common.account} · ${name === 'aiko' ? name : common.ranking.guest_name}`;
        await expect(header.getByRole('link', { name: label, exact: true })).toBeVisible();
      }
    });
  }
}

test('difficulty exposes one selected choice and can be changed with the keyboard in RTL', async ({ page }) => {
  await seedGame(page, PRACTICE_KEY, playing);
  await visit(page, '/ar/practice');
  const common = messages('ar', 'common');
  const group = page.getByRole('group', { name: messages('ar', 'practice').new_puzzle, exact: true });
  await expect(group.getByRole('button', { pressed: true })).toHaveCount(1);
  const medium = group.getByRole('button', { name: new RegExp(common.difficulty.medium) });
  await expect(medium).toHaveAttribute('aria-pressed', 'true');
  const easy = group.getByRole('button', { name: new RegExp(common.difficulty.easy) });
  await easy.focus();
  await easy.press('Space');
  await expect(easy).toHaveAttribute('aria-pressed', 'true');
  await expect(group.getByRole('button', { pressed: false })).toHaveCount(4);
  await expect(easy).toBeFocused();
  await easy.press('Tab');
  await expect(medium).toBeFocused();
  await medium.press('Enter');
  await expect(medium).toHaveAttribute('aria-pressed', 'true');
  await expect(easy).toHaveAttribute('aria-pressed', 'false');
});

test('toasts enter an existing polite live region without moving keyboard focus', async ({ page }) => {
  await seedGame(page, PRACTICE_KEY, playing);
  await visit(page, '/practice');
  const log = page.getByRole('log');
  await expect(log).toBeEmpty();
  await expect(log).toHaveAttribute('aria-live', 'polite');
  await expect(log).toHaveAttribute('aria-relevant', 'additions text');
  await expect(log).toHaveAttribute('aria-atomic', 'false');
  await log.evaluate((el) => el.setAttribute('data-test-persistent', 'yes'));
  const check = page.getByRole('button', { name: /^Check/ });
  await check.focus();
  await check.press('Enter');
  await expect(log).toContainText('So far so good: every bridge belongs to the solution.');
  await expect(check).toBeFocused();
  // An incorrect double bridge exercises the error message path too, without a network mock.
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(1));
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(1));
  await check.focus();
  await check.press('Enter');
  await expect(log).toContainText('Bridges that are not part of the solution: 1');
  await expect(log).toHaveAttribute('data-test-persistent', 'yes');
  await expect(check).toBeFocused();
  await expect(log.getByRole('alert')).toHaveCount(0);
});

test.describe('native and game gestures on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('swiping a read-only illustration scrolls the page and leaves native zoom available', async ({ page, context }) => {
    await visit(page, '/rules');
    const svg = board(page);
    await expect(svg).toHaveAttribute('role', 'img');
    await expect(svg).toHaveCSS('touch-action', 'auto');
    // A cancelable touchend must remain available to the browser too.
    expect(await svg.evaluate((el) => el.dispatchEvent(new TouchEvent('touchend', { bubbles: true, cancelable: true })))).toBe(true);
    await svg.evaluate((el) => el.scrollIntoView({ block: 'center' }));
    const rect = (await svg.boundingBox())!;
    const x = rect.x + rect.width / 2;
    const y = rect.y + rect.height * 0.75;
    const before = await page.evaluate(() => scrollY);
    const touch = await context.newCDPSession(page);
    await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
    for (let step = 1; step <= 8; step++) {
      await touch.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y - step * 20 }] });
      // Real time between input samples lets Chromium recognize the swipe as a gesture.
      await page.waitForTimeout(20);
    }
    await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(() => page.evaluate(() => scrollY)).toBeGreaterThan(before + 30);
    await touch.detach();
  });

  for (const locale of ['en', 'ar']) {
    test(`interactive drag cancellation stays safe in ${locale}`, async ({ page }) => {
      await seedGame(page, PRACTICE_KEY, playing);
      await visit(page, localized(locale, '/practice'));
      await expect(board(page)).toHaveCSS('touch-action', 'none');
      expect(await board(page).evaluate((el) => el.dispatchEvent(new TouchEvent('touchend', { bubbles: true, cancelable: true })))).toBe(false);
      const start = await cellCenter(page, 0, 1);
      const end = await cellCenter(page, 6, 1);
      await page.mouse.move(start.x, start.y);
      await page.mouse.down();
      await page.mouse.move(end.x, end.y, { steps: 5 });
      await expect(board(page).locator('g.bridge.ghost')).toBeVisible();
      await board(page).dispatchEvent('pointercancel', { pointerId: 1, pointerType: 'mouse' });
      await page.mouse.up();
      expect((await saved(page, PRACTICE_KEY)).bridges).toEqual(playing.bridges);
      await page.mouse.move(start.x, start.y);
      await page.mouse.down();
      await page.mouse.move(end.x, end.y, { steps: 5 });
      await page.mouse.up();
      await expect.poll(async () => (await saved(page, PRACTICE_KEY)).bridges[ladderRow(1)]).toBe(1);
    });
  }
});

test('reduced motion keeps selection and wrong-bridge feedback steady', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await seedGame(page, PRACTICE_KEY, playing);
  await visit(page, '/practice');
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(1));
  await addBridge(page, PRACTICE_KEY, LADDER, ladderRow(1));
  const point = await cellCenter(page, 0, 1);
  await page.mouse.click(point.x, point.y);
  const selection = board(page).locator('g.island.selected .sel');
  await expect(selection).toBeVisible();
  await expect(selection).toHaveCSS('animation-name', 'none');
  await page.getByRole('button', { name: /^Check/ }).click();
  const wrong = board(page).locator('g.bridge.wrong');
  await expect(wrong).toBeVisible();
  await expect(wrong).toHaveCSS('animation-name', 'none');
  await expect(wrong).not.toHaveCSS('filter', 'none');
  await page.getByRole('button', { name: /^Hint/ }).click();
  const hint = board(page).locator('g.bridge.hint');
  await expect(hint).toBeVisible();
  await expect(hint).toHaveCSS('animation-name', 'none');
  await expect(hint).not.toHaveCSS('filter', 'none');
});
