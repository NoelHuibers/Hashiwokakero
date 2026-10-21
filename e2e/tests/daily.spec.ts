// The daily puzzle from the cover to the leaderboard, as a guest and with an account made on the
// way. Every test plays as a new player: there is one daily a day per player.
import type { Locator, Page } from '@playwright/test';

import { register, signInWith, signOut } from '../account';
import { addBridge, board, DAILY_KEY, missing, saved } from '../board';
import { expectResult, solveDaily, startDaily } from '../daily';
import { expect, newUsername, test, visit } from '../fixtures';

/** The card over the board after this tab's finish. */
const winCard = (page: Page) => page.getByRole('dialog', { name: /Well done!/ });

/** The first card of the ranks page, the monthly race below has the same buttons. */
const todaysBoard = (page: Page) =>
  page.locator('[data-name="Card"]').filter({ has: page.getByRole('button', { name: /Earlier/ }) }).first();

test('a guest solves the daily and is on the leaderboard', async ({ page, context }) => {
  await visit(page, '/');
  await expect(page.getByText("No account needed, you'll play as a guest.")).toBeVisible();
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);

  const card = await expectResult(page);
  const name = (await card.locator('b').textContent())!;
  expect(name).toMatch(/^\p{Lu}\p{Ll}+ \p{Lu}\p{Ll}+ \d+$/u);
  const time = (await card.locator('p.font-pixel.text-3xl').textContent())!;

  // This tab's win covers the board until it's put away, the result card stays
  const win = winCard(page);
  await expect(win).toBeFocused();
  await expect(win.getByText(/^Rank \d+ of \d+ today$/)).toBeVisible();
  await expect(win.getByText(time, { exact: true })).toBeVisible();
  await win.getByRole('button', { name: 'Admire' }).click();
  await expect(win).toHaveCount(0);
  await expect(card).toBeVisible();
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);

  // Today's ranking next to the board, with the penalties. A row of the table, or a step of the
  // ranks page's podium.
  const mine = (root: Locator) => root.locator('tr, li.podium-step').filter({ hasText: name });
  await expect(mine(page.locator('aside'))).toContainText(/you/i);
  await expect(mine(page.locator('aside'))).toContainText(/\d+h 1c/);
  await expect(mine(page.locator('aside'))).toContainText(time);

  // Finished is finished: after a reload, and in another tab
  const finished = async (p: Page) => {
    await expect(p.getByRole('heading', { name: /Well done!/ })).toBeVisible();
    // The win card is for the tab that finished, right then
    await expect(p.getByRole('dialog')).toHaveCount(0);
    await expect(p.locator('p.font-pixel.text-3xl')).toHaveText(time);
    await expect(p.getByRole('button', { name: /^Start/ })).toHaveCount(0);
    for (const button of [/^Hint/, /^Check/, /^Give up/, /Undo/]) {
      await expect(p.getByRole('button', { name: button })).toBeDisabled();
    }
    expect((await saved(p, DAILY_KEY)).bridges).toEqual(solution);
  };
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await finished(page);
  const other = await context.newPage();
  await visit(other, '/');
  await finished(other);

  // The ranks page has them too, on today's board
  await visit(other, '/ranks');
  await expect(mine(todaysBoard(other))).toContainText(/you/i);
  await expect(mine(todaysBoard(other))).toContainText(time);
  await other.close();
});

