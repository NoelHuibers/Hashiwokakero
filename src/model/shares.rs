//! Shared puzzles (`/p/<code>`): a player shares the puzzle they just solved (or built in the
//! workshop) as a short link, others play exactly that puzzle and see everyone who solved it,
//! the sharer's time first as the one to beat.
//!
//! What the server sends the page ([`SharedPuzzlePage`]) and the rules both sides need: what a
//! code looks like, how a result is ranked ([`share_score`]) and what a result has to be to count
//! ([`check_result`]).

use common::{Board, Puzzle};
use serde::{Deserialize, Serialize};

use super::daily::{HINT_PENALTY_MS, UserInfo};
use super::error::ApiError;
use super::stamps::Stamp;

/// Characters of a share code.
pub const SHARE_CODE_LEN: usize = 8;
/// What share codes are made of: lower case letters and digits that can't be mistaken for one
/// another (no 0/o, 1/l/i), 31 of them, so a code has almost 40 random bits. Nothing else, so a
/// code reads out and types like a word.
pub const SHARE_ALPHABET: &[u8; 31] = b"abcdefghjkmnpqrstuvwxyz23456789";
/// Links a player can make in a day.
pub const MAX_SHARES_PER_DAY: i64 = 30;
/// Places the page's board shows (all of the ties of the last one), the viewer's own besides.
pub const SHARE_BOARD_SHOWN: i64 = 50;
/// The longest time a result can have: a week. A tab left open longer isn't a time.
pub const MAX_SHARE_TIME_MS: i64 = 7 * super::MS_PER_DAY;
/// Islands a puzzle needs at least: one alone has no bridge to build.
pub const SHARE_MIN_ISLANDS: usize = 2;

/// Whether `code` can be a share code (the server looks up nothing else).
pub fn is_share_code(code: &str) -> bool {
    code.len() == SHARE_CODE_LEN && code.bytes().all(|b| SHARE_ALPHABET.contains(&b))
}

/// The path of a shared puzzle's page (without the language).
pub fn share_path(code: &str) -> String {
    format!("/p/{code}")
}

/// What a result is ranked by: the time and, as on the daily, 30 s for every hint (on Play hints
/// are free, a board of hints would win otherwise).
pub const fn share_score(time_ms: i64, hints: i64) -> i64 {
    time_ms + hints * HINT_PENALTY_MS
}

/// A time as Play's clock shows it, in whole seconds: `02:13`, `1:02:03` past the hour.
pub fn share_clock(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// A solve of a shared puzzle as the page sends it: the solved board, the time on Play's clock,
/// the moves and the hints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedSolve {
    pub bridges: Vec<u8>,
    pub time_ms: i64,
    pub moves: u32,
    pub hints: u32,
}

/// Whether `solve` solves `board` (whose only solution is `solution`) the way a player can: the
/// board solved, a time within reason, and a move or a hint for every bridge of the solution
/// (every bridge takes one: a click builds one or, the other way round, two).
pub fn check_result(board: &Board, solution: &[u8], solve: &SharedSolve) -> Result<(), ApiError> {
    if solve.bridges.len() != board.edges.len() || solve.bridges.iter().any(|&b| b > 2) {
        return Err(ApiError::WrongPuzzle);
    }
    if solve.bridges != solution || !board.check(&solve.bridges).solved {
        return Err(ApiError::NotSolved);
    }
    let needed = solution.iter().filter(|&&b| b > 0).count();
    let done = usize::try_from(u64::from(solve.moves) + u64::from(solve.hints)).unwrap_or(usize::MAX);
    if !(0..=MAX_SHARE_TIME_MS).contains(&solve.time_ms) || done < needed {
        return Err(ApiError::WrongPuzzle);
    }
    Ok(())
}

/// Who shared a puzzle, and their result (`None`: shared from the workshop, not played).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedBy {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub stamp: Option<Stamp>,
    pub result: Option<SharedEntry>,
}

/// A player's result on a shared puzzle: their first solve, later ones don't change it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedEntry {
    /// Places are shared at the same [`share_score`].
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub stamp: Option<Stamp>,
    pub time_ms: i64,
    pub moves: i64,
    pub hints: i64,
    /// What ranks: [`share_score`].
    pub score_ms: i64,
}

