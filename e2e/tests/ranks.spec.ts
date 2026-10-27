// The rankings: the top three on a podium, your own place pressed like a hanko (an outline the
// lines between the rows step aside for).
import type { Locator, Page } from '@playwright/test';

import { allow, expect, hydrated, signIn, test, visit } from '../fixtures';
import { rankingExample } from '../ranking';

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

test('a tab of an older version is asked to reload, not shown the 404 page', async ({ page, problems }) => {
  // An older WASM calls server functions the new server doesn't have, the server answers with
  // its 404 page: like here, where the leaderboard's call goes to a path the server never had
  await page.route('**/api/daily_leaderboard', (route) =>
    route.continue({ url: route.request().url().replace(/daily_leaderboard$/, 'daily_leaderboard_old') }),
  );
  await visit(page, '/ranks');
  const toast = page.getByText('Hashi has been updated. Reload the page to continue.');
  await expect(toast).toHaveCount(1);
  await expect(page.getByText('<!DOCTYPE')).toHaveCount(0);
  await expect(page.getByText('Something went wrong')).toHaveCount(0);
  allow(problems, /^404 POST .*\/api\/daily_leaderboard/, /console error: Failed to load resource.*404/);

  // Its button reloads the page, which then loads the leaderboard
  await page.unroute('**/api/daily_leaderboard');
  const leaderboard = page.waitForResponse((r) => r.url().endsWith('/api/daily_leaderboard') && r.ok());
  await page.getByRole('button', { name: 'Reload' }).click();
  await leaderboard;
  await hydrated(page);
  await expect(toast).toHaveCount(0);
});

test('a tab of an older build is asked to reload when the server answers in another shape', async ({ page }) => {
  // The page names the build of its WASM and every answer the server's, the same here: the server
  // and the WASM come from one build (in CI from two jobs, put together by hash.txt)
  const answered = page.waitForResponse((r) => r.url().endsWith('/api/daily_leaderboard'));
  await visit(page, '/ranks');
  const build = await page.locator('meta[name="hashi-build"]').getAttribute('content');
  expect(build).toMatch(/^[\w-]+$/);
  expect((await answered).headers()['x-hashi-build']).toBe(build);

  // The leaderboard comes back in a shape this WASM can't read, as when a server function keeps
  // its name and returns another type
  let serverBuild = build!;
  await page.route('**/api/daily_leaderboard', (route) =>
    route.fulfill({
      contentType: 'application/json',
      headers: { 'x-hashi-build': serverBuild },
      body: '{"board":"of another shape"}',
    }),
  );
  const reload = page.getByText('Hashi has been updated. Reload the page to continue.');
  const earlier = page.getByRole('button', { name: /Earlier/ }).first();
  // From the same build it's a bug, shown as an error
  await earlier.click();
  await expect(page.getByText(/^Something went wrong/)).toBeVisible();
  await expect(reload).toHaveCount(0);
  // From another build the tab is outdated
  serverBuild = 'a-newer-build';
  await earlier.click();
  await expect(reload).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Reload' })).toBeVisible();
});

test('shared hundredths show every tied medalist in equal compact rows', async ({ page }) => {
  await page.route('**/api/daily_leaderboard', (route) => route.fulfill({
    json: {
      day: 20752, difficulty: 'Easy', me: null, finishers: 5,
      entries: [
        { rank: 1, user_id: 501, name: 'Noel', time_ms: 7826 },
        { rank: 1, user_id: 502, name: 'Nele', time_ms: 7831 },
        { rank: 3, user_id: 503, name: 'Third A', time_ms: 8803 },
        { rank: 3, user_id: 504, name: 'Third B', time_ms: 8804 },
        { rank: 5, user_id: 505, name: 'Fifth', time_ms: 9000 },
      ].map((entry) => ({ ...entry, guest: true, hints: 0, checks: 0, stamp: null })),
    },
  }));
  await visit(page, '/ranks');
  const podium = page.locator('ol:has(> li.podium-step)').first();
  await expect(podium.locator('> li')).toHaveCount(4);
  const winners = podium.locator('> li[data-place="1"]');
  await expect(winners).toHaveCount(2);
  await expect(winners.nth(0)).toContainText('7.83s');
  await expect(winners.nth(1)).toContainText('7.83s');
  await expect(winners.locator('.podium-shared-label')).toHaveCount(2);
  await expect(podium.locator('> li[data-place="3"]')).toHaveCount(2);
  const first = (await winners.nth(0).boundingBox())!;
  const second = (await winners.nth(1).boundingBox())!;
  expect(first.y).toBeLessThan(second.y);
  expect(first.height).toBe(second.height);
  expect(first.width).toBe(second.width);
  await expect(page.locator('table.rank-table').first()).toContainText('Fifth');
  const fourth = (await podium.locator('> li').nth(3).boundingBox())!;
  expect(fourth.y).toBeGreaterThan(first.y);
  await page.setViewportSize({ width: 360, height: 800 });
  const mobileFirst = (await winners.nth(0).boundingBox())!;
  const mobileSecond = (await winners.nth(1).boundingBox())!;
  expect(mobileFirst.y).toBeLessThan(mobileSecond.y);
  expect(mobileFirst.height).toBe(mobileSecond.height);
  expect(mobileFirst.width).toBe(mobileSecond.width);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(360);
});

