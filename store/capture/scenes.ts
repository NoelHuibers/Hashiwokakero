// What each store screenshot shows: a page of the site as one of the seeded players, prepared as
// a player would leave it. The order is the stores' order: the first ones sell the game.
import { DatabaseSync } from 'node:sqlite';

import { type BrowserContext, expect, type Locator, type Page } from '@playwright/test';

import { DAILY_KEY, edges, PLAY_KEY, type Puzzle, SHARED_KEY, saved } from '../../e2e/board.ts';
import { DB_PATH, STORE_URL } from './env.ts';
import { ME, PLAYING, PUZZLES, SHARED, sessionOf } from './seed.ts';

export type SceneId = 'daily' | 'ranking' | 'play' | 'friends' | 'shared' | 'stamps' | 'night';

export interface Scene {
  id: SceneId;
  /** The seeded player (username) whose session the browser has. */
  user: string;
  path: string;
  dark?: boolean;
  /** Before the page opens: local storage, the database. */
  before?: (context: BrowserContext) => Promise<void>;
  /** On the open page: what the player did. */
  play?: (page: Page) => Promise<void>;
  /**
   * What is scrolled to the top of the screen, below the header, on a screen; the page's top
   * without. Found by kanji, which are the same in every language.
   */
  scrollTo?: { screens: ('phone' | 'tablet')[]; target: (page: Page) => Locator; above?: number };
}

export const SCENES: Scene[] = [
  {
    // Today's daily, a good part of the way: the clock running, the moves counted. The board as
    // the browser saved it (the daily restores it on load): built tap by tap it took minutes on
    // the weekend's big boards (an Oni has ~230 bridges), which made the run time depend on the
    // weekday.
    id: 'daily',
    user: PLAYING,
    path: '/',
    before: async (context) => {
      restartClock();
      const { puzzle, solution } = PUZZLES.daily;
      const bridges = dailyProgress(puzzle, solution);
      const game = {
        puzzle,
        bridges,
        solution: null,
        unique: true,
        source: 'Custom',
        status: 'Playing',
        elapsed: 0,
        moves: bridges.reduce((a, b) => a + b, 0),
        hints: 0,
      };
      await seedGame(context, DAILY_KEY, game);
    },
    play: async (page) => {
      await expect(page.locator('svg.board-svg').first()).toHaveClass(/board-interactive/);
      // The page took the seeded board, it didn't start an empty one
      const { puzzle, solution } = PUZZLES.daily;
      expect((await saved(page, DAILY_KEY)).bridges).toEqual(dailyProgress(puzzle, solution));
    },
  },
  {
    // The leaderboard of the day with mika on the podium, the monthly race below
    id: 'ranking',
    user: ME,
    path: '/ranks',
  },
  {
    // A hard board of Play half built
    id: 'play',
    user: ME,
    path: '/play',
    before: async (context) => {
      const { puzzle, solution, source } = PUZZLES.play;
      const bridges = solution.map((b, e) => (e % 5 < 3 ? b : 0));
      const game = {
        puzzle,
        bridges,
        solution,
        unique: true,
        source,
        status: 'Playing',
        elapsed: 263,
        moves: bridges.reduce((a, b) => a + b, 0) + 3,
        hints: 0,
      };
      await seedGame(context, PLAY_KEY, game);
    },
  },
  {
    // The group: everyone's bridges between them, today's times below
    id: 'friends',
    user: ME,
    path: '/groups/1',
  },
  {
    // A puzzle aiko shared, mika on the way to beat her time; on the tablet everyone who solved
    // it beside the board
    id: 'shared',
    user: ME,
    path: `/p/${SHARED.code}`,
    before: async (context) => {
      const { puzzle, solution } = SHARED;
      const bridges = solution.map((b, e) => (e % 4 < 3 ? b : 0));
      const game = {
        puzzle,
        bridges,
        solution: null,
        unique: true,
        source: 'Custom',
        status: 'Playing',
        elapsed: 81,
        moves: bridges.reduce((a, b) => a + b, 0) + 2,
        hints: 0,
      };
      await seedGame(context, SHARED_KEY, game);
    },
    play: async (page) => {
      await expect(page.getByTestId('shared-to-beat')).toBeVisible();
      await expect(page.locator('svg.board-svg').first()).toHaveClass(/board-interactive/);
    },
  },
  {
    // The stamp book on the account page
    id: 'stamps',
    user: ME,
    path: '/account',
    scrollTo: {
      screens: ['phone', 'tablet'],
      target: (page) => page.locator('span', { hasText: /^判子帳$/ }).first(),
      above: 40,
    },
  },
  {
    // Night: today's daily solved, in the dark theme
    id: 'night',
    user: ME,
    path: '/',
    dark: true,
    // On the phone the result below the board: the time, the place, the rivals beaten
    scrollTo: {
      screens: ['phone'],
      target: (page) => page.getByRole('heading', { name: /見事/ }),
      above: 150,
    },
  },
];

