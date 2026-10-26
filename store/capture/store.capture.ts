// The store screenshots: every scene (scenes.ts) in every language of the site, on a phone and a
// tablet screen (devices.ts), then framed with a caption for each store (frame.ts). One test per
// language, they run side by side.
import { mkdirSync } from 'node:fs';
import path from 'node:path';

import { expect, test, type Browser } from '@playwright/test';

import { hydrated } from '../../e2e/fixtures';
import { localized } from '../../e2e/site';
import { readFileSync } from 'node:fs';

import { SITE_LANGUAGES } from '../locales.mjs';
import { FRAMES, SCREENS, type Screen } from './devices';
import { IMAGES_DIR, ONLY_LANGS, RAW_DIR, ROOT, STORE_URL } from './env';
import { feature, frame, framePage, type Caption } from './frame';
import { boardShot, SCENES, signIn, type Scene, type SceneId } from './scenes';

const LANGS = (SITE_LANGUAGES as string[]).filter((l) => !ONLY_LANGS || ONLY_LANGS.includes(l));
const CAPTIONS = JSON.parse(readFileSync(path.join(ROOT, 'store/captions.json'), 'utf8')) as Record<
  string,
  Record<SceneId | 'feature', Caption>
>;
/** Frame the screenshots of an earlier run again, without taking them: the templates' turn. */
const FRAME_ONLY = !!process.env.STORE_FRAME_ONLY;

for (const lang of LANGS) {
  test(`${lang}`, async ({ browser }) => {
    mkdirSync(path.join(RAW_DIR, lang), { recursive: true });
    mkdirSync(path.join(IMAGES_DIR, lang), { recursive: true });
    if (!FRAME_ONLY) {
      for (const scene of SCENES) {
        for (const screen of Object.values(SCREENS)) {
          await capture(browser, lang, scene, screen);
        }
      }
    }
    if (!FRAME_ONLY) {
      const context = await browser.newContext({ viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 2 });
      await boardShot(context, path.join(RAW_DIR, lang, 'board.png'));
      await context.close();
    }
    const page = await framePage(browser);
    await feature(page, { lang, caption: CAPTIONS[lang].feature, out: path.join(IMAGES_DIR, lang, 'feature.png') });
    for (const f of FRAMES) {
      const dir = path.join(IMAGES_DIR, lang, f.id);
      mkdirSync(dir, { recursive: true });
      for (const [i, scene] of SCENES.entries()) {
        const caption = CAPTIONS[lang]?.[scene.id];
        if (!caption) throw new Error(`no caption for ${scene.id} in ${lang} (store/captions.json)`);
        await frame(page, { frame: f, lang, scene: scene.id, caption, dark: !!scene.dark, out: path.join(dir, `${i + 1}-${scene.id}.png`) });
      }
    }
    await page.context().close();
  });
}

/** A page of the site as the scene's player left it, the whole screen. */
async function capture(browser: Browser, lang: string, scene: Scene, screen: Screen) {
  const context = await browser.newContext({
    viewport: { width: screen.width, height: screen.height },
    deviceScaleFactor: screen.scale,
    isMobile: true,
    hasTouch: true,
    locale: lang,
    timezoneId: 'UTC',
    colorScheme: scene.dark ? 'dark' : 'light',
    reducedMotion: 'reduce',
  });
  try {
    await signIn(context, scene.user);
    await scene.before?.(context);
    const page = await context.newPage();
    const response = await page.goto(localized(lang, scene.path));
    expect(response?.status(), `${scene.path} in ${lang}`).toBe(200);
    await hydrated(page);
    // The system bars, as the app's page leaves room for them
    await page.addStyleTag({
      content: `:root { --app-inset-top: ${screen.insetTop}px !important; --app-inset-bottom: ${screen.insetBottom}px !important; }`,
    });
    await scene.play?.(page);
    if (scene.scrollTo?.screens.includes(screen.id)) {
      const target = scene.scrollTo.target(page);
      await expect(target).toBeVisible();
      await target.evaluate((el, above) => {
        const header = document.querySelector('header')?.getBoundingClientRect().bottom ?? 0;
        window.scrollTo(0, el.getBoundingClientRect().top + window.scrollY - header - above);
      }, scene.scrollTo.above ?? 40);
    }
    // The local server's address where the app shows hashi.rs (the group's invite link)
    await page.evaluate((local) => {
      for (const input of document.querySelectorAll('input')) {
        if (input.value.startsWith(local)) input.value = input.value.replace(local, 'https://hashi.rs');
      }
    }, STORE_URL);
    await page.evaluate(() => document.fonts.ready);
    // Toasts and the last transitions gone
    await page.waitForTimeout(800);
    await page.screenshot({
      path: path.join(RAW_DIR, lang, `${scene.id}-${screen.id}.png`),
      animations: 'disabled',
      caret: 'hide',
    });
  } finally {
    await context.close();
  }
}
