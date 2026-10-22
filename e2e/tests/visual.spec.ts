// Screenshots of a few pages, compared with the ones in __screenshots__: the only tests that see
// a layout broken by a Tailwind update or a font that went missing. Run with the Playwright
// container's browser only (`pnpm run e2e:container`, `pnpm run e2e:update-snapshots` after a
// change on purpose), the fonts and the rendering differ from one system to the next.
//
// Whatever changes from day to day is hidden (not masked: a mask is as wide as the text under
// it): dates, the daily's number and difficulty, the monthly race. The project has a server and database of its own (see env.ts), and its tests run
// one after another, the daily last: until then today's ranking is the seeded one.
import type { Locator, Page } from '@playwright/test';

import { PRACTICE_KEY, seedGame } from '../board';
import { deleteResults, expectResult, solveDaily, startDaily } from '../daily';
import { register } from '../account';
import { expect, newUsername, test, visit } from '../fixtures';
import { LADDER, LADDER_SOLUTION, ladderRow } from '../puzzles';

test.describe.configure({ mode: 'serial' });

/**
 * Stops what still moves: the sea of the boards (SVG animations), the mouse over a tooltip. Back
 * at the top: building bridges scrolls, and the sticky header would cover what scrolled under it.
 */
async function settle(page: Page) {
  await page.mouse.move(0, 0);
  await page.evaluate(async () => {
    window.scrollTo(0, 0);
    for (const svg of document.querySelectorAll('svg')) {
      svg.pauseAnimations();
      svg.setCurrentTime(0);
    }
    await document.fonts.ready;
  });
}

/** Hides elements, keeping the space they take. */
async function hide(...locators: Locator[]) {
  for (const locator of locators) {
    for (const element of await locator.all()) await element.evaluate((el: HTMLElement) => (el.style.visibility = 'hidden'));
  }
}

async function expectPage(page: Page, name: string, hidden: Locator[] = []) {
  await hide(...hidden);
  await settle(page);
  await expect(page).toHaveScreenshot(`${name}.png`);
}

/** A card of the page, by a heading in it. */
const card = (page: Page, heading: RegExp) =>
  page.locator('[data-name="Card"]').filter({ has: page.getByRole('heading', { name: heading }) });

/** The header of the daily: today's weekday, date, number and difficulty. */
const dailyHeader = (page: Page) =>
  page
    .locator('main section div')
    .filter({ has: page.getByRole('heading', { level: 1, name: /^Daily bridge #/ }) })
    .nth(-2);

test('home, the daily before the start', async ({ page }) => {
  await visit(page, '/');
  await expect(page.getByRole('heading', { name: "Today's bridge" })).toBeVisible();
  await expect(page.getByText('Crossed today: 2')).toBeVisible();
  await expectPage(page, 'home', [
    dailyHeader(page),
    // The board behind the cover has today's size
    page.locator('main [aria-hidden="true"]').filter({ has: page.locator('.board-svg') }),
    card(page, /^This week/),
  ]);
});

test('practice, a board in play', async ({ page }) => {
  // The clock stands still, it would tick in the corner (a timer of one second on the seeded
  // 00:00). Paused before the page loads: the page starts with it paused and its timer never
  // fires, however long loading takes. Paused after the visit, the second between install and
  // pauseAt could be gone on a busy runner ("Cannot fast-forward to the past"), or the timer
  // could tick first.
  await page.clock.install({ time: new Date('2026-10-19T09:00:00Z') });
  await page.clock.pauseAt(new Date('2026-10-19T09:00:01Z'));
  const bridges = LADDER_SOLUTION.map((b, e) => (e === ladderRow(3) || e === ladderRow(4) ? 0 : b));
  await seedGame(page, PRACTICE_KEY, { puzzle: LADDER, bridges, solution: LADDER_SOLUTION, status: 'Playing', moves: 7, hints: 0 });
  await visit(page, '/practice');
  await expect(page.getByText('6 / 10')).toBeVisible();
  // Checked on its own: a digit more or less is within the screenshot's tolerance
  await expect(page.getByText('00:00', { exact: true })).toBeVisible();
  await expectPage(page, 'practice');
});

test('ranks', async ({ page }) => {
  await visit(page, '/ranks');
  // The first card, the monthly race below has the same buttons
  const today = page.locator('[data-name="Card"]').filter({ has: page.getByRole('button', { name: /Earlier/ }) }).first();
  // The seeded two of today, on the podium
  await expect(today.locator('li.podium-step')).toHaveCount(2);
  await expectPage(page, 'ranks', [
    // The weekday, number, date and difficulty of the day
    today.locator('span.hanko'),
    today.locator('[data-name="CardTitle"]'),
    today.locator('[data-name="CardDescription"]'),
    // The month so far
    card(page, /^Monthly race/),
  ]);
});

for (const [locale, name] of [
  ['ja', 'Japanese'],
  ['ar', 'Arabic, right to left'],
] as const) {
  test(`rules in ${name}`, async ({ page }) => {
    await visit(page, `/${locale}/rules`);
    await expectPage(page, `rules-${locale}`);
  });
}

test('the daily result', async ({ page }) => {
  // With an account: a guest's card has their generated name, which may take a second line
  const username = newUsername('visual');
  await visit(page, '/account');
  await register(page, username, 'screenshots-of-bridges');
  try {
    await visit(page, '/');
    const { puzzle, solution } = await startDaily(page);
    await solveDaily(page, puzzle, solution);
    const result = await expectResult(page);
    await hide(
      // Toasts come and go
      page.locator('.pointer-events-none.fixed.z-50'),
      result.locator('p.font-pixel.text-3xl'),
      result.getByText(/^Rank \d+ of \d+ today$/),
      result.getByText(/points in the monthly race/),
    );
    await settle(page);
    await expect(result).toHaveScreenshot('daily-result.png');
  } finally {
    // The ranks show the seeded day again, also to a retry
    deleteResults(username);
  }
});