test('sign up, sign out and in again, then the daily counts for the account', async ({ page }) => {
  const username = newUsername('daily');
  const password = 'bridges-all-the-way';
  await visit(page, '/account');
  await register(page, username, password);
  await expect(page.getByText(`Welcome, ${username}!`)).toBeVisible();
  await signOut(page);
  await signInWith(page, username, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();

  await page.getByRole('link', { name: /Daily/ }).first().click();
  await expect(page.getByText(`Playing as ${username}`, { exact: true })).toBeVisible();
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);
  const card = await expectResult(page);
  // Not a guest any more: nothing to claim
  await expect(card.getByText(/Claim your name/)).toHaveCount(0);
  // The win card has the stamp, and no toast repeats it
  const win = winCard(page);
  await expect(win.getByText('New stamps · 判子')).toBeVisible();
  await expect(win.getByRole('listitem').filter({ hasText: 'First bridge' })).toBeVisible();
  await expect(page.getByText('New stamp: First bridge!')).toHaveCount(0);

  // The account has the result and the stamp
  await win.getByRole('link', { name: 'Stamp book →' }).click();
  await expect(page).toHaveURL(/\/account$/);
  await expect(page.getByText('No dailies yet.')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Stamp book' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'First bridge' })).toBeVisible();
  await expect(page.getByText(/^\d+ of \d+ stamps collected\./)).not.toHaveText(/^0 of/);

  // And the leaderboard has the name, without the guest label
  await visit(page, '/ranks');
  const row = todaysBoard(page).locator('tr, li.podium-step').filter({ hasText: username });
  await expect(row).toContainText(/you/i);
  await expect(row).not.toContainText('guest');
});

test('the move log saves bridge changes and stays idle while the clock ticks', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    const set = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key === 'hashi.daily.log') {
        const count = Number(sessionStorage.getItem('test.daily.log.writes') ?? '0');
        set.call(sessionStorage, 'test.daily.log.writes', String(count + 1));
      }
      set.call(this, key, value);
    };
  });
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  const writes = () => page.evaluate(() => Number(sessionStorage.getItem('test.daily.log.writes') ?? '0'));
  const log = () => page.evaluate(() => JSON.parse(localStorage.getItem('hashi.daily.log') ?? 'null'));
  const first = solution.findIndex((b) => b > 0);
  await addBridge(page, DAILY_KEY, puzzle, first);
  await expect.poll(writes).toBe(1);
  expect((await log()).events).toEqual([expect.objectContaining({ edge: first, count: 1, tap: true })]);

  const timer = page.getByText('Time · 時間', { exact: true }).locator('..').locator('span').first();
  const beforeTick = (await timer.textContent())!;
  await page.clock.runFor(1_000);
  await expect(timer).not.toHaveText(beforeTick);
  expect(await writes()).toBe(1);
  await page.getByRole('button', { name: /Undo/ }).click();
  await expect.poll(writes).toBe(2);
  expect((await log()).events.at(-1)).toMatchObject({ edge: first, count: 0, tap: false });
  await page.getByRole('button', { name: /Redo/ }).click();
  await expect.poll(writes).toBe(3);
  expect((await log()).events.at(-1)).toMatchObject({ edge: first, count: 1, tap: false });
  const before = await log();
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(page.locator('svg.board-svg g.island').first()).toBeVisible();
  await page.clock.runFor(1_000);
  expect(await writes()).toBe(3);
  expect(await log()).toEqual(before);
});

const paidPath = '**/api/daily_paid_action';
const actionStatus = (page: Page) => page.getByTestId('daily-action-status');
const penaltyValue = (page: Page) => page.getByText('Penalty · 罰', { exact: true }).locator('..').locator('span').first();

// These use the real server, with only transport timing/failure controlled by the browser.
// They require matching built WASM/server assets (the same prerequisite as the daily suite).
test('a delayed paid action blocks double taps, keyboard edits and other actions immediately', async ({ page }) => {
  await visit(page, '/');
  await startDaily(page);
  const before = await saved(page, DAILY_KEY);
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  let requests = 0;
  await page.route(paidPath, async (route) => {
    requests++;
    await held;
    await route.continue();
  });
  const hint = page.getByRole('button', { name: /^Hint/ });
  try {
    // Two DOM clicks in one turn exercise the synchronous handler guard, before Leptos has
    // rendered disabled attributes. A disabled flag rendered on the next frame is too late.
    await hint.evaluate((button) => { (button as HTMLButtonElement).click(); (button as HTMLButtonElement).click(); });
    await expect.poll(() => requests).toBe(1);
    await expect(actionStatus(page)).toHaveText('Waiting for the server…');
    for (const name of [/^Hint/, /^Check/, /^Give up/, /Undo/, /Redo/, /^Clear/]) {
      await expect(page.getByRole('button', { name })).toBeDisabled();
    }
    const svg = board(page);
    await svg.focus();
    await svg.press('Enter');
    await svg.press('ArrowRight');
    await svg.press('Space');
    expect((await saved(page, DAILY_KEY)).bridges).toEqual(before.bridges);
    expect((await saved(page, DAILY_KEY)).moves).toBe(before.moves);
    expect(requests).toBe(1);
  } finally {
    release();
  }
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+30s');
  expect(requests).toBe(1);
  await expect(hint).toBeEnabled();
});

