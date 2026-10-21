// The replay theatre 再生: a group's day played back from the members' move logs, everyone at
// once on one board or side by side, with the timeline, the keys and the finish line. Today's
// stays closed until you finished it yourself, and only members get a group's logs.
import type { Page } from '@playwright/test';

import { solveDaily, startDaily } from '../daily';
import { expect, hydrated, signIn, test, visit } from '../fixtures';
import { REPLAY_DAY_BACK, SEEDED_GROUP } from '../seed';

const MS_PER_DAY = 86_400_000;
const today = () => Math.floor(Date.now() / MS_PER_DAY);

const clock = (page: Page) => page.getByTestId('replay-clock').locator('span').first();

/** The clock's time in ms, from "1:23.4". */
async function clockMs(page: Page) {
  const text = (await clock(page).innerText()).trim();
  const [m, s] = text.split(':');
  return Math.round((Number(m) * 60 + Number(s)) * 1000);
}

test('the seeded group watches yesterday: play, seek, step, side by side', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, `/groups/${SEEDED_GROUP.id}`);
  const card = page.getByTestId('replay-card');
  // aiko hasn't played today (seed.ts): today's replays wait
  await expect(card).toContainText("Finish today's daily to watch today's solves.");
  await expect(card.getByTestId('watch-today')).toHaveCount(0);
  const day = today() - REPLAY_DAY_BACK;
  await card.locator(`a[data-day="${day}"]`).click();
  await expect(page).toHaveURL(new RegExp(`/groups/${SEEDED_GROUP.id}/replay/${day}$`));
  await hydrated(page);

  const stage = page.getByTestId('replay-stage');
  await expect(stage).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: 'Replay theatre' })).toBeVisible();
  // A row per member, the ones who played with a toggle
  const legend = page.getByTestId('replay-legend');
  await expect(legend.locator('li[data-lane]')).toHaveCount(SEEDED_GROUP.members.length);
  const players = legend.locator('li[data-lane] button[aria-pressed]');
  const playing = await players.count();
  expect(playing).toBeGreaterThan(0);

  // It plays by itself, the clock runs and bridges appear in the members' colours
  await expect(page.getByTestId('replay-play')).toHaveAttribute('data-playing', 'true');
  await expect.poll(() => clockMs(page)).toBeGreaterThan(0);
  await expect(stage.locator('g[data-edge]').first()).toBeVisible();
  await page.getByTestId('replay-play').click();
  await expect(page.getByTestId('replay-play')).toHaveAttribute('data-playing', 'false');

  // Seeking to the end: everyone across the finish line, in order
  const timeline = page.getByTestId('replay-timeline');
  await timeline.scrollIntoViewIfNeeded();
  const box = (await timeline.boundingBox())!;
  await page.mouse.click(box.x + box.width - 1, box.y + box.height / 2);
  const finish = page.getByTestId('replay-finish');
  await expect(finish.locator('li[data-place]')).toHaveCount(playing);
  await expect(finish.locator('li[data-place="1"]')).toBeVisible();
  // A pennant per finisher on the timeline
  await expect(page.locator('[data-flag]')).toHaveCount(playing);

  // Keys: Home to the start, an arrow to the first move
  await timeline.focus();
  await page.keyboard.press('Home');
  await expect(clock(page)).toHaveText('0:00.0');
  await expect(finish.locator('li[data-place]')).toHaveCount(0);
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => clockMs(page)).toBeGreaterThan(0);
  const first = await clockMs(page);
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => clockMs(page)).toBeGreaterThan(first);

  // Hiding a member takes their lane off the board
  await players.first().click();
  await expect(players.first()).toHaveAttribute('aria-pressed', 'false');
  await page.getByRole('button', { name: 'Everyone' }).click();
  await expect(players.first()).toHaveAttribute('aria-pressed', 'true');

  // Side by side: a little board each
  await page.getByRole('button', { name: 'Side by side' }).click();
  await expect(page.getByTestId('replay-apart').locator('li[data-lane]')).toHaveCount(playing);
  await page.getByRole('button', { name: 'Together' }).click();
  await expect(stage).toBeVisible();

  // The day before, and back
  await page.getByRole('link', { name: 'The day before' }).click();
  await expect(page).toHaveURL(new RegExp(`/replay/${day - 1}$`));
  await hydrated(page);
  await page.getByRole('link', { name: 'The day after' }).click();
  await expect(page).toHaveURL(new RegExp(`/replay/${day}$`));

  // Today: no peeking before finishing it
  await visit(page, `/groups/${SEEDED_GROUP.id}/replay/${today()}`);
  await expect(page.getByTestId('replay-locked')).toBeVisible();
  await expect(page.getByTestId('replay-stage')).toHaveCount(0);
});

test('strangers get no replays', async ({ page }) => {
  await visit(page, `/groups/${SEEDED_GROUP.id}/replay/${today() - REPLAY_DAY_BACK}`);
  await expect(page.getByTestId('replay-missing')).toBeVisible();
  await expect(page.getByTestId('replay-stage')).toHaveCount(0);
});

test('after solving: your own replay, and the group tab opens today’s theatre', async ({ page }) => {
  test.slow();
  // A guest makes a group and solves the daily
  await visit(page, '/groups');
  await page.getByLabel('Group name').fill('Replay Ramblers');
  await page.getByRole('button', { name: 'Create the group' }).click();
  await expect(page.getByTestId('group-ready')).toBeVisible();
  await page.goto('/');
  await hydrated(page);
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);
  // The win card has the way to their own solve, with the check and the hints on the timeline
  const win = page.getByRole('dialog', { name: /Well done!/ });
  await expect(win).toBeVisible();
  await win.getByTestId('win-replay').click();
  await expect(page).toHaveURL(new RegExp(`/replay/${today()}$`));
  await hydrated(page);
  await expect(page.getByRole('heading', { level: 1, name: 'Your solve' })).toBeVisible();
  await expect(page.getByTestId('replay-stage')).toBeVisible();
  await expect(page.locator('[data-mark="check"]')).toHaveCount(1);
  expect(await page.locator('[data-mark="hint"]').count()).toBeGreaterThan(0);
  await page.keyboard.press('End');
  await expect(page.getByTestId('replay-finish').locator('li[data-place="1"]')).toBeVisible();
  // The board at the end is the solution: every bridge of it is there
  const bridges = solution.filter((b) => b > 0).length;
  await expect(page.getByTestId('replay-stage').locator('g[data-edge]')).toHaveCount(bridges);

  // The group tab of the daily page leads to today's theatre, open now
  await page.goto('/');
  await hydrated(page);
  await page.getByRole('tab', { name: /Replay Ramblers/ }).click();
  await page.getByTestId('watch-group').click();
  await expect(page).toHaveURL(new RegExp(`/groups/\\d+/replay/${today()}$`));
  await hydrated(page);
  await expect(page.getByTestId('replay-stage')).toBeVisible();
  await expect(page.getByTestId('replay-legend').locator('li[data-lane]')).toHaveCount(1);
});
