import { readFileSync } from 'node:fs';
import path from 'node:path';

import { edges, PLAY_KEY, seedGame } from '../board.ts';
import { ROOT } from '../env.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION } from '../puzzles.ts';
import { localized } from '../site.ts';

// Named after Play's old name, Practice (src/pages/play/solver_tools.rs)
const OPEN_KEY = 'hashi.practice.solver.open';
const playing = {
  puzzle: LADDER,
  bridges: edges(LADDER).map(() => 0),
  solution: LADDER_SOLUTION,
  status: 'Playing' as const,
  moves: 0,
  hints: 0,
};
const messages = (locale: string, namespace: string) =>
  JSON.parse(readFileSync(path.join(ROOT, 'locales', locale, `${namespace}.json`), 'utf8'));

for (const [width, locale] of [
  [320, 'en'],
  [375, 'de'],
  [375, 'ja'],
  [320, 'ar'],
  [1280, 'en'],
] as const) {
  test(`Play's instructions precede keyboard-accessible solver tools at ${width}px in ${locale}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 812 });
    await seedGame(page, PLAY_KEY, playing);
    await visit(page, localized(locale, '/play'));
    const tools = page.locator('aside details');
    const summary = tools.locator('summary');
    await expect(summary).toContainText(messages(locale, 'common').solver.title);
    await expect(tools).not.toHaveAttribute('open');
    await expect(tools.locator('#heuristic')).toBeHidden();
    const legend = page.getByRole('heading', {
      name: new RegExp(messages(locale, 'play').legend.title),
    });
    expect(
      await legend.evaluate(
        (element, selector) =>
          Boolean(
            element.compareDocumentPosition(document.querySelector(selector)!) &
              Node.DOCUMENT_POSITION_FOLLOWING,
          ),
        'aside details',
      ),
    ).toBe(true);
    expect((await summary.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await summary.focus();
    await summary.press('Enter');
    await expect(tools).toHaveAttribute('open', '');
    await expect(tools.locator('#heuristic')).toBeVisible();
    // The solver choice is one Tab stop, the chosen solver
    await summary.press('Tab');
    await expect(tools.getByRole('radio', { checked: true })).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(tools.locator('#heuristic')).toBeFocused();
    await summary.focus();
    await summary.press('Space');
    await expect(tools).not.toHaveAttribute('open');
    await summary.press('Tab');
    expect(await tools.evaluate((element) => element.contains(document.activeElement))).toBe(false);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
}

test('the solver disclosure of Play remembers opening and closing across reloads and navigation', async ({
  page,
}) => {
  await seedGame(page, PLAY_KEY, playing);
  await visit(page, '/play');
  const tools = page.locator('aside details');
  const summary = tools.locator('summary');
  await summary.click();
  await expect.poll(() => page.evaluate((key) => localStorage.getItem(key), OPEN_KEY)).toBe('true');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(tools).toHaveAttribute('open', '');
  await expect.poll(() => page.evaluate((key) => localStorage.getItem(key), OPEN_KEY)).toBe('true');
  await summary.click();
  await expect
    .poll(() => page.evaluate((key) => localStorage.getItem(key), OPEN_KEY))
    .toBe('false');
  await page.evaluate(() =>
    document.documentElement.setAttribute('data-play-navigation', 'same-document'),
  );
  await page.locator('header a[href="/create"]').click();
  await expect(page).toHaveURL(/\/create$/);
  await page.locator('header a[href="/play"]').click();
  await expect(page).toHaveURL(/\/play$/);
  await expect(page.locator('html')).toHaveAttribute('data-play-navigation', 'same-document');
  await expect(tools).not.toHaveAttribute('open');
  await expect(page.getByRole('button', { name: /^Check/ })).toBeVisible();
});

test('the solver disclosure of Play remains usable when its storage is unavailable', async ({
  page,
}) => {
  await seedGame(page, PLAY_KEY, playing);
  await page.addInitScript((blockedKey) => {
    const get = Storage.prototype.getItem;
    const set = Storage.prototype.setItem;
    Storage.prototype.getItem = function (key) {
      if (key === blockedKey) throw new DOMException('Unavailable', 'SecurityError');
      return get.call(this, key);
    };
    Storage.prototype.setItem = function (key, value) {
      if (key === blockedKey) throw new DOMException('Unavailable', 'SecurityError');
      set.call(this, key, value);
    };
  }, OPEN_KEY);
  await visit(page, '/play');
  const tools = page.locator('aside details');
  await expect(tools).not.toHaveAttribute('open');
  await tools.locator('summary').click();
  await expect(tools.locator('#heuristic')).toBeVisible();
  await tools.locator('summary').click();
  await expect(tools).not.toHaveAttribute('open');
});

test('the editor solver remains visible without the disclosure of Play', async ({ page }) => {
  await visit(page, '/create');
  await expect(page.locator('#heuristic')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Unique?' })).toBeVisible();
});
