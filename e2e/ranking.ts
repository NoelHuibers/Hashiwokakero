// Stable examples of the two layouts that are easy to break on phones: shared medals and
// long races with penalties. Used by behavior tests and screenshot tests.
import type { Page } from '@playwright/test';

export async function rankingExample(page: Page, kind: 'shared' | 'long' | 'many') {
  const shared = kind !== 'long';
  const entries = kind === 'many'
    ? Array.from({ length: 8 }, (_, i) => ({ rank: 1, user_id: 501 + i, name: `Winner ${i + 1}`, time_ms: 7830 }))
    : shared
      ? [
          { rank: 1, user_id: 501, name: 'Noel', time_ms: 7826, stamp: 'Streak7' },
          { rank: 1, user_id: 502, name: 'Nele', time_ms: 7831, stamp: 'Oni' },
          { rank: 3, user_id: 503, name: 'Swift Macaque 71', time_ms: 8803 },
          { rank: 4, user_id: 504, name: 'Ein_eifriger_Crane', time_ms: 12140 },
          { rank: 5, user_id: 505, name: 'Lucky_Kitsune', time_ms: 12210 },
        ]
      : [
          { rank: 1, user_id: 502, name: 'Nele', time_ms: 1724320, checks: 4, stamp: 'Oni' },
          { rank: 2, user_id: 501, name: 'Noel', time_ms: 1932210, checks: 20, stamp: 'Streak7' },
          { rank: 3, user_id: 503, name: 'Bierosaufus_Maximus', time_ms: 2273880, hints: 9 },
          { rank: 4, user_id: 504, name: 'Jessi', time_ms: 3032280, hints: 2, checks: 11, stamp: 'Oni' },
          { rank: 5, user_id: 505, name: 'Reichus', time_ms: 7427130, hints: 5, checks: 9 },
        ];
  const complete = entries.map((entry) => ({ guest: false, hints: 0, checks: 0, stamp: null, ...entry }));
  await page.route('**/api/daily_leaderboard', (route) => route.fulfill({
    json: {
      day: 20752, difficulty: shared ? 'Easy' : 'Oni', finishers: complete.length,
      me: complete.find((e) => e.user_id === 501), entries: complete,
    },
  }));
}