for (const kind of ['shared', 'long', 'many'] as const) {
  test(`${kind} results fit phones without overlapping names, medals, or times`, async ({ page }) => {
    await rankingExample(page, kind);
    await visit(page, '/ranks');
    const podium = page.locator('ol.podium').first();
    await expect(podium.locator('> li')).toHaveCount(kind === 'many' ? 8 : 3);
    for (const width of [320, 360, 390, 768, 1024]) {
      await page.setViewportSize({ width, height: 900 });
      const layout = await podium.locator('> li').evaluateAll((items) => items.map((li) => {
        const rect = (selector: string) => li.querySelector(selector)!.getBoundingClientRect();
        const medal = rect('.podium-medal');
        const name = rect('.podium-player');
        const value = rect('.podium-value');
        const box = li.getBoundingClientRect();
        const compact = getComputedStyle(li).display === 'grid';
        return {
          nameWidth: name.width,
          noOverlap: compact ? medal.right <= name.left && name.right <= value.left : name.bottom <= value.top,
          contained: value.right <= box.right && value.left >= box.left,
          height: box.height,
          compact,
        };
      }));
      for (const row of layout) {
        expect(row.nameWidth, `${width}px name width`).toBeGreaterThan(40);
        expect(row.noOverlap, `${width}px overlaps`).toBe(true);
        expect(row.contained, `${width}px score outside card`).toBe(true);
        if (row.compact) expect(row.height, `${width}px row height`).toBeLessThan(90);
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    }
    if (kind === 'long') {
      await expect(podium.locator('.rank-time-long')).toHaveCount(3);
      await expect(podium.locator('.rank-time').first()).toHaveText('28:44.32');
      await expect(page.locator('table.rank-table').first()).toContainText('2:03:47.13');
      await expect(podium.locator('.podium-shared-label')).toHaveCount(0);
    } else {
      await expect(podium.locator('.podium-shared-label')).toHaveCount(kind === 'many' ? 8 : 2);
    }
  });
}

test('the monthly medal rows include everyone sharing a medal place', async ({ page }) => {
  await page.route('**/api/month_leaderboard', (route) => route.fulfill({
    json: {
      month: 2026 * 12 + 9, me: null, players: 5,
      entries: [1, 2, 2, 2, 5].map((rank, i) => ({
        rank, user_id: 601 + i, name: `Monthly ${i + 1}`, guest: false,
        points: rank === 1 ? 200 : rank === 2 ? 180 : 100, solved: 10, stamp: null,
      })),
    },
  }));
  await visit(page, '/ranks');
  const monthly = page.locator('[data-name="Card"]').filter({ has: page.getByRole('heading', { name: /^Monthly race/ }) });
  await expect(monthly.locator('li.podium-step')).toHaveCount(4);
  await expect(monthly.locator('li.podium-step[data-place="2"]')).toHaveCount(3);
  await expect(monthly.locator('.podium-shared-label')).toHaveCount(3);
  await expect(monthly.locator('table')).toContainText('Monthly 5');
  await expect(monthly.locator('.rank-time')).toHaveCount(0);
});
