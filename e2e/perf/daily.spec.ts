// How long the daily's two moments that count take in the browser, on a CPU slowed down like a
// mid-range phone's: from pressing Start to a board to play (the clock already runs), and from
// the last bridge to the win card with the clock stopped. Each split into the browser's part
// and the server's reply, so a slower page shows apart from a slower network. The results go to
// e2e/test-results-perf/daily.json and, on CI, the job summary; they are only reported, a runner's
// speed varies too much (the browser tests in e2e/tests/daily-speed.spec.ts are the guard).
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { addBridge, board, DAILY_KEY, missing, saved } from '../board.ts';
import { dailySolution, hintUntil } from '../daily.ts';
import { expect, test, visit } from '../fixtures.ts';

const CPU_THROTTLING = Number(process.env.PERF_CPU ?? 4);
const RESULTS = path.join(import.meta.dirname, '../test-results-perf/daily.json');

interface Marks {
  /** Every pointerup, the press of Start and the taps of the last bridge. */
  ups: number[];
  board?: number;
  win?: number;
}

test('daily start and finish', async ({ page }) => {
  test.setTimeout(5 * 60_000);
  const cdp = await page.context().newCDPSession(page);
  await visit(page, '/');
  const start = page.getByRole('button', { name: /^Start/ });
  await expect(start).toBeEnabled();
  await page.evaluate(() => {
    const marks: Marks = { ups: [] };
    (window as unknown as { marks: Marks }).marks = marks;
    document.addEventListener('pointerup', () => marks.ups.push(performance.now()), {
      capture: true,
    });
    new MutationObserver(() => {
      if (!marks.board && document.querySelector('svg.board-svg.board-interactive g.island'))
        marks.board = performance.now();
      if (!marks.win && document.querySelector('[data-testid="daily-win"]'))
        marks.win = performance.now();
    }).observe(document.body, { subtree: true, childList: true, attributes: true });
  });

  await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU_THROTTLING });
  await start.click();
  await expect(board(page)).toHaveClass(/board-interactive/, { timeout: 30_000 });
  const started = await page.evaluate(() => {
    const marks = (window as unknown as { marks: Marks }).marks;
    const reply = performance
      .getEntriesByType('resource')
      .filter((e) => e.name.includes('/api/start_daily'))
      .at(-1) as PerformanceResourceTiming;
    return {
      pressToRequest: reply.requestStart - marks.ups[0]!,
      reply: reply.responseEnd - reply.requestStart,
      replyToBoard: marks.board! - reply.responseEnd,
      total: marks.board! - marks.ups[0]!,
    };
  });

  // Most of the board by hints, quickly, then the last bridge by hand again on the slow CPU
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });
  const { puzzle } = await saved(page, DAILY_KEY);
  const solution = dailySolution(puzzle);
  await hintUntil(page, solution, 1);
  const [last] = missing((await saved(page, DAILY_KEY)).bridges, solution);
  if (last === undefined) throw new Error('hints left no bridge of the solution to build');
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU_THROTTLING });
  for (let n = (await saved(page, DAILY_KEY)).bridges[last] ?? 0; n < solution[last]!; n++)
    await addBridge(page, DAILY_KEY, puzzle, last);
  await expect(page.getByTestId('daily-win')).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/^Rank \d+ of \d+ today$/).first()).toBeVisible();
  const finished = await page.evaluate(() => {
    const marks = (window as unknown as { marks: Marks }).marks;
    const reply = performance
      .getEntriesByType('resource')
      .filter((e) => e.name.includes('/api/submit_daily'))
      .at(-1) as PerformanceResourceTiming;
    const tap = marks.ups.filter((t) => t < marks.win!).at(-1)!;
    return {
      tapToWinCard: marks.win! - tap,
      tapToRequest: reply.requestStart - tap,
      reply: reply.responseEnd - reply.requestStart,
    };
  });
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });

  const round = (o: Record<string, number>) =>
    Object.fromEntries(Object.entries(o).map(([k, v]) => [k, Math.round(v)]));
  const results = { cpuThrottling: CPU_THROTTLING, start: round(started), finish: round(finished) };
  mkdirSync(path.dirname(RESULTS), { recursive: true });
  writeFileSync(RESULTS, `${JSON.stringify(results, null, 2)}\n`);
  const lines = [
    `Start → board: ${results.start.total} ms (reply ${results.start.reply} ms, reply → board ${results.start.replyToBoard} ms)`,
    `Last bridge → win card: ${results.finish.tapToWinCard} ms (the submit's reply took ${results.finish.reply} ms)`,
  ];
  console.log(lines.join('\n'));
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `### The daily, CPU ${CPU_THROTTLING}× slower\n\n${lines.map((l) => `- ${l}`).join('\n')}\n`,
    );
  }
});
