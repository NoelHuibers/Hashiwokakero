//! Fair play: the move log of a daily and the checks that keep the leaderboard human.

use common::{Board, Puzzle};
use serde::{Deserialize, Serialize};

/// One change of a bridge during a daily: `t` ms after the start, `edge` set to `count`. `tap`
/// marks the player's own click, as opposed to a hint, undo, redo or restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveEvent {
    pub t: u32,
    pub edge: u16,
    pub count: u8,
    pub tap: bool,
}

/// Why a daily result isn't ranked. The player still sees their time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Flag {
    /// The move log is missing or doesn't lead to the submitted board.
    Log,
    /// Faster than anyone can place the bridges.
    TooFast,
    /// Clicks too close together, over and over.
    Pace,
    /// The same device started this daily with another account first.
    SecondAccount,
    /// Taken off the board after a review.
    Review,
    /// The player is banned: all their results are off the boards until the ban is lifted.
    Banned,
}

impl Flag {
    pub const ALL: [Self; 6] = [
        Self::Log,
        Self::TooFast,
        Self::Pace,
        Self::SecondAccount,
        Self::Review,
        Self::Banned,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::TooFast => "too_fast",
            Self::Pace => "pace",
            Self::SecondAccount => "second_account",
            Self::Review => "review",
            Self::Banned => "banned",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }
}

/// The fastest a person places a bridge, on average over a whole puzzle.
pub const MIN_MS_PER_BRIDGE: i64 = 250;
/// Clicks on different bridges closer together than this are suspicious...
pub const MIN_TAP_GAP_MS: u32 = 70;
/// ...when more than this share of all clicks are like that.
pub const MAX_FAST_TAP_SHARE: f64 = 0.1;
/// Server and browser clocks differ a little, and requests take time.
pub const CLOCK_SLACK_MS: i64 = 15_000;
pub const MAX_LOG_EVENTS: usize = 20_000;

/// Checks the move log of a finished daily.
///
/// It has to lead from the empty board to the submitted one, fit the time the server measured
/// (`elapsed_ms`, without penalties) and be humanly possible. Hints placed some bridges for
/// free, the floor accounts for them.
pub fn verify_log(
    board: &Board,
    solution: &[u8],
    bridges: &[u8],
    log: &[MoveEvent],
    elapsed_ms: i64,
    hints: i64,
) -> Result<(), Flag> {
    if log.is_empty() || log.len() > MAX_LOG_EVENTS {
        return Err(Flag::Log);
    }
    let mut state = board.empty_bridges();
    let mut last = 0;
    for e in log {
        let slot = state.get_mut(usize::from(e.edge)).ok_or(Flag::Log)?;
        if e.count > 2 || e.t < last || i64::from(e.t) > elapsed_ms + CLOCK_SLACK_MS {
            return Err(Flag::Log);
        }
        *slot = e.count;
        last = e.t;
    }
    if state != bridges {
        return Err(Flag::Log);
    }
    // Every bridge of the solution takes a click, minus what hints placed (up to two each)
    let needed = solution.iter().map(|&b| i64::from(b)).sum::<i64>() - 2 * hints;
    if elapsed_ms < needed.max(0) * MIN_MS_PER_BRIDGE {
        return Err(Flag::TooFast);
    }
    let taps: Vec<&MoveEvent> = log.iter().filter(|e| e.tap).collect();
    let fast = taps
        .windows(2)
        .filter(|w| w[0].edge != w[1].edge && w[1].t - w[0].t < MIN_TAP_GAP_MS)
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "click counts are far below 2^52"
    )]
    let too_many = fast > 3 && fast as f64 > MAX_FAST_TAP_SHARE * taps.len() as f64;
    if too_many {
        return Err(Flag::Pace);
    }
    Ok(())
}

