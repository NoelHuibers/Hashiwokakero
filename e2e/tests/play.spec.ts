// Play: a generated puzzle (a server function), bridges built on it (the board's pointer
// handling in the WASM), and a whole game up to the win screen, saved in the browser.
import type { Page } from '@playwright/test';

import {
  addBridge,
  board,
  boardPoint,
  build,
  PLAY_KEY,
  type Puzzle,
  saved,
  seedGame,
  toBoard,
} from '../board.ts';
import { expect, test, visit } from '../fixtures.ts';
import { LADDER, LADDER_SOLUTION, ladderRow } from '../puzzles.ts';

const bridges = (page: Page) => board(page).locator('g.bridge:not(.ghost)');

/** Where the islands are, to tell one puzzle from another. */
const layout = (page: Page) =>
  board(page)
    .locator('g.island')
    .evaluateAll((islands) =>
      islands.map((g) => `${g.getAttribute('transform')}:${g.textContent}`).join(' '),
    );

/**
 * The middle of the water between an island and its right neighbour, in the viewport
 * coordinates the mouse and touchscreen take. Computed from the islands' cells: their boxes
 * also hold the trees and gates on them, and were off centre.
 */
async function gap(page: Page) {
  // In the middle of the screen: "if needed" left its top row under the sticky header once
  // Generate, then further down on phones, was clicked
  await board(page).evaluate((el) => el.scrollIntoView({ block: 'center' }));
  const point = await board(page)
    .locator('g.island')
    .evaluateAll((islands) => {
      const svg = (islands[0] as SVGGElement).ownerSVGElement!;
      const toPage = svg.getScreenCTM()!;
      // translate(x,y) in board units, 16 per cell
      const cells = islands.map((g) => {
        const [x, y] = g
          .getAttribute('transform')!
          .match(/-?[\d.]+/g)!
          .map(Number);
        return { x: x!, y: y! };
      });
      for (const { x, y } of cells) {
        const right = cells.filter((c) => c.y === y && c.x > x).sort((p, q) => p.x - q.x)[0];
        // At least one empty cell between them, so the point is on water and not an island
        if (right && right.x - x >= 32) {
          const p = new DOMPoint((x + right.x) / 2 + 8, y + 8).matrixTransform(toPage);
          return { x: p.x, y: p.y };
        }
      }
      return null;
    });
  expect(point, 'two islands in a row with water between them').not.toBeNull();
  return point!;
}

/**
 * A new puzzle: of a difficulty, with its chip above the board, else with the button next to the
 * clock (Medium unless another was picked).
 */
async function newPuzzle(page: Page, difficulty?: string) {
  await visit(page, '/play');
  // The first visit generates a puzzle by itself
  await expect(board(page).locator('g.island').first()).toBeVisible();
  const first = await layout(page);
  if (difficulty) {
    await page
      .getByRole('group', { name: 'New puzzle' })
      .getByRole('button', { name: difficulty, exact: true })
      .click();
  } else {
    await page.getByRole('button', { name: 'New puzzle', exact: true }).click();
  }
  await expect.poll(() => layout(page), { message: 'a new puzzle' }).not.toBe(first);
  await expect(page.getByText('Growing islands')).toBeHidden();
  await expect(bridges(page)).toHaveCount(0);
}

const wonHeading = (page: Page) => page.getByRole('heading', { name: /Well done!/ });

test('generate a puzzle and build a bridge with the mouse', async ({ page }) => {
  await newPuzzle(page);
  const { x, y } = await gap(page);
  await page.mouse.click(x, y);
  await expect(bridges(page)).toHaveCount(1);
});