/**
 * The daily as a player works through it: every bridge of the first islands in reading order,
 * until a good half of the solution's edges have theirs.
 */
function dailyProgress(puzzle: Puzzle, solution: number[]): number[] {
  const target = Math.round(solution.filter((b) => b > 0).length * 0.55);
  const order = edges(puzzle)
    .flatMap((pair, e) => ((solution[e] ?? 0) > 0 ? [{ e, pair }] : []))
    .sort((a, b) => a.pair[0] - b.pair[0] || a.pair[1] - b.pair[1])
    .slice(0, target);
  const bridges = solution.map(() => 0);
  for (const { e } of order) bridges[e] = solution[e] ?? 0;
  return bridges;
}

/** A game in the browser's storage before the page loads, once: the page saves over it later. */
async function seedGame(context: BrowserContext, key: string, game: object) {
  await context.addInitScript(
    ([key, value]) => {
      if (sessionStorage.getItem('store-seeded')) return;
      localStorage.setItem(key, value);
      sessionStorage.setItem('store-seeded', '1');
    },
    [key, JSON.stringify(game)] as const,
  );
}

/** The second mika started today's daily a minute and a half ago, whenever the screenshot is. */
function restartClock() {
  const db = new DatabaseSync(DB_PATH, { timeout: 5_000 });
  try {
    db.prepare(
      'UPDATE attempts SET started_at = ? WHERE day = ? AND user_id = (SELECT id FROM users WHERE username = ?)',
    ).run(Date.now() - 97_000, Math.floor(Date.now() / 86_400_000), PLAYING);
  } finally {
    db.close();
  }
}

export async function signIn(context: BrowserContext, user: string) {
  await context.addCookies([
    {
      name: 'hashi_session',
      value: sessionOf(user),
      url: STORE_URL,
      httpOnly: true,
      sameSite: 'Lax',
    },
  ]);
}

/**
 * The board of Play with every bridge but one, for the feature graphic: a big board full of
 * wooden and vermilion bridges (the last one missing, or the page would celebrate the win).
 */
export async function boardShot(context: BrowserContext, out: string) {
  const { puzzle, solution, source } = PUZZLES.play;
  const last = solution.findLastIndex((b) => b > 0);
  const bridges = solution.map((b, e) => (e === last ? 0 : b));
  const game = {
    puzzle,
    bridges,
    solution,
    unique: true,
    source,
    status: 'Playing',
    elapsed: 0,
    moves: 0,
    hints: 0,
  };
  await seedGame(context, PLAY_KEY, game);
  const page = await context.newPage();
  await page.goto(`${STORE_URL}/play`);
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  // biome-ignore lint/suspicious/noPlaywrightNetworkidle: every image loaded for the store screenshot
  await page.waitForLoadState('networkidle');
  // biome-ignore lint/suspicious/noPlaywrightWaitForTimeout: lets the last transitions end for the store screenshot
  await page.waitForTimeout(500);
  await page.locator('svg.board-svg').first().screenshot({ path: out, animations: 'disabled' });
}
