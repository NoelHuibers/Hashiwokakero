//! Solving tips, each with a small board that shows the bridges it forces: all of them on the
//! rules page, the first three under the daily.

use common::{Board, Pos, Puzzle};
use leptos::prelude::*;

use crate::components::board::{BoardMode, BoardView};
use crate::i18n::{t, t_string, use_i18n};

/// An island at (x, y) with its number.
type TipIsland = ((u8, u8), u8);
/// A bridge between the islands at two cells (x, y) with its count.
type TipBridge = ((u8, u8), (u8, u8), u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    /// A 4 in a corner has a double bridge to both neighbours.
    Corner,
    /// Two 1s never connect to each other.
    Ones,
    /// A 5 with three neighbours gets at least one bridge to each.
    Count,
    /// A bridge blocks the line it crosses.
    Block,
    /// A group must not be closed off.
    Connect,
}

impl Tip {
    pub const ALL: [Self; 5] = [
        Self::Corner,
        Self::Ones,
        Self::Count,
        Self::Block,
        Self::Connect,
    ];

    /// Width, height and islands of a small puzzle with exactly one solution, and the bridges of
    /// it that the tip finds.
    fn layout(self) -> (u8, u8, &'static [TipIsland], &'static [TipBridge]) {
        match self {
            Self::Corner => (
                5,
                3,
                &[((0, 0), 4), ((4, 0), 3), ((0, 2), 2), ((4, 2), 1)],
                &[((0, 0), (4, 0), 2), ((0, 0), (0, 2), 2)],
            ),
            Self::Ones => (
                5,
                3,
                &[((0, 0), 1), ((4, 0), 1), ((0, 2), 3), ((4, 2), 3)],
                &[((0, 0), (0, 2), 1), ((4, 0), (4, 2), 1)],
            ),
            Self::Count => (
                5,
                3,
                &[
                    ((2, 0), 5),
                    ((0, 0), 2),
                    ((4, 0), 3),
                    ((2, 2), 4),
                    ((0, 2), 3),
                    ((4, 2), 1),
                ],
                &[
                    ((0, 0), (2, 0), 1),
                    ((2, 0), (4, 0), 1),
                    ((2, 0), (2, 2), 1),
                ],
            ),
            // The double bridge in the middle is the 2's only way, the 1 can't reach the 2 on
            // the right past it
            Self::Block => (
                5,
                4,
                &[
                    ((2, 0), 2),
                    ((0, 1), 1),
                    ((4, 1), 2),
                    ((0, 3), 2),
                    ((2, 3), 4),
                    ((4, 3), 3),
                ],
                &[((2, 0), (2, 3), 2), ((0, 1), (0, 3), 1)],
            ),
            // The 1 has only the 3. If the 3 gave its other two bridges to the 2, the three
            // islands would be done and cut off
            Self::Connect => (
                5,
                3,
                &[
                    ((0, 0), 1),
                    ((2, 0), 3),
                    ((4, 0), 2),
                    ((2, 2), 2),
                    ((4, 2), 2),
                ],
                &[((0, 0), (2, 0), 1), ((2, 0), (2, 2), 1)],
            ),
        }
    }

    /// The tip's board with the bridges it finds.
    pub fn board(self) -> (Board, Vec<u8>) {
        let (width, height, islands, bridges) = self.layout();
        let mut puzzle = Puzzle::empty(width, height);
        for &((x, y), n) in islands {
            puzzle.set(Pos::new(x, y), n);
        }
        let board = Board::new(puzzle);
        let mut counts = board.empty_bridges();
        for &(a, b, n) in bridges {
            if let Some(e) = edge(&board, a, b) {
                counts[e] = n;
            }
        }
        (board, counts)
    }
}

/// The edge between the islands at two cells.
fn edge(board: &Board, (ax, ay): (u8, u8), (bx, by): (u8, u8)) -> Option<usize> {
    let a = board.puzzle.island_at(Pos::new(ax, ay))?;
    let b = board.puzzle.island_at(Pos::new(bx, by))?;
    board.edge_between(a, b)
}