test('play a puzzle from the first bridge to the win screen', async ({ page }) => {
  await newPuzzle(page, 'Easy');
  await expect(page.getByText(/^Easy 7×7$/)).toBeVisible();
  const game = await saved(page, PLAY_KEY);
  expect(game.puzzle.width).toBe(7);
  // The generator sends the solution along, the hints and checks use it
  const solution = game.solution!;
  expect(solution).toHaveLength(game.bridges.length);

  // A click on a pair of islands cycles their bridges: one, two, none
  const edge = 0;
  await addBridge(page, PLAY_KEY, game.puzzle, edge);
  await expect(bridges(page)).toHaveCount(1);
  await addBridge(page, PLAY_KEY, game.puzzle, edge);
  await expect(bridges(page)).toHaveCount(1);
  await addBridge(page, PLAY_KEY, game.puzzle, edge);
  await expect(bridges(page)).toHaveCount(0);
  expect((await saved(page, PLAY_KEY)).moves).toBe(3);

  // Undo goes back through them, redo forward again
  const undo = page.getByRole('button', { name: /Undo/ });
  const redo = page.getByRole('button', { name: /Redo/ });
  await expect(redo).toBeDisabled();
  await undo.click();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).bridges[edge]).toBe(2);
  await undo.click();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).bridges[edge]).toBe(1);
  await redo.click();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).bridges[edge]).toBe(2);
  await redo.click();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).bridges[edge]).toBe(0);
  await expect(redo).toBeDisabled();

  // Restart clears the board, it is behind the "more" button
  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('button', { name: 'Restart' }).click();
  await expect(page.getByRole('button', { name: 'Restart' })).toBeHidden();
  await expect(bridges(page)).toHaveCount(0);
  expect((await saved(page, PLAY_KEY)).bridges.every((b) => b === 0)).toBe(true);

  // Half of the solution, then a reload: the game is still there
  const half = solution.map((s, e) => (e < solution.length / 2 ? s : 0));
  await build(page, PLAY_KEY, game.puzzle, half);
  const before = await saved(page, PLAY_KEY);
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect.poll(() => layout(page)).toBe(layoutOf(game.puzzle));
  await expect(bridges(page)).toHaveCount(half.filter((b) => b > 0).length);
  expect((await saved(page, PLAY_KEY)).bridges).toEqual(before.bridges);

  // The rest, and every island is done
  await build(page, PLAY_KEY, game.puzzle, solution);
  await expect(wonHeading(page)).toBeVisible();
  await expect(board(page).locator('g.island:not(.done)')).toHaveCount(0);
  await expect(
    page.getByText(`${game.puzzle.islands.length} / ${game.puzzle.islands.length}`),
  ).toBeVisible();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).status).toBe('Won');

  // A won game stays won
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
  await expect(wonHeading(page)).toBeVisible();
  await page.getByRole('button', { name: 'Next puzzle' }).click();
  await expect(wonHeading(page)).toBeHidden();
  await expect.poll(async () => (await saved(page, PLAY_KEY)).status).toBe('Playing');
});

/** `layout` of a puzzle, without the page. */
function layoutOf(puzzle: Puzzle) {
  return puzzle.islands
    .map((i) => `translate(${i.pos.x * 16},${i.pos.y * 16}):${i.required}`)
    .join(' ');
}

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('generate a puzzle and build a bridge with a tap', async ({ page }) => {
    await newPuzzle(page);
    const { x, y } = await gap(page);
    await page.touchscreen.tap(x, y);
    await expect(bridges(page)).toHaveCount(1);
  });

  test('solve a puzzle by tapping', async ({ page }) => {
    await newPuzzle(page, 'Easy');
    const game = await saved(page, PLAY_KEY);
    await build(page, PLAY_KEY, game.puzzle, game.solution!, true);
    await expect(wonHeading(page)).toBeVisible();
    await expect(board(page).locator('g.island:not(.done)')).toHaveCount(0);
  });
});

/** The ladder, solved: the win screen shows as soon as the page loads. */
const wonLadder = {
  puzzle: LADDER,
  bridges: LADDER_SOLUTION,
  solution: LADDER_SOLUTION,
  status: 'Won',
  moves: 9,
  hints: 0,
} as const;