/// A puzzle in a form that is the same for all its rotations and mirror images, to recognise
/// today's daily however it is turned.
pub fn canonical(puzzle: &Puzzle) -> (u8, u8, Vec<(u8, u8, u8)>) {
    let (w, h) = (puzzle.width, puzzle.height);
    (0..8)
        .map(|t| {
            let transpose = t & 4 != 0;
            let (tw, th) = if transpose { (h, w) } else { (w, h) };
            let mut islands: Vec<(u8, u8, u8)> = puzzle
                .islands
                .iter()
                .map(|i| {
                    let (mut x, mut y) = (i.pos.x, i.pos.y);
                    if t & 1 != 0 {
                        x = w - 1 - x;
                    }
                    if t & 2 != 0 {
                        y = h - 1 - y;
                    }
                    if transpose {
                        (x, y) = (y, x);
                    }
                    (y, x, i.required)
                })
                .collect();
            islands.sort_unstable();
            (tw, th, islands)
        })
        .min()
        .unwrap_or((w, h, Vec::new()))
}

/// A move log in numbers, to spot programs at a glance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSummary {
    pub events: u32,
    pub taps: u32,
    /// From the first to the last change.
    pub span_ms: u32,
    /// Between two clicks on different bridges.
    pub fastest_gap_ms: Option<u32>,
    pub median_gap_ms: Option<u32>,
}

