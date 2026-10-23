// The app without a network: practice runs on the phone, through the app's Tauri commands
// (src/api/practice.rs, app/src-tauri), and the pages that need the server say so calmly. A fake
// `window.__TAURI_INTERNALS__` stands in for Tauri: it records the commands and answers them
// with fixed puzzles. Every server function call fails, as without a network.
import type { Page } from '@playwright/test';

import { board, DAILY_KEY, edges, seedGame } from '../board';
import { LADDER, LADDER_SOLUTION } from '../puzzles';
import { expect, test, visit } from '../fixtures';

interface Call {
  cmd: string;
  args: string;
}

/** Tauri as the page sees it in the app: `invoke` takes a command and its `{ args }`. */
async function fakeTauri(page: Page) {
  await page.addInitScript(
    ([puzzle, solution]) => {
      const calls: { cmd: string; args: string }[] = [];
      const answers: Record<string, (args: any) => unknown> = {
        // Without the solution, so hints and checks ask the solver
        generate_puzzle: ({ params }) => ({ puzzle, solution: [], params, unique: true, attempts: 0, generate_ms: 1 }),
        solve_puzzle: ({ config, unique }) => ({
          outcome: { Solved: { bridges: solution, unique: unique ? true : null } },
          stats: {
            config,
            variables: 1,
            clauses: 1,
            encode_ms: 0,
            solve_ms: 0,
            decisions: null,
            restarts: null,
            sat_calls: 1,
            connectivity_cuts: 0,
          },
        }),
        puzzle_cnf: () => 'p cnf 1 1\n1 0\n',
      };
      Object.assign(window, {
        __TAURI_INTERNALS__: {
          invoke(cmd: string, payload: { args: string }) {
            // The plugins' commands (the system bars' icons, src/api/native.rs) need no answer
            if (cmd.startsWith('plugin:')) return Promise.resolve(null);
            calls.push({ cmd, args: payload.args });
            const answer = answers[cmd];
            if (!answer) return Promise.reject(`unknown command ${cmd}`);
            // The commands take and give the server functions' JSON as text
            return Promise.resolve(JSON.stringify(answer(JSON.parse(payload.args))));
          },
        },
        tauriCalls: calls,
      });
    },
    [LADDER, LADDER_SOLUTION] as const,
  );
}

const calls = (page: Page) => page.evaluate(() => (window as unknown as { tauriCalls: Call[] }).tauriCalls);

/** No network: every server function call fails, and the requests are kept. */
async function offline(page: Page) {
  const api: string[] = [];
  await page.route(/\/api\//, (route) => {
    api.push(new URL(route.request().url()).pathname);
    return route.abort('internetdisconnected');
  });
  return api;
}

/** The toasts on the screen, by their kind's mark (src/components/layout/toaster.rs). */
const toasts = (page: Page) => page.locator('[role=log] > div');
const errorToasts = (page: Page) => toasts(page).filter({ hasText: '✕' });

/** The browser's notes about the failed requests are expected here, nothing else. */
function withoutNetworkErrors(problems: string[]) {
  const rest = problems.filter((p) => !/ERR_INTERNET_DISCONNECTED|Failed to load resource/.test(p));
  problems.splice(0, problems.length, ...rest);
}

test('practice generates, hints and checks on the phone', async ({ page, problems }) => {
  await fakeTauri(page);
  const api = await offline(page);
  await visit(page, '/practice');
  await expect(board(page).locator('g.island')).toHaveCount(LADDER.islands.length);

  // The seed reaches the phone whole: the JSON is sent as text, not as a JavaScript number
  const [generate] = await calls(page);
  expect(generate.cmd).toBe('generate_puzzle');
  expect(generate.args).toMatch(/^\{"params":\{"width":\d+,"height":\d+,"difficulty":"\w+","seed":\d+\}\}$/);

  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  await expect(board(page).locator('g.bridge:not(.ghost)')).toHaveCount(1);
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await expect(toasts(page).filter({ hasText: 'So far so good' })).toBeVisible();
  const solve = (await calls(page)).filter((c) => c.cmd === 'solve_puzzle');
  expect(solve).toHaveLength(1);
  expect(JSON.parse(solve[0].args)).toMatchObject({ puzzle: LADDER, unique: false });

  expect(api.filter((p) => /generate_puzzle|solve_puzzle|puzzle_cnf/.test(p))).toEqual([]);
  await expect(errorToasts(page)).toHaveCount(0);
  withoutNetworkErrors(problems);
});

/** Today's daily as the daily page saves it, in the state given. */
async function seedDaily(page: Page, status: 'Playing' | 'Won') {
  const today = new Date().toISOString().slice(0, 10);
  const params = { width: 7, height: 7, difficulty: 'Medium', seed: 1 };
  await seedGame(page, DAILY_KEY, {
    puzzle: LADDER,
    bridges: status === 'Won' ? LADDER_SOLUTION : edges(LADDER).map(() => 0),
    solution: null,
    status,
    moves: 0,
    hints: 0,
    source: { Daily: { date: today, params } },
  } as never);
}

/** The daily's puzzle in the workshop, proven unique there. */
async function proveInWorkshop(page: Page) {
  await page.addInitScript((puzzle) => localStorage.setItem('hashi.editor.v2', JSON.stringify(puzzle)), LADDER);
  await visit(page, '/create');
  await page.getByRole('button', { name: 'Unique?' }).click();
}

test("the phone's solvers don't touch today's unfinished daily", async ({ page, problems }) => {
  await fakeTauri(page);
  await offline(page);
  await seedDaily(page, 'Playing');
  await proveInWorkshop(page);
  await expect(errorToasts(page).filter({ hasText: "That's today's daily" })).toBeVisible();
  expect((await calls(page)).filter((c) => c.cmd === 'solve_puzzle')).toEqual([]);
  withoutNetworkErrors(problems);
});

test('the workshop proves a finished daily unique on the phone', async ({ page, problems }) => {
  await fakeTauri(page);
  await offline(page);
  await seedDaily(page, 'Won');
  await proveInWorkshop(page);
  await expect(page.getByText('Unique: a fine puzzle!')).toBeVisible();
  const solve = (await calls(page)).filter((c) => c.cmd === 'solve_puzzle');
  expect(solve.map((c) => JSON.parse(c.args).unique)).toEqual([true]);
  await expect(errorToasts(page)).toHaveCount(0);
  withoutNetworkErrors(problems);
});

test('the daily shows that it needs the network, without errors', async ({ page, problems }) => {
  await fakeTauri(page);
  await offline(page);
  await visit(page, '/');
  await expect(page.getByText("Couldn't load today's puzzle. Try again.")).toBeVisible();
  await expect(errorToasts(page)).toHaveCount(0);
  withoutNetworkErrors(problems);
});

test('pages that need the server say once that the app is offline', async ({ page, problems }) => {
  await fakeTauri(page);
  await offline(page);
  // The ranks make several calls at once, all of them fail
  await visit(page, '/ranks');
  const notice = toasts(page).filter({ hasText: "You're offline. Practice still works." });
  await expect(notice).toHaveCount(1);
  await expect(errorToasts(page)).toHaveCount(0);
  // Not again on the next page while offline, once it went away
  await expect(notice).toHaveCount(0);
  await page.getByRole('link', { name: /Groups/ }).first().click();
  await expect(page).toHaveURL(/\/groups$/);
  await page.waitForLoadState('networkidle');
  await expect(toasts(page)).toHaveCount(0);
  withoutNetworkErrors(problems);
});