test.describe('on a phone, the last tap', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  // The browser sends a click after a tap, and the tap that solved the puzzle brought up the win
  // screen under the finger: its buttons got the click
  test("doesn't press the win screen's buttons", async ({ page, context }) => {
    // Where the win screen's first button is, on the board
    await seedGame(page, PLAY_KEY, wonLadder);
    await visit(page, '/play');
    const next = page.getByRole('button', { name: /^Next puzzle/ });
    const box = (await next.boundingBox())!;
    const target = await toBoard(page, { x: box.x + box.width / 2, y: box.y + box.height / 2 });
    const row = Math.floor(target.y / 16);
    const column = Math.floor(target.x / 16);
    expect(row, 'the button lies over a row of the ladder').toBeGreaterThanOrEqual(1);
    expect(row).toBeLessThanOrEqual(5);
    expect(column, 'and over its water').toBeGreaterThanOrEqual(1);
    expect(column).toBeLessThanOrEqual(5);

    // The same ladder without that row's bridge, built there with a tap
    const playing = await context.newPage();
    const bridges = LADDER_SOLUTION.map((b, e) => (e === ladderRow(row) ? 0 : b));
    await seedGame(playing, PLAY_KEY, { ...wonLadder, bridges, status: 'Playing', moves: 8 });
    await visit(playing, '/play');
    await playing.evaluate(() => {
      const w = window as unknown as { clicked: string[] };
      w.clicked = [];
      document.addEventListener(
        'click',
        (e) => w.clicked.push((e.target as Element).closest('button')?.textContent ?? ''),
        true,
      );
    });
    const point = await boardPoint(playing, target.x, target.y);
    await playing.touchscreen.tap(point.x, point.y);
    await expect(wonHeading(playing)).toBeVisible();
    // Taps are handled in order: once this one is through, a click of the first would be too
    await playing.touchscreen.tap(10, 300);
    expect(
      await playing.evaluate(() =>
        (window as unknown as { clicked: string[] }).clicked.filter(Boolean),
      ),
    ).toEqual([]);
    await expect(wonHeading(playing)).toBeVisible();
    expect((await saved(playing, PLAY_KEY)).status).toBe('Won');
    await playing.close();
  });
});

test.describe('copying the result', () => {
  test('without the permission says so', async ({ page }) => {
    await seedGame(page, PLAY_KEY, wonLadder);
    await visit(page, '/play');
    await page.getByRole('button', { name: 'Copy' }).click();
    await expect(page.getByText("Couldn't copy to the clipboard.")).toBeVisible();
    await expect(page.getByText('Result copied to the clipboard.')).toHaveCount(0);
  });

  test('with the permission copies it', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await seedGame(page, PLAY_KEY, wonLadder);
    await visit(page, '/play');
    await page.getByRole('button', { name: 'Copy' }).click();
    await expect(page.getByText('Result copied to the clipboard.')).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(
      /^🌉 Hashi · Your own puzzle ·/,
    );
  });
});

// Play was called Practice: its old address, in every language, stays a link to it
test.describe("Play's old address", () => {
  test('redirects for good on the server, with the query', async ({ page, request }) => {
    for (const [from, to] of [
      ['/practice', '/play'],
      ['/practice?size=9', '/play?size=9'],
      ['/de/practice', '/de/play'],
      ['/zh-Hant/practice/?size=9', '/zh-Hant/play?size=9'],
    ] as const) {
      const res = await request.get(from, { maxRedirects: 0 });
      expect(res.status(), from).toBe(301);
      expect(res.headers().location, from).toBe(to);
    }
    await visit(page, '/ja/practice?size=9');
    await expect(page).toHaveURL(/\/ja\/play\?size=9$/);
    await expect(board(page).locator('g.island').first()).toBeVisible();
  });

  test('leads to Play in the router of a page, from a link of an older build', async ({ page }) => {
    await visit(page, '/rules');
    await page.evaluate(() => {
      const link = Object.assign(document.createElement('a'), {
        href: '/practice?size=9',
        id: 'old-link',
        textContent: 'Practice',
      });
      document.querySelector('main')!.append(link);
      Object.assign(window, { sameDocument: true });
    });
    await page.locator('#old-link').click();
    await expect(page).toHaveURL(/\/play\?size=9$/);
    await expect(board(page).locator('g.island').first()).toBeVisible();
    // The router's, not a page load
    expect(
      await page.evaluate(
        () => (window as unknown as { sameDocument?: boolean }).sameDocument === true,
      ),
    ).toBe(true);
    // In place of the old address: back is the page before it
    await page.goBack();
    await expect(page).toHaveURL(/\/rules$/);
  });
});