/// Everything the page of a shared puzzle shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedPuzzlePage {
    pub code: String,
    /// The puzzle, never its solution: the hints come from the solvers, like on Play.
    pub puzzle: Puzzle,
    /// `None`: the sharer deleted their account (or is banned), the link works on without them.
    pub sharer: Option<SharedBy>,
    /// The best [`SHARE_BOARD_SHOWN`] places, the fastest first.
    pub board: Vec<SharedEntry>,
    /// The viewer's result, also when it isn't on [`Self::board`].
    pub mine: Option<SharedEntry>,
    /// Everyone who solved it.
    pub solvers: i64,
    pub created_at: i64,
}

/// A link made: its code, and who made it (a guest made for it, the page shows them signed in).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedLink {
    pub code: String,
    pub user: UserInfo,
}

/// A solve sent: the page again, whether it was the player's first (only that one counts), and
/// who they are (a guest made for it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedSubmit {
    pub page: SharedPuzzlePage,
    pub counted: bool,
    pub user: UserInfo,
}

#[cfg(test)]
mod tests {
    use common::Pos;

    use super::*;

    /// Two islands in a row and one below the right one, the solution `[2, 1]`.
    fn board() -> Board {
        let mut puzzle = Puzzle::empty(3, 3);
        puzzle.set(Pos::new(0, 0), 2);
        puzzle.set(Pos::new(2, 0), 3);
        puzzle.set(Pos::new(2, 2), 1);
        Board::new(puzzle)
    }

    fn solve(bridges: Vec<u8>, time_ms: i64, moves: u32, hints: u32) -> SharedSolve {
        SharedSolve {
            bridges,
            time_ms,
            moves,
            hints,
        }
    }

    #[test]
    fn codes_are_short_and_plain() {
        assert!(is_share_code("k7m2xq9a"));
        assert!(!is_share_code("k7m2xq9"));
        assert!(!is_share_code("k7m2xq9ab"));
        // Look-alikes, upper case, anything for a path
        for code in ["k7m2xq9l", "k7m2xq90", "K7M2XQ9A", "k7m2xq_a", "../admin"] {
            assert!(!is_share_code(code), "{code}");
        }
        assert_eq!(share_path("k7m2xq9a"), "/p/k7m2xq9a");
        assert_eq!(SHARE_ALPHABET.len(), 31);
        let mut sorted = SHARE_ALPHABET.to_vec();
        sorted.dedup();
        assert_eq!(sorted.len(), SHARE_ALPHABET.len());
    }

    #[test]
    fn times_read_like_plays_clock() {
        assert_eq!(share_clock(133_999), "02:13");
        assert_eq!(share_clock(0), "00:00");
        assert_eq!(share_clock(3_723_000), "1:02:03");
        assert_eq!(share_clock(-5), "00:00");
    }

    #[test]
    fn hints_cost_thirty_seconds() {
        assert_eq!(share_score(133_000, 0), 133_000);
        assert_eq!(share_score(133_000, 2), 193_000);
    }

    #[test]
    fn only_a_solved_board_played_within_reason_counts() {
        let board = board();
        let solution = [2, 1];
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 1], 61_000, 2, 0)),
            Ok(())
        );
        // A hint builds a bridge too
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 1], 0, 1, 1)),
            Ok(())
        );
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 0], 61_000, 2, 0)),
            Err(ApiError::NotSolved)
        );
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 1, 0], 61_000, 2, 0)),
            Err(ApiError::WrongPuzzle)
        );
        assert_eq!(
            check_result(&board, &solution, &solve(vec![3, 1], 61_000, 2, 0)),
            Err(ApiError::WrongPuzzle)
        );
        // Fewer moves than bridges, a time below zero or of more than a week
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 1], 61_000, 1, 0)),
            Err(ApiError::WrongPuzzle)
        );
        assert_eq!(
            check_result(&board, &solution, &solve(vec![2, 1], -1, 2, 0)),
            Err(ApiError::WrongPuzzle)
        );
        assert_eq!(
            check_result(
                &board,
                &solution,
                &solve(vec![2, 1], MAX_SHARE_TIME_MS + 1, 2, 0)
            ),
            Err(ApiError::WrongPuzzle)
        );
    }
}