/// The first `count` tips as cards, a board above each text.
#[component]
pub fn TipCards(
    /// How many of [`Tip::ALL`] to show.
    count: usize,
    /// Classes of the grid of cards.
    #[prop(into)]
    class: String,
    /// The titles are `<h4>`s, below an `<h3>` of the page, instead of `<h3>`s.
    #[prop(optional)]
    nested: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let cards = Tip::ALL
        .into_iter()
        .take(count)
        .map(|tip| {
            let (title, text) = match tip {
                Tip::Corner => (
                    t!(i18n, rules.tips.corner_title).into_any(),
                    t!(i18n, rules.tips.corner).into_any(),
                ),
                Tip::Ones => (
                    t!(i18n, rules.tips.ones_title).into_any(),
                    t!(i18n, rules.tips.ones).into_any(),
                ),
                Tip::Count => (
                    t!(i18n, rules.tips.count_title).into_any(),
                    t!(i18n, rules.tips.count).into_any(),
                ),
                Tip::Block => (
                    t!(i18n, rules.tips.block_title).into_any(),
                    t!(i18n, rules.tips.block).into_any(),
                ),
                Tip::Connect => (
                    t!(i18n, rules.tips.connect_title).into_any(),
                    t!(i18n, rules.tips.connect).into_any(),
                ),
            };
            // What the board shows, for screen readers and search engines
            let label = Signal::derive(move || {
                match tip {
                    Tip::Corner => t_string!(i18n, rules.tips.corner_board),
                    Tip::Ones => t_string!(i18n, rules.tips.ones_board),
                    Tip::Count => t_string!(i18n, rules.tips.count_board),
                    Tip::Block => t_string!(i18n, rules.tips.block_board),
                    Tip::Connect => t_string!(i18n, rules.tips.connect_board),
                }
                .to_string()
            });
            let title = if nested {
                view! { <h4 class="font-bold">{title}</h4> }.into_any()
            } else {
                view! { <h3 class="font-bold">{title}</h3> }.into_any()
            };
            let (board, bridges) = tip.board();
            view! {
                <div class="flex flex-col gap-4 rounded-xl border bg-card p-4 text-card-foreground shadow-sm">
                    <BoardView
                        board=Signal::stored(board)
                        bridges=Signal::stored(bridges)
                        mode=BoardMode::View
                        max_height="8.5rem"
                        framed=true
                        label=label
                    />
                    <div class="flex flex-col gap-1">
                        {title}
                        <p class="text-sm leading-relaxed text-muted-foreground">{text}</p>
                    </div>
                </div>
            }
        })
        .collect_view();
    view! { <div class=class>{cards}</div> }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every board is a piece of a real puzzle: the bridges join islands in line with nothing in
    /// between, one or two at a time, never crossing, and no island gets more than its number.
    #[test]
    fn every_tip_board_is_a_legal_fragment() {
        for tip in Tip::ALL {
            let (width, height, islands, bridges) = tip.layout();
            let (board, counts) = tip.board();
            assert_eq!(board.puzzle.islands.len(), islands.len(), "{tip:?}");
            for &((x, y), n) in islands {
                assert!(
                    x < width && y < height,
                    "{tip:?}: ({x}, {y}) is off the board"
                );
                assert!((1..=8).contains(&n), "{tip:?}: island of {n}");
            }
            assert!(!bridges.is_empty(), "{tip:?} shows no bridge");
            for &(a, b, n) in bridges {
                assert!(
                    edge(&board, a, b).is_some(),
                    "{tip:?}: {a:?} and {b:?} can't be joined"
                );
                assert!((1..=2).contains(&n), "{tip:?}: {n} bridges");
            }
            assert_eq!(
                counts.iter().filter(|&&c| c > 0).count(),
                bridges.len(),
                "{tip:?}"
            );
            let check = board.check(&counts);
            assert!(check.crossing.is_empty(), "{tip:?}: bridges cross");
            for (island, degree) in board.puzzle.islands.iter().zip(&check.degrees) {
                assert!(
                    *degree <= island.required,
                    "{tip:?}: the {} at {:?} has {degree} bridges",
                    island.required,
                    island.pos
                );
            }
        }
    }

    /// The bridges a tip shows are in the solution, the only one.
    #[cfg(feature = "ssr")]
    #[test]
    fn tips_show_bridges_of_the_only_solution() {
        use common::{SolveOutcome, SolverConfig};

        for tip in Tip::ALL {
            let (board, counts) = tip.board();
            let report = backend::api::solve_unique(&board.puzzle, &SolverConfig::default())
                .expect("the puzzle can be solved");
            let SolveOutcome::Solved { bridges, unique } = report.outcome else {
                panic!("{tip:?} has no solution");
            };
            assert_eq!(unique, Some(true), "{tip:?} has more than one solution");
            for (e, (&shown, &solved)) in counts.iter().zip(&bridges).enumerate() {
                assert!(
                    shown <= solved,
                    "{tip:?}: {shown} bridges on {:?}, the solution has {solved}",
                    board.endpoints(e)
                );
            }
        }
    }

    /// The boards show what their texts say.
    #[test]
    fn tips_show_what_they_say() {
        let degree = |tip: Tip, x, y| {
            let (board, counts) = tip.board();
            let i = board.puzzle.island_at(Pos::new(x, y)).unwrap();
            board.incident[i]
                .iter()
                .map(|&e| counts[e])
                .collect::<Vec<_>>()
        };

        // The 4 in the corner: two neighbours, a double bridge to each
        assert_eq!(degree(Tip::Corner, 0, 0), [2, 2]);

        // The two 1s in the top row: each connects down, not to the other
        let (board, counts) = Tip::Ones.board();
        assert_eq!(edge(&board, (0, 0), (4, 0)).map(|e| counts[e]), Some(0));
        assert_eq!(degree(Tip::Ones, 0, 0).iter().sum::<u8>(), 1);
        assert_eq!(degree(Tip::Ones, 4, 0).iter().sum::<u8>(), 1);

        // The 5: three neighbours, one bridge to each
        assert_eq!(degree(Tip::Count, 2, 0), [1, 1, 1]);

        // The 1 can't reach the 2 on the right across the double bridge
        let (board, counts) = Tip::Block.board();
        let across = edge(&board, (0, 1), (4, 1)).unwrap();
        assert!(board.is_blocked(&counts, across));
        assert_eq!(degree(Tip::Block, 0, 1).iter().sum::<u8>(), 1);

        // The 3 sends a bridge down, out of the row it would close off with the 1 and the 2
        let (board, counts) = Tip::Connect.board();
        assert_eq!(edge(&board, (2, 0), (2, 2)).map(|e| counts[e]), Some(1));
    }
}
