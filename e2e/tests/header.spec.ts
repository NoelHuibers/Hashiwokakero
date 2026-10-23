// The header in every language: on a 320 px phone the logo and the help, language, settings
// and account buttons share the screen, each big enough to tap; on a tablet the tab bar takes
// the six links, from lg on they're in the header, from xl next to the player's name. A phone browser
// makes the page as wide as its widest part and zooms out: whatever sticks out of a page pushes
// the header's last button off the screen, so the pages are checked whole too.
import type { Page } from '@playwright/test';

import { register } from '../account';
import { expect, newUsername, signIn, test, visit } from '../fixtures';
import { SEEDED_GROUP } from '../seed';
import { localized, LOCALES } from '../site';

/** The pages the tab bar leads to, a group, the account and the rules. */
const ALL = ['/', '/practice', '/ranks', '/groups', `/groups/${SEEDED_GROUP.id}`, '/create', '/lab', '/account', '/rules'];
const SOME = ['/', '/groups', '/ranks', '/account'];

/** What doesn't fit `width`: buttons off the screen or too small, a squeezed wordmark, overflow. */
async function misfits(page: Page, width: number, { tap }: { tap: boolean }) {
  return page.evaluate(
    ({ width, tap }) => {
      const problems: string[] = [];
      if (innerWidth !== width) problems.push(`the page is ${innerWidth} px wide`);
      const scroll = document.documentElement.scrollWidth;
      if (scroll > width) problems.push(`it scrolls sideways to ${scroll} px`);
      const row = document.querySelector('header > div')!;
      if (row.scrollWidth > row.clientWidth) problems.push(`the header's row needs ${row.scrollWidth} px`);
      for (const el of document.querySelectorAll('header a, header button')) {
        const box = el.getBoundingClientRect();
        if (!box.width) continue;
        const name = el.getAttribute('aria-label') ?? el.textContent?.trim();
        if (box.left < 0 || box.right > width) problems.push(`${name} lies at ${box.left}..${box.right}`);
        if (tap && (box.width < 44 || box.height < 44)) problems.push(`${name} is ${box.width}×${box.height}`);
      }
      // No text in it wraps: the wordmark, the tagline, the links' names
      for (const el of document.querySelectorAll<HTMLElement>('header span')) {
        if (el.children.length || !el.textContent?.trim() || !el.offsetHeight) continue;
        // A second line starts at least a font size below the first
        const text = document.createRange();
        text.selectNodeContents(el);
        const tops = [...text.getClientRects()].map((r) => r.top);
        const size = parseFloat(getComputedStyle(el).fontSize);
        if (Math.max(...tops) - Math.min(...tops) > 0.8 * size) problems.push(`${el.textContent} wraps`);
      }
      return problems;
    },
    { width, tap },
  );
}

async function expectFits(page: Page, locale: string, width: number, pages: string[], { tap = true } = {}) {
  for (const path of pages) {
    await visit(page, localized(locale, path));
    await page.evaluate(() => document.fonts.ready);
    expect(await misfits(page, width, { tap }), `${localized(locale, path)} at ${width} px`).toEqual([]);
  }
}

test.describe('phone', () => {
  // As a phone: a page wider than the screen widens the layout viewport, header and all
  test.use({ isMobile: true, hasTouch: true });

  for (const locale of LOCALES) {
    test(`the header fits 320 px in ${locale}, signed out and as a guest at night`, async ({ page, context }) => {
      await page.setViewportSize({ width: 320, height: 640 });
      await expectFits(page, locale, 320, SOME);
      // A member of the seeded group
      await signIn(context, 'Drowsy Otter 21');
      await page.emulateMedia({ colorScheme: 'dark' });
      await expectFits(page, locale, 320, ALL);
    });

    test(`the header fits 390 px in ${locale}, signed in`, async ({ page, context }) => {
      await signIn(context, 'aiko');
      await page.setViewportSize({ width: 390, height: 760 });
      await expectFits(page, locale, 390, SOME);
    });
  }
});

// The name is shown from xl on, the longest one a username can be in its widest letters
for (const locale of LOCALES) {
  test(`the header fits a tablet and laptops in ${locale} with a long name`, async ({ page }) => {
    await visit(page, '/account');
    await register(page, newUsername('WWWWWWWWWW'), 'eight-planks-wide');
    for (const width of [768, 1024, 1280]) {
      await page.setViewportSize({ width, height: 800 });
      await expectFits(page, locale, width, ['/', '/groups'], { tap: width < 1024 });
    }
  });
}
