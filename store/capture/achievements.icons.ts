// The icons of the achievements and leaderboards (app/README.md, Game achievements): each stamp's
// hanko (store/templates/hanko.html) at Game Center's 1024 × 1024 and Play's 512 × 512, into
// target/store/achievements/icons/<platform>/<id>.png. No server: the template and the fonts are
// served from the repository (framePage's offline mode).
import { mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { test } from '@playwright/test';

import { ROOT, STORE_URL } from './env';
import { framePage, waitReady } from './frame';

interface Model {
  achievements: { id: string; kanji: string; tier: number | null }[];
  leaderboards: { id: string; kanji: string; play: boolean }[];
}

const MODEL = JSON.parse(readFileSync(path.join(ROOT, 'store/achievements/model.json'), 'utf8')) as Model;
const ICONS = path.join(ROOT, 'target/store/achievements/icons');
const SIZES = { apple: 1024, play: 512 } as const;

for (const [platform, size] of Object.entries(SIZES)) {
  test(platform, async ({ browser }) => {
    const dir = path.join(ICONS, platform);
    mkdirSync(dir, { recursive: true });
    const page = await framePage(browser, { offline: true });
    await page.setViewportSize({ width: size, height: size });
    const items = [
      ...MODEL.achievements,
      ...MODEL.leaderboards.filter((b) => platform === 'apple' || b.play).map((b) => ({ ...b, tier: null })),
    ];
    for (const { id, kanji, tier } of items) {
      const icon = { size, kanji, tier };
      await page.goto(`${STORE_URL}/__store/hanko.html?icon=${encodeURIComponent(JSON.stringify(icon))}`);
      await waitReady(page);
      await page.screenshot({ path: path.join(dir, `${id}.png`), animations: 'disabled', caret: 'hide' });
    }
    await page.context().close();
  });
}
