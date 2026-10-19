// Playing a board like a player: bridges built by clicking (or tapping) one island, then the
// other, which works for every edge, also between neighbours without water in between.
import { expect, type Locator, type Page } from '@playwright/test';

export interface Island {
  pos: { x: number; y: number };
  required: number;
}

export interface Puzzle {
  width: number;
  height: number;
  /** In reading order, as the app keeps them. */
  islands: Island[];
}

/** What the app saves in local storage, see `SavedGame` in src/state/game.rs. */
export interface SavedGame {
  puzzle: Puzzle;
  bridges: number[];
  solution: number[] | null;
  status: 'Empty' | 'Playing' | 'Won' | 'Revealed';
  moves: number;
  hints: number;
}

export const PRACTICE_KEY = 'hashi.game.v2';
export const DAILY_KEY = 'hashi.daily.v1';

/** The edges of a puzzle in the order of `Board::new` (common/src/board.rs): the island pairs. */
export function edges(puzzle: Puzzle): [number, number][] {
  const at = (x: number, y: number) => puzzle.islands.findIndex((i) => i.pos.x === x && i.pos.y === y);
  const list: [number, number][] = [];
  puzzle.islands.forEach((island, a) => {
    for (let x = island.pos.x + 1; x < puzzle.width; x++) {
      const b = at(x, island.pos.y);
      if (b >= 0) {
        list.push([a, b]);
        break;
      }
    }
    for (let y = island.pos.y + 1; y < puzzle.height; y++) {
      const b = at(island.pos.x, y);
      if (b >= 0) {
        list.push([a, b]);
        break;
      }
    }
  });
  return list;
}

/** The first board of the page, the practice legend below it draws small ones. */
export const board = (page: Page): Locator => page.locator('svg.board-svg').first();

export async function saved(page: Page, key: string): Promise<SavedGame> {
  const raw = await page.evaluate((k) => localStorage.getItem(k), key);
  expect(raw, `a game saved as ${key}`).not.toBeNull();
  return JSON.parse(raw!) as SavedGame;
}

/**
 * A point of the board (in its units, 16 per cell) in the viewport coordinates the mouse and
 * touchscreen take, scrolled to the middle of the screen first: near the edges it can be under
 * the sticky header or the phones' tab bar.
 */
export async function boardPoint(page: Page, bx: number, by: number) {
  return board(page).evaluate(
    (svg, [x, y]) => {
      const at = () => new DOMPoint(x, y).matrixTransform((svg as SVGSVGElement).getScreenCTM()!);
      const before = at();
      if (before.y < 120 || before.y > innerHeight - 120) window.scrollBy(0, before.y - innerHeight / 2);
      const p = at();
      return { x: p.x, y: p.y };
    },
    [bx, by],
  );
}

/** Where a point of the viewport is on the board, in board units. */
export async function toBoard(page: Page, point: { x: number; y: number }) {
  return board(page).evaluate(
    (svg, { x, y }) => {
      const p = new DOMPoint(x, y).matrixTransform((svg as SVGSVGElement).getScreenCTM()!.inverse());
      return { x: p.x, y: p.y };
    },
    point,
  );
}

/** The middle of a cell, see `boardPoint`. */
export const cellCenter = (page: Page, x: number, y: number) => boardPoint(page, x * 16 + 8, y * 16 + 8);

export async function press(page: Page, point: { x: number; y: number }, touch: boolean) {
  if (touch) await page.touchscreen.tap(point.x, point.y);
  else await page.mouse.click(point.x, point.y);
}

/**
 * Adds one bridge between two islands: a click on each. Waits until the game saved it, so the
 * next one starts from it.
 */
export async function addBridge(page: Page, key: string, puzzle: Puzzle, edge: number, touch = false) {
  const [a, b] = edges(puzzle)[edge];
  const before = (await saved(page, key)).bridges[edge];
  for (const island of [a, b]) {
    const { x, y } = puzzle.islands[island].pos;
    await press(page, await cellCenter(page, x, y), touch);
  }
  await expect
    .poll(async () => (await saved(page, key)).bridges[edge], { message: `bridge ${a}-${b} built` })
    .toBe((before + 1) % 3);
}

/** Builds the missing bridges of `solution`, edge by edge. Expects no wrong bridges on the board. */
export async function build(page: Page, key: string, puzzle: Puzzle, solution: number[], touch = false) {
  const current = (await saved(page, key)).bridges;
  for (let e = 0; e < solution.length; e++) {
    for (let n = current[e]; n < solution[e]; n++) await addBridge(page, key, puzzle, e, touch);
  }
}

/** Edges where the board has fewer bridges than the solution. */
export function missing(bridges: number[], solution: number[]) {
  return solution.flatMap((s, e) => (bridges[e] < s ? [e] : []));
}

/**
 * Puts a game into local storage before the page loads, once: reloads keep what the page saved
 * since. The page saves its game every second while the clock runs, so writing it from the test
 * while the page is open would race with that.
 */
export async function seedGame(page: Page, key: string, game: SavedGame & { puzzle: Puzzle }) {
  const saved = {
    unique: null,
    source: 'Custom',
    elapsed: 0,
    ...game,
  };
  await page.addInitScript(
    ([k, value]) => {
      if (sessionStorage.getItem(`seeded:${k}`)) return;
      localStorage.setItem(k, value);
      sessionStorage.setItem(`seeded:${k}`, '1');
    },
    [key, JSON.stringify(saved)],
  );
}
