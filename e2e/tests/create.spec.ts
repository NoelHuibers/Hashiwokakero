// The workshop: a puzzle drawn island by island, proven unique, exported, imported again and
// played.
import { text } from 'node:stream/consumers';

import { board, build, cellCenter, PRACTICE_KEY, saved } from '../board';
import { expect, test, visit } from '../fixtures';

/** A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it. */
const ISLANDS = [
  { x: 0, y: 0, n: 2 },
  { x: 2, y: 0, n: 3 },
  { x: 2, y: 2, n: 1 },
];

test('draw a puzzle, check it, export it, import it and play it', async ({ page }) => {
  await visit(page, '/create');
  await expect(page.getByText('8×8 cells, 0 islands')).toBeVisible();

  // Hovering a cell and typing its number
  for (const [i, { x, y, n }] of ISLANDS.entries()) {
    const { x: px, y: py } = await cellCenter(page, x, y);
    await page.mouse.move(px, py);
    await page.keyboard.press(String(n));
    await expect(page.getByText(`8×8 cells, ${i + 1} islands`)).toBeVisible();
    // A lone 2 can't get its bridges, the alert says so until its neighbours are there
    if (i === 0) await expect(page.getByText('Heads up')).toBeVisible();
  }
  await expect(page.getByText('Heads up')).toHaveCount(0);
  await expect(board(page).locator('g.island text')).toHaveText(['2', '3', '1']);

  // The solver proves it unique and shows its bridges
  await page.getByRole('button', { name: 'Unique?' }).click();
  await expect(page.getByText('Unique: a fine puzzle!')).toBeVisible();
  await expect(board(page).locator('g.bridge')).toHaveCount(2);

  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export .txt' }).click();
  // Read as a stream, not from path(): with the browser in a container (pnpm run e2e:container)
  // the file is on the container's side
  const txt = await text(await (await download).createReadStream());
  // Rows and columns, then the grid
  expect(txt).toBe(`8 8\n2.3.....\n........\n..1.....\n${'........\n'.repeat(5)}`);

  await page.getByRole('button', { name: 'Clear' }).click();
  await expect(page.getByText('8×8 cells, 0 islands')).toBeVisible();

  // The exported file brings the same puzzle back
  await page.locator('input[type=file]').setInputFiles({ name: 'mine.txt', mimeType: 'text/plain', buffer: Buffer.from(txt) });
  await expect(page.getByText('Imported mine.txt')).toBeVisible();
  await expect(page.getByText('8×8 cells, 3 islands')).toBeVisible();
  await expect(board(page).locator('g.island text')).toHaveText(['2', '3', '1']);

  await page.getByRole('button', { name: /^Play this puzzle/ }).click();
  await expect(page).toHaveURL(/\/practice$/);
  await expect(page.getByText('Your own puzzle', { exact: true })).toBeVisible();
  const game = await saved(page, PRACTICE_KEY);
  expect(game.puzzle.islands).toEqual(ISLANDS.map(({ x, y, n }) => ({ pos: { x, y }, required: n })));
  await build(page, PRACTICE_KEY, game.puzzle, [2, 1]);
  await expect(page.getByRole('heading', { name: /Well done!/ })).toBeVisible();
});