for (const action of ['Hint', 'Check'] as const) {
  test(`a lost successful ${action} reply survives reload and retry without another penalty`, async ({ page }) => {
    await visit(page, '/');
    await startDaily(page);
    const bodies: Record<string, unknown>[] = [];
    await page.route(paidPath, async (route) => {
      bodies.push(route.request().postDataJSON());
      const response = await route.fetch(); // the real server commits the charge
      if (bodies.length === 1) {
        // Simulate a reply that cannot be decoded after the server committed successfully.
        await route.fulfill({ response, contentType: 'application/json', body: '{"lost":"reply"}' });
      } else {
        await route.fulfill({ response });
      }
    });
    const button = page.getByRole('button', { name: new RegExp(`^${action}`) });
    await button.click();
    await expect(actionStatus(page)).toContainText('Press the same action again');
    await expect(button).toBeEnabled(); // the failure clears pending
    await expect(page.getByRole('button', { name: /^Give up/ })).toBeDisabled();
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
    await expect(actionStatus(page)).toContainText('Press the same action again');
    await button.click();
    await expect(actionStatus(page)).toBeEmpty();
    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toEqual(bodies[0]);
    const penalty = action === 'Hint' ? '+30s' : '+10s';
    await expect(penaltyValue(page)).toHaveText(penalty);
    expect(await page.evaluate(() => Object.keys(localStorage).filter((key) => key.startsWith('hashi.daily.action.')))).toEqual([]);
  });
}

test('a failed paid request clears pending and retries the original intent', async ({ page }) => {
  await visit(page, '/');
  await startDaily(page);
  const bodies: Record<string, unknown>[] = [];
  await page.route(paidPath, async (route) => {
    bodies.push(route.request().postDataJSON());
    if (bodies.length === 1) {
      // No server write this time. The UI cannot distinguish this from a lost success.
      await route.fulfill({ contentType: 'application/json', body: 'null' });
    } else {
      await route.continue();
    }
  });
  const check = page.getByRole('button', { name: /^Check/ });
  await check.click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await expect(check).toBeEnabled();
  await expect(penaltyValue(page)).toHaveText('+0s');
  await check.click();
  await expect(actionStatus(page)).toBeEmpty();
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toEqual(bodies[0]);
  await expect(penaltyValue(page)).toHaveText('+10s');
});

test('a paid reply arriving after leaving the daily cannot mutate its persistent board', async ({ page }) => {
  await visit(page, '/');
  await startDaily(page);
  const before = (await saved(page, DAILY_KEY)).bridges;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  let committed = false;
  await page.route(paidPath, async (route) => {
    const response = await route.fetch();
    committed = true;
    await held;
    await route.fulfill({ response });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect.poll(() => committed).toBe(true);
  try {
    await page.getByRole('link', { name: /Practice/ }).first().click();
    await expect(page).toHaveURL(/\/practice/);
  } finally {
    release();
  }
  await page.waitForLoadState('networkidle');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before);
  await page.getByRole('link', { name: /Daily/ }).first().click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before);
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+30s');
});


test('a final paid hint flushes its move log and submits exactly once', async ({ page }) => {
  await visit(page, '/');
  const { solution } = await startDaily(page);
  let submissions = 0;
  page.on('request', (request) => {
    if (request.url().endsWith('/api/submit_daily_attempt')) submissions++;
  });
  const hint = page.getByRole('button', { name: /^Hint/ });
  while (missing((await saved(page, DAILY_KEY)).bridges, solution).length > 0) {
    const before = (await saved(page, DAILY_KEY)).bridges;
    await hint.click();
    await expect.poll(async () => (await saved(page, DAILY_KEY)).bridges).not.toEqual(before);
  }
  await expectResult(page);
  expect(submissions).toBe(1);
  const log = await page.evaluate(() => JSON.parse(localStorage.getItem('hashi.daily.log') ?? 'null'));
  expect(log.events.length).toBeGreaterThan(0);
  expect(log.events.every((event: { tap: boolean }) => !event.tap)).toBe(true);
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
});

