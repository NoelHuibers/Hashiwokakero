// The headings' font in every language. Kaisei Decol (--display-font in style/tailwind.css) is
// a Japanese font: it draws Cyrillic as wide as kanji, so a Russian heading looked letterspaced
// (Г р у п п ы), and it lacks Ukrainian's і ї є ґ, Polish ą ę ś ź ż and Turkish ğ ş İ, which
// came from another font in the middle of a word. Asks Chromium which fonts drew each heading.
import type { CDPSession, Page } from '@playwright/test';

import { expect, signIn, test, visit } from '../fixtures';
import { localized, LOCALES } from '../site';

/** Where headings are in Kaisei Decol: languages it draws whole, in proportional letters. */
const KAISEI = new Set(['en', 'de', 'es', 'fr', 'id', 'it', 'ja', 'nl', 'pt']);
// Scripts whose shaping makes fewer glyphs than letters (ligatures, conjuncts)
const SHAPED = new Set(['ar', 'hi']);

const PAGES = ['/', '/groups', '/ranks', '/account'];

// Headings, the small bold capitals and whatever text sits in them
const SELECTOR = ':is(h1, h2, h3, .uppercase.font-bold), :is(h1, h2, h3, .uppercase.font-bold) *';

interface Drawn {
  text: string;
  fonts: { familyName: string; glyphCount: number }[];
}

/** The text right inside each visible heading on the page and the fonts that drew it. */
async function headings(page: Page, cdp: CDPSession): Promise<Drawn[]> {
  // Marks the visible ones, CDP finds them by the mark
  const texts = await page.evaluate((selector) => {
    const texts: string[] = [];
    for (const el of document.querySelectorAll(selector)) {
      const own = [...el.childNodes].filter((c) => c.nodeType === Node.TEXT_NODE).map((c) => c.textContent).join('');
      if (!own.trim() || !el.getClientRects().length) continue;
      el.setAttribute('data-type-test', String(texts.length));
      texts.push(own);
    }
    return texts;
  }, SELECTOR);
  expect(texts.length, 'headings on the page').toBeGreaterThan(0);
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeIds } = await cdp.send('DOM.querySelectorAll', { nodeId: root.nodeId, selector: '[data-type-test]' });
  return Promise.all(
    nodeIds.map(async (nodeId) => {
      const { attributes } = await cdp.send('DOM.getAttributes', { nodeId });
      const mark = attributes[attributes.indexOf('data-type-test') + 1];
      const { fonts } = await cdp.send('CSS.getPlatformFontsForNode', { nodeId });
      return { text: texts[Number(mark)], fonts };
    }),
  );
}

for (const locale of LOCALES) {
  test(`headings are drawn in one fitting font in ${locale}`, async ({ page, context }) => {
    await signIn(context, 'aiko');
    const cdp = await context.newCDPSession(page);
    await cdp.send('DOM.enable');
    await cdp.send('CSS.enable');
    for (const path of PAGES) {
      await visit(page, localized(locale, path));
      await page.evaluate(() => document.fonts.ready);
      for (const { text, fonts } of await headings(page, cdp)) {
        const families = fonts.map((f) => f.familyName);
        const where = `"${text.trim()}" on ${path} (${families.join(', ')})`;
        if (!KAISEI.has(locale)) {
          expect(families, `not in Kaisei Decol: ${where}`).not.toContain('Kaisei Decol');
        }
        // Kanji and kana may come from another font than the language's letters
        if (SHAPED.has(locale) || /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u.test(text)) continue;
        // Every letter from the first font: only spaces, digits and signs may come from another
        // (CDP counts the text of the children too: the pixel font's kanji next to titles)
        const [first, ...others] = fonts
          .filter((f) => f.familyName !== 'DotGothic16')
          .sort((a, b) => b.glyphCount - a.glyphCount);
        const notLetters = [...text].filter((c) => !/\p{L}/u.test(c)).length;
        const fallback = others.reduce((sum, f) => sum + f.glyphCount, 0);
        expect(fallback, `letters of ${where} drawn by another font than ${first.familyName}`).toBeLessThanOrEqual(notLetters);
      }
    }
  });
}

// What the test above relies on: in the languages Kaisei Decol draws, the headings are in it
test('headings are in Kaisei Decol in English and Japanese', async ({ page, context }) => {
  const cdp = await context.newCDPSession(page);
  await cdp.send('DOM.enable');
  await cdp.send('CSS.enable');
  for (const locale of ['en', 'ja']) {
    await visit(page, localized(locale, '/ranks'));
    await page.evaluate(() => document.fonts.ready);
    const families = (await headings(page, cdp)).flatMap((h) => h.fonts.map((f) => f.familyName));
    expect(families).toContain('Kaisei Decol');
  }
});
