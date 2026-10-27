// What each store screenshot shows: a page of the site as one of the seeded players, prepared as
// a player would leave it. The order is the stores' order: the first ones sell the game.
import { DatabaseSync } from 'node:sqlite';

import { expect, type BrowserContext, type Locator, type Page } from '@playwright/test';

import { addBridge, DAILY_KEY, edges, PRACTICE_KEY } from '../../e2e/board';
import { DB_PATH, STORE_URL } from './env';
import { ME, PLAYING, PUZZLES, sessionOf } from './seed';

export type SceneId = 'daily' | 'ranking' | 'practice' | 'friends' | 'stamps' | 'night';

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
    // Today's daily, a good part of the way: the clock running, the moves counted
    id: 'daily',
    user: PLAYING,
    path: '/',
    before: async () => restartClock(),
    play: async (page) => {
      const { puzzle, solution } = PUZZLES.daily;
      await expect(page.locator('svg.board-svg').first()).toHaveClass(/board-interactive/);
      // Every bridge of the first islands in reading order, as a player works through the board
      const built = new Set<number>();
      const target = Math.round(solution.filter((b) => b > 0).length * 0.55);
      const order = edges(puzzle)
        .map((pair, e) => ({ e, pair }))
        .filter(({ e }) => solution[e] > 0)
        .sort((a, b) => a.pair[0] - b.pair[0] || a.pair[1] - b.pair[1]);
      for (const { e } of order) {
        if (built.size >= target) break;
        for (let n = 0; n < solution[e]; n++) await addBridge(page, DAILY_KEY, puzzle, e, true);
        built.add(e);
      }
      await page.evaluate(() => window.scrollTo(0, 0));
    },
  },
  {
    // The leaderboard of the day with mika on the podium, the monthly race below
    id: 'ranking',
    user: ME,
    path: '/ranks',
  },
  {
    // A hard practice board half built
    id: 'practice',
    user: ME,
    path: '/practice',
    before: async (context) => {
      const { puzzle, solution, source } = PUZZLES.practice;
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
      await context.addInitScript(
        ([key, value]) => {
          if (sessionStorage.getItem('store-seeded')) return;
          localStorage.setItem(key, value);
          sessionStorage.setItem('store-seeded', '1');
        },
        [PRACTICE_KEY, JSON.stringify(game)] as const,
      );
    },
  },
  {
    // The group: everyone's bridges between them, today's times below
    id: 'friends',
    user: ME,
    path: '/groups/1',
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

/** The second mika started today's daily a minute and a half ago, whenever the screenshot is. */
function restartClock() {
  const db = new DatabaseSync(DB_PATH, { timeout: 5_000 });
  try {
    db.prepare(
      `UPDATE attempts SET started_at = ? WHERE day = ? AND user_id = (SELECT id FROM users WHERE username = ?)`,
    ).run(Date.now() - 97_000, Math.floor(Date.now() / 86_400_000), PLAYING);
  } finally {
    db.close();
  }
}

export async function signIn(context: BrowserContext, user: string) {
  await context.addCookies([
    { name: 'hashi_session', value: sessionOf(user), url: STORE_URL, httpOnly: true, sameSite: 'Lax' },
  ]);
}

/**
 * The practice board with every bridge but one, for the feature graphic: a big board full of
 * wooden and vermilion bridges (the last one missing, or the page would celebrate the win).
 */
export async function boardShot(context: BrowserContext, out: string) {
  const { puzzle, solution, source } = PUZZLES.practice;
  const last = solution.findLastIndex((b) => b > 0);
  const bridges = solution.map((b, e) => (e === last ? 0 : b));
  const game = { puzzle, bridges, solution, unique: true, source, status: 'Playing', elapsed: 0, moves: 0, hints: 0 };
  await context.addInitScript(
    ([key, value]) => {
      if (sessionStorage.getItem('store-seeded')) return;
      localStorage.setItem(key, value);
      sessionStorage.setItem('store-seeded', '1');
    },
    [PRACTICE_KEY, JSON.stringify(game)] as const,
  );
  const page = await context.newPage();
  await page.goto(`${STORE_URL}/practice`);
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  await page.locator('svg.board-svg').first().screenshot({ path: out, animations: 'disabled' });
}
