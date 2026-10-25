// The time from navigation to a hydrated page: the WASM downloaded, compiled and run over the
// server's HTML, until App sets its `data-hydrated` mark (src/lib.rs). Each page loads cold (a
// new context, nothing cached) several times on a CPU slowed down like a mid-range phone's, the
// median goes to e2e/test-results-perf/hydration.json for `pnpm run bundle-size`.
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { expect, test } from '@playwright/test';

/** The daily (home), the practice board and the ranks. */
const PAGES = ['/', '/practice', '/ranks'];
const RUNS = Number(process.env.PERF_RUNS ?? 5);
/** How many times slower the CPU runs (DevTools' throttling). */
const CPU_THROTTLING = Number(process.env.PERF_CPU ?? 4);

const RESULTS = path.join(import.meta.dirname, '../test-results-perf/hydration.json');

interface Load {
  /** When the HTML's first byte arrived, in ms after navigation started. */
  firstByte: number;
  domContentLoaded: number;
  hydrated: number;
}

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
const round = (n: number) => Math.round(n);

test('hydration times', async ({ browser, baseURL }) => {
  const load = async (page: string): Promise<Load> => {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'light' });
    try {
      const tab = await context.newPage();
      const cdp = await context.newCDPSession(tab);
      await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU_THROTTLING });
      // A performance mark the moment App sets its mark, before the page could tell otherwise.
      // The document has no element yet when this runs, the observer waits for <html>.
      await tab.addInitScript(() => {
        const observer = new MutationObserver(() => {
          if (document.documentElement?.hasAttribute('data-hydrated')) {
            performance.mark('hashi:hydrated');
            observer.disconnect();
          }
        });
        observer.observe(document, { subtree: true, attributes: true, attributeFilter: ['data-hydrated'] });
      });
      const response = await tab.goto(new URL(page, baseURL).href);
      expect(response?.status(), `HTTP status of ${page}`).toBe(200);
      await expect(tab.locator('html')).toHaveAttribute('data-hydrated', '', { timeout: 60_000 });
      return await tab.evaluate(() => {
        const [nav] = performance.getEntriesByType('navigation') as PerformanceNavigationTiming[];
        const [mark] = performance.getEntriesByName('hashi:hydrated');
        return { firstByte: nav.responseStart, domContentLoaded: nav.domContentLoadedEventEnd, hydrated: mark.startTime };
      });
    } finally {
      await context.close();
    }
  };

  // The browser's own start shouldn't count for the first page
  await load(PAGES[0]);
  const pages: Record<string, { median: number; min: number; max: number; runs: Load[] }> = {};
  for (const page of PAGES) {
    const runs: Load[] = [];
    for (let i = 0; i < RUNS; i++) runs.push(await load(page));
    const hydrated = runs.map((r) => r.hydrated);
    pages[page] = {
      median: round(median(hydrated)),
      min: round(Math.min(...hydrated)),
      max: round(Math.max(...hydrated)),
      runs: runs.map((r) => ({ firstByte: round(r.firstByte), domContentLoaded: round(r.domContentLoaded), hydrated: round(r.hydrated) })),
    };
    console.log(`${page}: ${pages[page].median} ms (${hydrated.map(round).join(', ')})`);
  }
  mkdirSync(path.dirname(RESULTS), { recursive: true });
  writeFileSync(RESULTS, `${JSON.stringify({ runs: RUNS, cpuThrottling: CPU_THROTTLING, pages }, null, 2)}\n`);
});
