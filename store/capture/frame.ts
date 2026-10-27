// Frames a captured page for a store: the caption of the scene in the language over a device with
// the screenshot in it (store/templates/frame.html), at the store's size.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import type { Browser, Page } from '@playwright/test';

import { RTL_LOCALES } from '../../e2e/site';
import { SCREENS, type Frame } from './devices';
import { RAW_DIR, ROOT, STORE_URL } from './env';
import type { SceneId } from './scenes';

const TEMPLATES = path.join(ROOT, 'store/templates');

export interface Caption {
  title: string;
  text?: string;
}

/** The hanko 判子 above each caption, the scene in one kanji as the site stamps its pages. */
export const HANKO: Record<SceneId, string> = {
  daily: '橋',
  ranking: '競',
  practice: '練',
  friends: '仲',
  stamps: '判',
  night: '夜',
};

/** A page that serves the templates and the raw screenshots from the site's origin (its fonts). */
export async function framePage(browser: Browser): Promise<Page> {
  const context = await browser.newContext({ viewport: { width: 1000, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  await page.route(`${STORE_URL}/__store/**`, async (route) => {
    const url = new URL(route.request().url());
    const rest = url.pathname.replace(/^\/__store\//, '');
    if (rest.startsWith('raw/')) {
      const file = path.join(RAW_DIR, rest.slice(4));
      if (!file.startsWith(RAW_DIR)) return route.abort();
      return route.fulfill({ body: readFileSync(file), contentType: 'image/png' });
    }
    return route.fulfill({ body: readFileSync(path.join(TEMPLATES, rest)), contentType: 'text/html; charset=utf-8' });
  });
  return page;
}

/** Writes one store image of a scene. */
export async function frame(
  page: Page,
  { frame, lang, scene, caption, dark, out }: { frame: Frame; lang: string; scene: SceneId; caption: Caption; dark: boolean; out: string },
) {
  const screen = SCREENS[frame.screen];
  await page.setViewportSize({ width: frame.width, height: frame.height });
  const config = {
    width: frame.width,
    height: frame.height,
    kind: frame.screen,
    store: frame.store,
    sizes: frame.sizes,
    image: `/__store/raw/${lang}/${scene}-${frame.screen}.png`,
    screen,
    lang,
    dir: RTL_LOCALES.has(lang) ? 'rtl' : 'ltr',
    dark,
    hanko: HANKO[scene],
    title: caption.title,
    text: caption.text,
  };
  await page.goto(`${STORE_URL}/__store/frame.html?frame=${encodeURIComponent(JSON.stringify(config))}`);
  await waitReady(page);
  await page.screenshot({ path: out, animations: 'disabled', caret: 'hide' });
}

/** Writes Google Play's feature graphic in a language (store/templates/feature.html). */
export async function feature(page: Page, { lang, caption, out }: { lang: string; caption: Caption; out: string }) {
  await page.setViewportSize({ width: 1024, height: 500 });
  const config = {
    lang,
    dir: RTL_LOCALES.has(lang) ? 'rtl' : 'ltr',
    title: caption.title,
    text: caption.text,
    image: `/__store/raw/${lang}/board.png`,
  };
  await page.goto(`${STORE_URL}/__store/feature.html?feature=${encodeURIComponent(JSON.stringify(config))}`);
  await waitReady(page);
  await page.screenshot({ path: out, animations: 'disabled', caret: 'hide' });
}

/** Until the template has laid itself out (window.READY), or failed (window.FAILED). */
export async function waitReady(page: Page) {
  type Status = { READY?: boolean; FAILED?: string };
  await page.waitForFunction(() => (window as Status).READY || (window as Status).FAILED, null, { timeout: 30_000 });
  const failed = await page.evaluate(() => (window as Status).FAILED);
  if (failed) throw new Error(`the template failed: ${failed}`);
}
