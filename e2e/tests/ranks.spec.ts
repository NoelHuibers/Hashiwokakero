// The rankings: the top three on a podium, your own place pressed like a hanko (an outline the
// lines between the rows step aside for).
import type { Locator, Page } from '@playwright/test';

import { expect, signIn, test, visit } from '../fixtures';

/** The podium of the daily board some days ago (the seed has no results of today). */
async function daysAgo(page: Page, days: number) {
  await visit(page, '/ranks');
  const earlier = page.getByRole('button', { name: /Earlier/ }).first();
  const title = page.getByText(/Daily bridge #/).first();
  for (let i = 0; i < days; i++) {
    const before = await title.innerText();
    await earlier.click();
    await expect(title).not.toHaveText(before);
  }
  const podium = page.locator('ol:has(> li.podium-step)').first();
  await expect(podium).toBeVisible();
  return podium;
}

/** The podium steps by place: their position and height. */
async function steps(podium: Locator) {
  const boxes = await podium.locator('> li').evaluateAll((items) =>
    items.map((li) => {
      const r = li.getBoundingClientRect();
      return { place: Number((li as HTMLElement).dataset.place), x: r.x, height: r.height };
    }),
  );
  return Object.fromEntries(boxes.map((b) => [b.place, b]));
}

test('the top three stand on a podium, 2 · 1 · 3, the winner highest', async ({ page }) => {
  const podium = await daysAgo(page, 1);
  const by = await steps(podium);
  expect(Object.keys(by)).toEqual(['1', '2', '3']);
  expect(by[2].x).toBeLessThan(by[1].x);
  expect(by[1].x).toBeLessThan(by[3].x);
  expect(by[1].height).toBeGreaterThan(by[2].height);
  expect(by[2].height).toBeGreaterThan(by[3].height);
  // The rest of the board stays a table, signed out nothing is marked
  await expect(page.locator('table.rank-table').first()).toBeVisible();
  await expect(page.locator('.rank-mark')).toHaveCount(0);
});

test('the podium mirrors in right to left languages', async ({ page }) => {
  await visit(page, '/ar/ranks');
  await page.locator('button:has-text("←")').first().click();
  const podium = page.locator('ol:has(> li.podium-step)').first();
  await expect(podium).toBeVisible();
  const by = await steps(podium);
  expect(by[3].x).toBeLessThan(by[1].x);
  expect(by[1].x).toBeLessThan(by[2].x);
});

test('your own row is outlined and the lines next to it step aside', async ({ page, context }) => {
  // Fifth and last two days ago, after aiko (the seed is the same every run)
  await signIn(context, 'Drowsy Otter 21');
  await daysAgo(page, 2);
  const marked = page.locator('tr.rank-mark').first();
  await expect(marked).toContainText('Drowsy Otter 21');
  const look = await marked.evaluate((tr) => {
    const cells = [...tr.children] as HTMLElement[];
    const style = (el: Element) => getComputedStyle(el);
    const before = tr.previousElementSibling;
    const other = [...tr.parentElement!.children].find((row) => row !== tr && row.children.length > 1)!;
    return {
      outline: style(cells[1]).borderTopStyle,
      rounded: style(cells[0]).borderTopLeftRadius !== '0px' && style(cells.at(-1)!).borderTopRightRadius !== '0px',
      lineBefore: before ? style(before.children[0]).borderBottomColor : 'rgba(0, 0, 0, 0)',
      // The marked row's contents don't move
      shift: cells[1].getBoundingClientRect().x - other.children[1].getBoundingClientRect().x,
    };
  });
  expect(look).toEqual({ outline: 'solid', rounded: true, lineBefore: 'rgba(0, 0, 0, 0)', shift: 0 });
});

test('on the podium your step is marked', async ({ page, context }) => {
  // Second yesterday
  await signIn(context, 'aiko');
  const podium = await daysAgo(page, 1);
  const mine = podium.locator('li.rank-mark');
  await expect(mine).toHaveAttribute('data-place', '2');
  await expect(mine).toContainText('aiko');
  await expect(mine).toContainText('you');
});