pub fn summarize_log(log: &[MoveEvent]) -> LogSummary {
    let taps: Vec<&MoveEvent> = log.iter().filter(|e| e.tap).collect();
    let mut gaps: Vec<u32> = taps
        .windows(2)
        .filter(|w| w[0].edge != w[1].edge)
        .map(|w| w[1].t.saturating_sub(w[0].t))
        .collect();
    gaps.sort_unstable();
    LogSummary {
        events: u32::try_from(log.len()).unwrap_or(u32::MAX),
        taps: u32::try_from(taps.len()).unwrap_or(u32::MAX),
        span_ms: match (log.first(), log.last()) {
            (Some(a), Some(b)) => b.t.saturating_sub(a.t),
            _ => 0,
        },
        fastest_gap_ms: gaps.first().copied(),
        median_gap_ms: gaps.get(gaps.len() / 2).copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two islands joined by a double bridge and a third joined by one: 2 = 2 - 1, 3 bridges.
    fn small() -> (Board, Vec<u8>) {
        let mut puzzle = Puzzle::empty(5, 5);
        puzzle.set(common::Pos::new(0, 0), 2);
        puzzle.set(common::Pos::new(4, 0), 3);
        puzzle.set(common::Pos::new(4, 4), 1);
        let board = Board::new(puzzle);
        let mut solution = board.empty_bridges();
        for (e, edge) in board.edges.iter().enumerate() {
            solution[e] = if edge.a == 0 { 2 } else { 1 };
        }
        (board, solution)
    }

    fn tap(t: u32, edge: usize, count: u8) -> MoveEvent {
        MoveEvent {
            t,
            edge: u16::try_from(edge).unwrap(),
            count,
            tap: true,
        }
    }

    #[test]
    fn honest_logs_pass() {
        let (board, solution) = small();
        assert_eq!(board.edges.len(), 2);
        let log = [tap(900, 0, 1), tap(1300, 0, 2), tap(2400, 1, 1)];
        assert_eq!(
            verify_log(&board, &solution, &solution, &log, 3000, 0),
            Ok(())
        );
        // A hint placed one bridge (not a tap), the floor is lower
        let hinted = [
            MoveEvent {
                tap: false,
                ..tap(200, 0, 2)
            },
            tap(400, 1, 1),
        ];
        assert_eq!(
            verify_log(&board, &solution, &solution, &hinted, 500, 1),
            Ok(())
        );
    }

    #[test]
    fn bad_logs_are_flagged() {
        let (board, solution) = small();
        let log = [tap(900, 0, 1), tap(1300, 0, 2), tap(2400, 1, 1)];
        // No log, or one that doesn't end at the submitted board
        assert_eq!(
            verify_log(&board, &solution, &solution, &[], 3000, 0),
            Err(Flag::Log)
        );
        assert_eq!(
            verify_log(&board, &solution, &solution, &log[..2], 3000, 0),
            Err(Flag::Log)
        );
        // Time going backwards, or far beyond what the server measured
        let backwards = [tap(900, 0, 1), tap(800, 0, 2), tap(2400, 1, 1)];
        assert_eq!(
            verify_log(&board, &solution, &solution, &backwards, 3000, 0),
            Err(Flag::Log)
        );
        let late = [tap(900, 0, 1), tap(1300, 0, 2), tap(40_000, 1, 1)];
        assert_eq!(
            verify_log(&board, &solution, &solution, &late, 3000, 0),
            Err(Flag::Log)
        );
        // Three bridges in half a second
        let quick = [tap(100, 0, 1), tap(300, 0, 2), tap(500, 1, 1)];
        assert_eq!(
            verify_log(&board, &solution, &solution, &quick, 600, 0),
            Err(Flag::TooFast)
        );
        // Edges that don't exist
        let bogus = [tap(900, 7, 1)];
        assert_eq!(
            verify_log(&board, &solution, &solution, &bogus, 3000, 0),
            Err(Flag::Log)
        );
    }

    #[test]
    fn machine_pace_is_flagged() {
        // A big board solved click after click, 20 ms apart on alternating edges
        let mut puzzle = Puzzle::empty(9, 1);
        for x in 0..9 {
            puzzle.set(common::Pos::new(x, 0), if x == 0 || x == 8 { 1 } else { 2 });
        }
        let board = Board::new(puzzle);
        let solution = vec![1; board.edges.len()];
        let log: Vec<MoveEvent> = (0..board.edges.len())
            .map(|e| tap(20 * u32::try_from(e).unwrap() + 5000, e, 1))
            .collect();
        assert_eq!(
            verify_log(&board, &solution, &solution, &log, 6000, 0),
            Err(Flag::Pace)
        );
        let human: Vec<MoveEvent> = (0..board.edges.len())
            .map(|e| tap(700 * u32::try_from(e).unwrap() + 500, e, 1))
            .collect();
        assert_eq!(
            verify_log(&board, &solution, &solution, &human, 7000, 0),
            Ok(())
        );
    }

    #[test]
    fn log_summaries() {
        let log = [
            tap(1000, 0, 1),
            tap(1500, 0, 2),
            tap(1800, 1, 1),
            MoveEvent {
                tap: false,
                ..tap(1900, 2, 1)
            },
            tap(4000, 3, 1),
        ];
        let s = summarize_log(&log);
        assert_eq!((s.events, s.taps, s.span_ms), (5, 4, 3000));
        // Gaps between different bridges: 300 (0→1) and 2200 (1→3)
        assert_eq!((s.fastest_gap_ms, s.median_gap_ms), (Some(300), Some(2200)));
        assert_eq!(summarize_log(&[]), LogSummary::default());
    }

    #[test]
    fn canonical_ignores_rotation_and_mirroring() {
        let mut puzzle = Puzzle::empty(5, 3);
        puzzle.set(common::Pos::new(0, 0), 2);
        puzzle.set(common::Pos::new(4, 0), 3);
        puzzle.set(common::Pos::new(4, 2), 1);
        // Rotated by 90° (width and height swap) and mirrored
        let mut turned = Puzzle::empty(3, 5);
        turned.set(common::Pos::new(2, 0), 2);
        turned.set(common::Pos::new(2, 4), 3);
        turned.set(common::Pos::new(0, 4), 1);
        let mut mirrored = Puzzle::empty(5, 3);
        mirrored.set(common::Pos::new(4, 0), 2);
        mirrored.set(common::Pos::new(0, 0), 3);
        mirrored.set(common::Pos::new(0, 2), 1);
        assert_eq!(canonical(&puzzle), canonical(&turned));
        assert_eq!(canonical(&puzzle), canonical(&mirrored));
        let mut other = mirrored.clone();
        other.set(common::Pos::new(0, 2), 2);
        assert_ne!(canonical(&puzzle), canonical(&other));
    }
}
