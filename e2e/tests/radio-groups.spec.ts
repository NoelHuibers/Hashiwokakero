// The radio groups, the solver choice and the workshop's brushes, used with the keyboard: one
// Tab stop, the checked radio, and the arrow keys move the focus and the choice along.
import type { Locator, Page } from '@playwright/test';

import { board, cellCenter, edges, PRACTICE_KEY, seedGame } from '../board';
import { expect, test, visit } from '../fixtures';
import { LADDER, LADDER_SOLUTION } from '../puzzles';

const SOLVER_KEY = 'hashi.solver.v2';
const playing = {
  puzzle: LADDER, bridges: edges(LADDER).map(() => 0), solution: LADDER_SOLUTION,
  status: 'Playing' as const, moves: 0, hints: 0,
};

/** The practice page's solver tools, opened from their summary with the keyboard. */
async function openSolverTools(page: Page, path: string) {
  await seedGame(page, PRACTICE_KEY, playing);
  await visit(page, path);
  const tools = page.locator('details:has([role=radiogroup])');
  const summary = tools.locator('summary');
  await summary.focus();
  await summary.press('Enter');
  await expect(tools).toHaveAttribute('open', '');
  return { tools, summary, group: tools.getByRole('radiogroup') };
}

/** Exactly `radio` is checked and the group's one Tab stop. */
async function expectChecked(group: Locator, radio: Locator) {
  await expect(radio).toHaveAttribute('aria-checked', 'true');
  await expect(radio).toHaveAttribute('tabindex', '0');
  await expect(group.getByRole('radio', { checked: true })).toHaveCount(1);
  await expect(group.locator('[role=radio][tabindex="0"]')).toHaveCount(1);
}

const savedSolver = (page: Page) =>
  page.evaluate((key) => JSON.parse(localStorage.getItem(key) ?? 'null')?.kind, SOLVER_KEY);

test('tab into the solver choice and pick a solver with the arrow keys', async ({ page }) => {
  const { tools, summary, group } = await openSolverTools(page, '/practice');
  await expect(page.getByRole('radiogroup', { name: 'Solver', exact: true })).toBeVisible();
  const cdcl = group.getByRole('radio', { name: 'Own CDCL' });
  const dpll = group.getByRole('radio', { name: 'Own DPLL' });
  const splr = group.getByRole('radio', { name: 'splr' });
  await expectChecked(group, cdcl);

  // Tab no longer skips the group: it stops at the checked solver
  await summary.press('Tab');
  await expect(cdcl).toBeFocused();

  await page.keyboard.press('ArrowRight');
  await expect(dpll).toBeFocused();
  await expectChecked(group, dpll);
  await expect(cdcl).toHaveAttribute('aria-checked', 'false');
  await expect(cdcl).toHaveAttribute('tabindex', '-1');
  await expect.poll(() => savedSolver(page)).toBe('Dpll');
  await expect(tools.locator('.terminal')).toContainText('$ hashi solve --solver dpll');

  // Around the end and back, Home and End, up and down
  await page.keyboard.press('ArrowDown');
  await expect(splr).toBeFocused();
  await expectChecked(group, splr);
  await page.keyboard.press('ArrowRight');
  await expect(cdcl).toBeFocused();
  await expectChecked(group, cdcl);
  await page.keyboard.press('ArrowLeft');
  await expect(splr).toBeFocused();
  await expectChecked(group, splr);
  await page.keyboard.press('Home');
  await expect(cdcl).toBeFocused();
  await expectChecked(group, cdcl);
  await page.keyboard.press('End');
  await expect(splr).toBeFocused();
  await expectChecked(group, splr);
  await page.keyboard.press('ArrowUp');
  await expect(dpll).toBeFocused();
  await expectChecked(group, dpll);

  // One Tab stop: out of the group in one step and back to the checked solver
  await page.keyboard.press('Tab');
  await expect(tools.locator('#heuristic')).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(dpll).toBeFocused();

  // The radios are buttons: Space and Enter check the focused one
  await cdcl.focus();
  await page.keyboard.press('Space');
  await expectChecked(group, cdcl);
  await dpll.focus();
  await page.keyboard.press('Enter');
  await expectChecked(group, dpll);

  // The chosen solver is the one that runs
  await tools.getByRole('button', { name: 'Unique?' }).click();
  await expect(tools.locator('.terminal')).toContainText('s SATISFIABLE');
  await expect(tools.locator('.terminal')).toContainText(/c config\s*Own DPLL/);

  // And it is still chosen after a reload
  await visit(page, '/practice');
  await expect(page.locator('[role=radiogroup] [aria-checked=true]')).toHaveText('Own DPLL');
});

test('in Arabic the left arrow goes to the next solver', async ({ page }) => {
  const { summary, group } = await openSolverTools(page, '/ar/practice');
  const radios = group.getByRole('radio');
  await expect(radios).toHaveCount(3);
  await summary.press('Tab');
  await expect(radios.nth(0)).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(radios.nth(1)).toBeFocused();
  await expectChecked(group, radios.nth(1));
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(radios.nth(2)).toBeFocused();
  await expectChecked(group, radios.nth(2));
  await expect.poll(() => savedSolver(page)).toBe('Splr');
});

test('pick a brush with the arrow keys and draw with it', async ({ page }) => {
  await visit(page, '/create');
  const group = page.getByRole('radiogroup', { name: 'Brush', exact: true });
  const radios = group.getByRole('radio');
  await expect(radios).toHaveCount(10);
  const countUp = group.getByRole('radio', { name: 'Each tap counts up' });
  const erase = group.getByRole('radio', { name: 'Erase' });
  await expectChecked(group, countUp);

  await countUp.focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  const two = group.getByRole('radio', { name: 'Place a 2' });
  await expect(two).toBeFocused();
  await expectChecked(group, two);
  await page.keyboard.press('End');
  await expect(erase).toBeFocused();
  await expectChecked(group, erase);
  await page.keyboard.press('ArrowRight');
  await expect(countUp).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowUp');
  const eight = group.getByRole('radio', { name: 'Place a 8' });
  await expect(eight).toBeFocused();
  await expectChecked(group, eight);

  // One Tab stop: the next Tab leaves the group, Shift+Tab comes back to the checked brush
  await page.keyboard.press('Tab');
  expect(await group.evaluate((element) => element.contains(document.activeElement))).toBe(false);
  await page.keyboard.press('Shift+Tab');
  await expect(eight).toBeFocused();

  // The picked brush places its number
  const { x, y } = await cellCenter(page, 1, 1);
  await page.mouse.click(x, y);
  await expect(board(page).locator('g.island text')).toHaveText(['8']);
});
