// Fixed puzzles, for tests that need to know where the bridges go before the page does.
import { edges, type Puzzle } from './board';

/**
 * A ladder on a 7×7 board: an island at both ends of rows 1 to 5, the left ones joined from top
 * to bottom. Every row's bridge crosses the whole board, so there is water to tap almost
 * anywhere in the middle.
 */
export const LADDER: Puzzle = {
  width: 7,
  height: 7,
  islands: [1, 2, 3, 4, 5].flatMap((y) => [
    { pos: { x: 0, y }, required: y === 1 || y === 5 ? 2 : 3 },
    { pos: { x: 6, y }, required: 1 },
  ]),
};

/** A bridge on every row and down the left side. */
export const LADDER_SOLUTION: number[] = edges(LADDER).map(([a, b]) => {
  const [p, q] = [LADDER.islands[a].pos, LADDER.islands[b].pos];
  return p.y === q.y || p.x === 0 ? 1 : 0;
});

/** The edge of a row of the ladder. */
export const ladderRow = (y: number) =>
  edges(LADDER).findIndex(([a, b]) => LADDER.islands[a].pos.y === y && LADDER.islands[b].pos.y === y);