test('a solved daily whose submit failed waits to be submitted again', async ({ page }) => {
  await visit(page, '/');
  const { solution } = await startDaily(page);
  let submissions = 0;
  await page.route('**/api/submit_daily_attempt', async (route) => {
    submissions++;
    // The server never sees the first one, and the page can't read the reply
    if (submissions === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else await route.continue();
  });
  const hint = page.getByRole('button', { name: /^Hint/ });
  while (missing((await saved(page, DAILY_KEY)).bridges, solution).length > 0) {
    const before = (await saved(page, DAILY_KEY)).bridges;
    await hint.click();
    await expect.poll(async () => (await saved(page, DAILY_KEY)).bridges).not.toEqual(before);
  }
  const again = page.getByRole('button', { name: 'Retry result' });
  await expect(again).toBeVisible();
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  // Still solved and locked, and it doesn't retry on its own
  await expect(page.getByRole('button', { name: /Undo/ })).toBeDisabled();
  await page.waitForLoadState('networkidle');
  expect(submissions).toBe(1);
  await expect(winCard(page)).toHaveCount(0);

  await again.click();
  await expect(winCard(page)).toBeVisible();
  expect(submissions).toBe(2);
  await expect(again).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(winCard(page)).toHaveCount(0);
  await expectResult(page);
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
});

test('closing the tab after a lost success preserves the paid intent for the next tab', async ({ page, context, problems }) => {
  await visit(page, '/');
  await startDaily(page);
  await page.route(paidPath, async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, contentType: 'application/json', body: 'null' });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await page.close();
  const next = await context.newPage();
  next.on('pageerror', (error) => problems.push(`page error: ${error.message}`));
  next.on('console', (message) => {
    if (message.type() === 'error') problems.push(`console error: ${message.text()}`);
  });
  try {
    await visit(next, '/');
    await expect(actionStatus(next)).toContainText('Press the same action again');
    await next.getByRole('button', { name: /^Hint/ }).click();
    await expect(actionStatus(next)).toBeEmpty();
    await expect(penaltyValue(next)).toHaveText('+30s');
  } finally {
    await next.close();
  }
});

test('unavailable intent storage prevents a charge and releases pending', async ({ page }) => {
  await page.addInitScript(() => {
    const set = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key.startsWith('hashi.daily.action.')) throw new DOMException('test storage unavailable', 'QuotaExceededError');
      set.call(this, key, value);
    };
  });
  await visit(page, '/');
  await startDaily(page);
  let requests = 0;
  page.on('request', (request) => {
    if (request.url().endsWith('/api/daily_paid_action')) requests++;
  });
  const hint = page.getByRole('button', { name: /^Hint/ });
  await hint.click();
  await expect(page.getByText('Browser storage is required to retry this action safely. Enable it and try again.')).toBeVisible();
  await expect(hint).toBeEnabled();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+0s');
  expect(requests).toBe(0);
});

for (const action of ['Hint', 'Check'] as const) {
  test(`a lost successful guest ${action} is recovered after login without a new purchase`, async ({ page }) => {
    const username = newUsername('recover');
    const password = 'recover-bridges-safely';
    await visit(page, '/account');
    await register(page, username, password);
    await signOut(page);
    await visit(page, '/');
    await startDaily(page);
    const before = (await saved(page, DAILY_KEY)).bridges;
    let purchases = 0;
    await page.route(paidPath, async (route) => {
      purchases++;
      const response = await route.fetch();
      await route.fulfill({ response, contentType: 'application/json', body: 'null' });
    });
    await page.getByRole('button', { name: new RegExp(`^${action}`) }).click();
    await expect(actionStatus(page)).toContainText('Press the same action again');
    expect((await saved(page, DAILY_KEY)).bridges).toEqual(before);
    await visit(page, '/account');
    await signInWith(page, username, password);
    await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
    await visit(page, '/');
    await expect(actionStatus(page)).toContainText('recover an earlier action after signing in');
    await expect(page.getByRole('button', { name: /^Give up/ })).toBeDisabled();
    // Recovering the other action first must never buy it. The proven original action is
    // restored, and its next click only reads the same receipt again.
    const other = action === 'Hint' ? 'Check' : 'Hint';
    await page.getByRole('button', { name: new RegExp(`^${other}`) }).click();
    await expect(actionStatus(page)).toContainText('Press the same action again');
    await expect(page.getByRole('button', { name: new RegExp(`^${other}`) })).toBeDisabled();
    await page.reload(); // original storage key remains recoverable after another reload
    await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
    await expect(actionStatus(page)).toContainText('recover an earlier action after signing in');
    await page.getByRole('button', { name: new RegExp(`^${action}`) }).click();
    await expect(actionStatus(page)).toBeEmpty();
    expect(purchases).toBe(1);
    await expect(penaltyValue(page)).toHaveText(action === 'Hint' ? '+30s' : '+10s');
    if (action === 'Hint') expect((await saved(page, DAILY_KEY)).bridges).not.toEqual(before);
    expect(await page.evaluate(() => Object.keys(localStorage).filter((key) => key.startsWith('hashi.daily.action.')))).toEqual([]);
  });
}

