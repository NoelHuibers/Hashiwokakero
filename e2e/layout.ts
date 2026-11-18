// Checks of a page's layout that screenshots would only catch in the one state they show.
import { expect, type Locator } from '@playwright/test';

interface Box {
  name: string;
  top: number;
  bottom: number;
  left: number;
  right: number;
}

/**
 * Fails if the cards in `area` leave a hole between them: in each direction, the next card that
 * lies beside a card (or the edge of all the cards together) has to be at most `maxGap` pixels
 * away. A short card in a grid row next to a tall one, or the shorter of two columns ending a
 * card's height above the other, leaves more. The default lets two columns of cards (24 px apart)
 * end a few lines of text apart, which depends on the language and the width; the shortest cards
 * are 180 px tall.
 *
 * A card ends where its content does: one stretched to fill a hole hides it, it doesn't fill it.
 * Only the outermost cards count, not cards inside cards. A card is named by its first line.
 */
export async function expectNoHoles(area: Locator, what: string, { maxGap = 128 } = {}) {
  const boxes: Box[] = await area.evaluate((root) =>
    [...root.querySelectorAll<HTMLElement>('[data-name="Card"]')]
      .filter((card) => !card.parentElement?.closest('[data-name="Card"]'))
      .filter((card) => card.offsetParent !== null)
      .map((card) => {
        const r = card.getBoundingClientRect();
        const name = (card.innerText.trim().split('\n')[0] ?? '').slice(0, 40);
        const style = getComputedStyle(card);
        const content = Math.max(
          r.top,
          ...[...card.children].map((child) => child.getBoundingClientRect().bottom),
        );
        return {
          name,
          top: r.top + scrollY,
          bottom:
            content +
            Number.parseFloat(style.paddingBottom) +
            Number.parseFloat(style.borderBottomWidth) +
            scrollY,
          left: r.left,
          right: r.right,
        };
      }),
  );
  expect(boxes.length, `${what}: cards`).toBeGreaterThan(0);
  const all = {
    top: Math.min(...boxes.map((b) => b.top)),
    bottom: Math.max(...boxes.map((b) => b.bottom)),
    left: Math.min(...boxes.map((b) => b.left)),
    right: Math.max(...boxes.map((b) => b.right)),
  };
  // Beside each other: overlapping by more than a pixel across the direction looked in
  const across = (a: number, b: number, c: number, d: number) =>
    Math.min(b, d) - Math.max(a, c) > 1;
  const holes: string[] = [];
  for (const card of boxes) {
    const others = boxes.filter((b) => b !== card);
    const vertical = others.filter((b) => across(card.left, card.right, b.left, b.right));
    const horizontal = others.filter((b) => across(card.top, card.bottom, b.top, b.bottom));
    const gaps = {
      below: Math.min(
        all.bottom - card.bottom,
        ...vertical.filter((b) => b.top >= card.bottom - 1).map((b) => b.top - card.bottom),
      ),
      above: Math.min(
        card.top - all.top,
        ...vertical.filter((b) => b.bottom <= card.top + 1).map((b) => card.top - b.bottom),
      ),
      'to the right': Math.min(
        all.right - card.right,
        ...horizontal.filter((b) => b.left >= card.right - 1).map((b) => b.left - card.right),
      ),
      'to the left': Math.min(
        card.left - all.left,
        ...horizontal.filter((b) => b.right <= card.left + 1).map((b) => card.left - b.right),
      ),
    };
    for (const [where, gap] of Object.entries(gaps)) {
      if (gap > maxGap) holes.push(`${Math.round(gap)} px empty ${where} "${card.name}"`);
    }
  }
  expect(holes, `${what}: holes between the cards`).toEqual([]);
}