test('a conflicting account attempt never adopts a lost guest hint', async ({ page }) => {
  const username = newUsername('conflict');
  const password = 'conflicting-bridges';
  await visit(page, '/account');
  await register(page, username, password);
  await visit(page, '/');
  await startDaily(page); // target account already owns today's attempt
  const before = (await saved(page, DAILY_KEY)).bridges;
  await visit(page, '/account');
  await signOut(page);
  await visit(page, '/');
  await startDaily(page);
  let purchases = 0;
  await page.route(paidPath, async (route) => {
    purchases++;
    const response = await route.fetch();
    if (purchases === 1) await route.fulfill({ response, contentType: 'application/json', body: 'null' });
    else await route.fulfill({ response });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await visit(page, '/account');
  await signInWith(page, username, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
  await visit(page, '/');
  await expect(board(page)).toBeVisible();
  await expect(penaltyValue(page)).toHaveText('+0s');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before);
  expect(purchases).toBe(1);
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+30s');
  expect(purchases).toBe(2); // a new, intentional purchase for the target account
});

test('signing out and switching accounts preserves the former account intent without adoption', async ({ page }) => {
  const first = newUsername('first');
  const second = newUsername('second');
  const password = 'separate-bridge-owners';
  await visit(page, '/account');
  await register(page, first, password);
  await visit(page, '/');
  await startDaily(page);
  await page.route(paidPath, async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, contentType: 'application/json', body: 'null' });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await visit(page, '/account');
  await signOut(page);
  await register(page, second, password);
  await visit(page, '/');
  await startDaily(page);
  await expect(penaltyValue(page)).toHaveText('+0s');
  await expect(actionStatus(page)).toBeEmpty();
  expect(await page.evaluate(() => Object.keys(localStorage).filter((key) => key.startsWith('hashi.daily.action.')).length)).toBe(1);
  await visit(page, '/account');
  await signOut(page);
  await signInWith(page, first, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${first}` })).toBeVisible();
  await visit(page, '/');
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await page.unroute(paidPath);
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+30s');
});

test('an unknown transfer lookup blocks fresh purchases until recovery succeeds', async ({ page }) => {
  const username = newUsername('lookup');
  const password = 'unknown-receipt-outcome';
  await visit(page, '/account');
  await register(page, username, password);
  await signOut(page);
  await visit(page, '/');
  await startDaily(page);
  let purchases = 0;
  await page.route(paidPath, async (route) => {
    purchases++;
    const response = await route.fetch();
    await route.fulfill({ response, contentType: 'application/json', body: 'null' });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await visit(page, '/account');
  await signInWith(page, username, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
  await visit(page, '/');
  let lookups = 0;
  await page.route('**/api/daily_recover_actions', async (route) => {
    lookups++;
    if (lookups === 1) await route.fulfill({ contentType: 'application/json', body: '{"unknown":true}' });
    else await route.continue();
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect.poll(() => lookups).toBe(1);
  await expect(actionStatus(page)).toContainText('recover an earlier action after signing in');
  await expect(page.getByRole('button', { name: /^Give up/ })).toBeDisabled();
  expect(purchases).toBe(1);
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  expect(purchases).toBe(1);
});

test('a guest merge in another tab preserves a committed hint whose reply outlives the daily page', async ({ page, context, problems }) => {
  const username = newUsername('tabmerge');
  const password = 'cross-tab-bridge-receipt';
  await visit(page, '/account');
  await register(page, username, password);
  await signOut(page);
  await visit(page, '/');
  await startDaily(page);
  const before = (await saved(page, DAILY_KEY)).bridges;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  let committed = false;
  let purchases = 0;
  await page.route(paidPath, async (route) => {
    purchases++;
    const response = await route.fetch();
    committed = true;
    await held;
    await route.fulfill({ response });
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect.poll(() => committed).toBe(true);
  const other = await context.newPage();
  other.on('pageerror', (error) => problems.push(`page error: ${error.message}`));
  try {
    await page.getByRole('link', { name: /Practice/ }).first().click();
    await expect(page).toHaveURL(/\/practice/);
    await visit(other, '/account');
    await signInWith(other, username, password);
    await expect(other.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
  } finally {
    release();
    await other.close();
  }
  await page.waitForLoadState('networkidle');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before);
  await visit(page, '/'); // a fresh auth snapshot sees the shared cookie's new identity
  await expect(actionStatus(page)).toContainText('recover an earlier action after signing in');
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  expect(purchases).toBe(1);
  await expect(penaltyValue(page)).toHaveText('+30s');
});

test('a delayed quiet refresh cannot roll back a paid counter or touch a disposed daily', async ({ page }) => {
  await visit(page, '/');
  await startDaily(page);
  for (const dispose of [false, true]) {
    let release!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    let captured = false;
    await page.route('**/api/daily_today', async (route) => {
      const response = await route.fetch();
      captured = true;
      await held;
      await route.fulfill({ response });
    });
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect.poll(() => captured).toBe(true);
    try {
      if (dispose) {
        await page.getByRole('link', { name: /Practice/ }).first().click();
        await expect(page).toHaveURL(/\/practice/);
      } else {
        await page.getByRole('button', { name: /^Check/ }).click();
        await expect(actionStatus(page)).toBeEmpty();
        await expect(penaltyValue(page)).toHaveText('+10s');
      }
    } finally {
      release();
    }
    await page.waitForLoadState('networkidle');
    if (!dispose) await expect(penaltyValue(page)).toHaveText('+10s');
    await page.unroute('**/api/daily_today');
  }
});

test('a definitive missing transferred receipt ends recovery without purchasing on that click', async ({ page }) => {
  const username = newUsername('nocharge');
  const password = 'recover-without-purchase';
  await visit(page, '/account');
  await register(page, username, password);
  await signOut(page);
  await visit(page, '/');
  await startDaily(page);
  let purchases = 0;
  await page.route(paidPath, async (route) => {
    purchases++;
    if (purchases === 1) {
      // A failed transport before the server saw the original request leaves no receipt.
      await route.fulfill({ contentType: 'application/json', body: 'null' });
    } else {
      await route.continue();
    }
  });
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toContainText('Press the same action again');
  await visit(page, '/account');
  await signInWith(page, username, password);
  await expect(page.getByRole('heading', { name: `こんにちは, ${username}` })).toBeVisible();
  await visit(page, '/');
  await expect(actionStatus(page)).toContainText('recover an earlier action after signing in');
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+0s');
  expect(purchases).toBe(1);
  // Only the next, explicit purchase click can charge this account.
  await page.getByRole('button', { name: /^Hint/ }).click();
  await expect(actionStatus(page)).toBeEmpty();
  await expect(penaltyValue(page)).toHaveText('+30s');
  expect(purchases).toBe(2);
});

// Transient failures must preserve the board and wait for an explicit, bounded retry.
const metadataStatus = (page: Page) => page.getByTestId('daily-metadata-status');
const rankingStatus = (page: Page) => page.getByTestId('daily-ranking-status');
const submitStatus = (page: Page) => page.getByTestId('daily-submit-status');
const submitPath = '**/api/submit_daily_attempt';
const reconcilePath = '**/api/daily_attempt_info';

test('initial metadata failure has a persistent manual retry with an immediate pending guard', async ({ page }) => {
  let requests = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  await page.route('**/api/daily_today', async (route) => {
    requests++;
    if (requests === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else { await held; await route.continue(); }
  });
  await visit(page, '/');
  await expect(metadataStatus(page)).toContainText("Couldn't load today's puzzle.");
  await expect(page.getByRole('button', { name: /^Start/ })).toHaveCount(0);
  await expect(rankingStatus(page)).toBeEmpty(); // ranking succeeded independently
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.waitForLoadState('networkidle');
  expect(requests).toBe(1); // no automatic error loop, including focus
  try {
    await page.getByRole('button', { name: 'Retry puzzle', exact: true }).evaluate((button) => {
      (button as HTMLButtonElement).click(); (button as HTMLButtonElement).click();
    });
    await expect.poll(() => requests).toBe(2);
    await expect(metadataStatus(page)).toContainText('Loading');
    expect(requests).toBe(2);
  } finally { release(); }
  await expect(metadataStatus(page)).toBeEmpty();
  await expect(page.getByRole('button', { name: /^Start/ })).toBeEnabled();
});

test('ranking failure retries independently and preserves the running board', async ({ page }) => {
  let rankings = 0;
  let retrySucceeds = false;
  await page.route('**/api/daily_leaderboard', async (route) => {
    rankings++;
    if (!retrySucceeds) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else await route.continue();
  });
  await visit(page, '/');
  await expect(rankingStatus(page)).toContainText("Couldn't load the ranking.");
  const { puzzle, solution } = await startDaily(page);
  await page.waitForLoadState('networkidle'); // settle the new guest's identity refresh
  await addBridge(page, DAILY_KEY, puzzle, solution.findIndex((b) => b > 0));
  const before = await saved(page, DAILY_KEY);
  const beforeRetry = rankings;
  retrySucceeds = true;
  let metadata = 0;
  page.on('request', (request) => { if (request.url().endsWith('/api/daily_today')) metadata++; });
  await page.getByRole('button', { name: 'Retry ranking', exact: true }).evaluate((button) => {
    (button as HTMLButtonElement).click(); (button as HTMLButtonElement).click();
  });
  await expect(rankingStatus(page)).toBeEmpty();
  expect(rankings).toBe(beforeRetry + 1);
  expect(metadata).toBe(0);
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before.bridges);
  expect((await saved(page, DAILY_KEY)).moves).toBe(before.moves);
});

test('a lost submit success and failed reconciliation keep the solution until explicit recovery', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  let submissions = 0;
  let reads = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  await page.route(submitPath, async (route) => {
    submissions++;
    const response = await route.fetch(); // finish is committed, response is lost
    await route.fulfill({ response, contentType: 'application/json', body: 'null' });
  });
  await page.route(reconcilePath, async (route) => {
    reads++;
    if (reads <= 2) {
      if (reads === 2) await held;
      await route.fulfill({ contentType: 'application/json', body: 'null' });
    } else await route.continue();
  });
  await solveDaily(page, puzzle, solution);
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  const originalLog = await page.evaluate(() => localStorage.getItem('hashi.daily.log'));
  const penalty = await penaltyValue(page).textContent();
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
  for (const name of [/^Hint/, /^Check/, /^Give up/, /Undo/, /Redo/, /^Clear/]) {
    await expect(page.getByRole('button', { name })).toBeDisabled();
  }
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.waitForLoadState('networkidle');
  expect([submissions, reads]).toEqual([1, 1]);
  try {
    await page.getByRole('button', { name: 'Retry result', exact: true }).evaluate((button) => {
      (button as HTMLButtonElement).click(); (button as HTMLButtonElement).click();
    });
    await expect.poll(() => reads).toBe(2);
    await expect(submitStatus(page)).toHaveText('Saving your result…');
    expect(submissions).toBe(1);
  } finally { release(); }
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  expect(submissions).toBe(1); // a failed read must not dispatch a write
  await page.getByRole('button', { name: 'Retry result', exact: true }).click();
  await expect(submitStatus(page)).toBeEmpty();
  await expectResult(page);
  expect([submissions, reads]).toEqual([1, 3]); // read-only confirmation of the lost success
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
  expect(await page.evaluate(() => localStorage.getItem('hashi.daily.log'))).toBe(originalLog);
  await expect(penaltyValue(page)).toHaveText(penalty!);
});

test('an unsent submission can explicitly retry the original solved board and final log', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  const bodies: Record<string, unknown>[] = [];
  await page.route(submitPath, async (route) => {
    bodies.push(route.request().postDataJSON());
    if (bodies.length === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else await route.continue();
  });
  await solveDaily(page, puzzle, solution);
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
  const log = await page.evaluate(() => JSON.parse(localStorage.getItem('hashi.daily.log')!));
  // A competing tab can rewrite the legacy shared log key while recovery is pending.
  await page.evaluate(() => localStorage.setItem('hashi.daily.log', JSON.stringify({ day: -1, events: [] })));
  await page.getByRole('button', { name: 'Retry result', exact: true }).click();
  await expect(submitStatus(page)).toBeEmpty();
  await expectResult(page);
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toEqual(bodies[0]);
  expect(bodies[0].log).toEqual(log.events);
  expect(log.events.at(-1).tap).toBe(true); // the final hand-built bridge was flushed
});

test('a delayed submit response cannot touch a disposed daily page', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  let committed = false;
  await page.route(submitPath, async (route) => {
    const response = await route.fetch();
    committed = true;
    await held;
    await route.fulfill({ response });
  });
  await solveDaily(page, puzzle, solution);
  await expect.poll(() => committed).toBe(true);
  const before = await saved(page, DAILY_KEY);
  const log = await page.evaluate(() => localStorage.getItem('hashi.daily.log'));
  try {
    await page.getByRole('link', { name: /Practice/ }).first().click();
    await expect(page).toHaveURL(/\/practice/);
  } finally { release(); }
  await page.waitForLoadState('networkidle');
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before.bridges);
  expect(await page.evaluate(() => localStorage.getItem('hashi.daily.log'))).toBe(log);
  await visit(page, '/');
  await expectResult(page);
});

test('a transient metadata refresh failure preserves running bridges and allows manual recovery', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  await page.waitForLoadState('networkidle'); // settle the guest identity refresh before intercepting reads
  await addBridge(page, DAILY_KEY, puzzle, solution.findIndex((b) => b > 0));
  const before = await saved(page, DAILY_KEY);
  let reads = 0;
  await page.route('**/api/daily_today', async (route) => {
    reads++;
    if (reads === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else await route.continue();
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(metadataStatus(page)).toContainText("Couldn't load today's puzzle.");
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before.bridges);
  expect((await saved(page, DAILY_KEY)).moves).toBe(before.moves);
  await expect(page.getByRole('button', { name: /^Check/ })).toBeEnabled();
  await page.getByRole('button', { name: 'Retry puzzle', exact: true }).click();
  await expect(metadataStatus(page)).toBeEmpty();
  expect(reads).toBe(2);
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(before.bridges);
  expect((await saved(page, DAILY_KEY)).moves).toBe(before.moves);
});

test('a reload of an unconfirmed solved daily stays locked for explicit retry', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  let writes = 0;
  await page.route(submitPath, async (route) => {
    writes++;
    if (writes === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
    else await route.continue();
  });
  await solveDaily(page, puzzle, solution);
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  const log = await page.evaluate(() => localStorage.getItem('hashi.daily.log'));
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(submitStatus(page)).toContainText('Your result could not be confirmed.');
  expect(writes).toBe(1);
  expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
  expect(await page.evaluate(() => localStorage.getItem('hashi.daily.log'))).toBe(log);
  await expect(page.getByRole('button', { name: /^Clear/ })).toBeDisabled();
  await page.getByRole('button', { name: 'Retry result', exact: true }).click();
  await expectResult(page);
  expect(writes).toBe(2);
});

for (const failFirst of [false, true]) {
  test(`a solved SPA return discovers metadata${failFirst ? ' after a manual retry' : ''}`, async ({ page }) => {
    await visit(page, '/');
    const { puzzle, solution } = await startDaily(page);
    await solveDaily(page, puzzle, solution);
    await expectResult(page);
    await page.keyboard.press('Escape');
    await page.getByRole('link', { name: /Practice/ }).first().click();
    await expect(page).toHaveURL(/\/practice/);
    await page.evaluate(() => document.documentElement.dataset.returnMarker = 'same-document');
    let reads = 0;
    await page.route('**/api/daily_today', async (route) => {
      reads++;
      if (failFirst && reads === 1) await route.fulfill({ contentType: 'application/json', body: 'null' });
      else await route.continue();
    });
    await page.getByRole('link', { name: /Daily/ }).first().click();
    if (failFirst) {
      await expect(metadataStatus(page)).toContainText("Couldn't load today's puzzle.");
      const retry = page.getByRole('button', { name: 'Retry puzzle', exact: true });
      await expect(retry).toBeEnabled();
      await retry.click();
    }
    await expectResult(page);
    await expect(metadataStatus(page)).toBeEmpty();
    await expect(page.locator('html')).toHaveAttribute('data-return-marker', 'same-document');
    await expect(winCard(page)).toHaveCount(0);
    expect((await saved(page, DAILY_KEY)).bridges).toEqual(solution);
    expect(reads).toBe(failFirst ? 2 : 1);
  });
}

test('midnight replacement of solved metadata keeps the new discovery request', async ({ page }) => {
  await visit(page, '/');
  const { puzzle, solution } = await startDaily(page);
  await solveDaily(page, puzzle, solution);
  await expectResult(page);
  await page.keyboard.press('Escape');
  await page.getByRole('link', { name: /Practice/ }).first().click();
  await expect(page).toHaveURL(/\/practice/);
  let reads = 0;
  let nextDay = 0;
  await page.route('**/api/daily_today', async (route) => {
    const response = await route.fetch();
    const info = await response.json();
    reads++;
    nextDay = info.day + 1;
    // The old day's finish was fetched just as the server clock crossed midnight.
    info.server_now = nextDay * 86_400_000 + 1000;
    if (reads > 1) {
      info.day = nextDay;
      info.attempt = null;
      info.puzzle = null;
      info.stamps = [];
      info.finishers = 0;
    }
    await route.fulfill({ response, json: info });
  });
  await page.getByRole('link', { name: /Daily/ }).first().click();
  await expect.poll(() => reads).toBe(2);
  await expect(metadataStatus(page)).toBeEmpty();
  await expect(page.getByRole('button', { name: /^Start/ })).toBeEnabled();
  await expect(submitStatus(page)).toHaveCount(0);
  await page.waitForLoadState('networkidle');
  expect(reads).toBe(2);
});
