//! Group puzzles (課題 kadai): a puzzle a member made in 作る Create, posted to one of their
//! groups as a challenge for a week. The others play it like a daily, timed by the server with
//! the same penalties, and are ranked within the group; the author watches how they do.
//!
//! What the server sends the group page ([`GroupPuzzles`]) and the puzzle's page
//! ([`PuzzlePage`]), and the rules both sides need: what can be posted, how long it stays open,
//! and the numbers of the author's view ([`PuzzleStats`]).

use common::Puzzle;
use serde::{Deserialize, Serialize};

use super::MS_PER_DAY;
use super::daily::AttemptInfo;
use super::error::ApiError;
use super::groups::TodayEntry;
use super::replay::MarkKind;

/// How long a group puzzle is open to play, from posting.
pub const OPEN_DAYS: i64 = 7;
/// The widest and tallest a group puzzle can be: the size of the hardest dailies and then some,
/// the server proves every posted puzzle unique with the SAT solver.
pub const MAX_SIDE: u8 = 20;
/// Islands a puzzle needs at least: one alone has no bridge to build.
pub const MIN_ISLANDS: usize = 2;
/// Open puzzles a member can have in a group at once.
pub const MAX_OPEN: usize = 3;
/// Characters of a puzzle's title at most (after trimming).
pub const TITLE_MAX: usize = 40;
/// Closed puzzles the group page shows, the newest first.
pub const ARCHIVE_SHOWN: usize = 8;

/// When a puzzle posted at `created_at` closes.
pub const fn closes_at(created_at: i64) -> i64 {
    created_at + OPEN_DAYS * MS_PER_DAY
}

/// A puzzle's title as it is stored: trimmed, inner runs of white space as one space, `None` for
/// none. An error if it is too long or has control characters.
pub fn clean_title(title: &str) -> Result<Option<String>, ApiError> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Ok(None);
    }
    if title.chars().count() > TITLE_MAX || title.chars().any(char::is_control) {
        return Err(ApiError::PuzzleTitleInvalid);
    }
    Ok(Some(title))
}

/// Whether a puzzle can be posted, before the solver looks at it: at most [`MAX_SIDE`] across
/// and down, at least [`MIN_ISLANDS`] islands.
pub fn check_size(puzzle: &Puzzle) -> Result<(), ApiError> {
    if puzzle.width > MAX_SIDE || puzzle.height > MAX_SIDE {
        return Err(ApiError::PuzzleTooBig);
    }
    if puzzle.islands.len() < MIN_ISLANDS {
        return Err(ApiError::WrongPuzzle);
    }
    Ok(())
}

/// How long until a puzzle closes, for "5 days left".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeLeft {
    Days(i64),
    Hours(i64),
    Minutes(i64),
    Closed,
}

/// The time left until `closes_at`, rounded down to whole days, hours or (at least one) minutes.
pub fn time_left(now: i64, closes_at: i64) -> TimeLeft {
    const HOUR: i64 = MS_PER_DAY / 24;
    let left = closes_at - now;
    match left {
        ..=0 => TimeLeft::Closed,
        l if l >= MS_PER_DAY => TimeLeft::Days(l / MS_PER_DAY),
        l if l >= HOUR => TimeLeft::Hours(l / HOUR),
        l => TimeLeft::Minutes((l / 60_000).max(1)),
    }
}

/// Who made a puzzle, as the group sees them. `None` in its place: the author left the group or
/// deleted their account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzleAuthor {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub colour: u8,
}

/// Where the viewer stands with a puzzle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MyPuzzle {
    /// They made it: they can't play it, they watch.
    Author,
    /// Not started.
    Fresh,
    /// Started, not finished.
    Playing,
    /// Solved: the time with penalties, the place among the group (`None`: not ranked).
    Solved { time_ms: i64, rank: Option<i64> },
    /// Gave up.
    GaveUp,
}

impl MyPuzzle {
    /// Done with it: the times of the others are no spoiler any more.
    pub fn done(self) -> bool {
        matches!(self, Self::Author | Self::Solved { .. } | Self::GaveUp)
    }
}

/// A player of a puzzle (a member but the author) as a pip in their colour, filled once they
/// solved it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pip {
    pub colour: u8,
    pub solved: bool,
}

/// A puzzle in the group page's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzleSummary {
    pub id: i64,
    pub group_id: i64,
    pub title: Option<String>,
    pub author: Option<PuzzleAuthor>,
    pub width: u8,
    pub height: u8,
    pub islands: u32,
    pub created_at: i64,
    pub closes_at: i64,
    /// Still open to play, on the server's clock.
    pub open: bool,
    /// The members who can play it (everyone but the author), in the group's order.
    pub pips: Vec<Pip>,
    pub mine: MyPuzzle,
    /// Closed: the puzzle and its solution, the final board of the archive.
    pub board: Option<FinalBoard>,
    /// Closed: who was fastest, and their time.
    pub winner: Option<(String, i64)>,
}

impl PuzzleSummary {
    /// Members who solved it.
    pub fn solved(&self) -> usize {
        self.pips.iter().filter(|p| p.solved).count()
    }

    /// Members who can play it.
    pub fn players(&self) -> usize {
        self.pips.len()
    }
}

/// A closed puzzle with its solution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalBoard {
    pub puzzle: Puzzle,
    pub solution: Vec<u8>,
}

/// The bridge that took the most help: hints that fixed it, and checks that found it wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardest {
    pub edge: usize,
    pub hints: i64,
    pub wrong: i64,
}

/// How everyone did, for the author (and everyone once it's closed): the "your puzzle" view and
/// the applause after it closed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzleStats {
    pub players: i64,
    pub solved: i64,
    pub gave_up: i64,
    /// Started and not finished (yet).
    pub playing: i64,
    pub fastest_ms: Option<i64>,
    pub median_ms: Option<i64>,
    pub hints: i64,
    pub checks: i64,
    pub hardest: Option<Hardest>,
}

/// A hint or a check taken during a group puzzle, as stored with the attempt: when (ms after the
/// start) and the bridges it was about, the one a hint fixed or the ones a check found wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Help {
    pub t: u32,
    pub kind: MarkKind,
    pub edges: Vec<u16>,
}

/// The bridge most help went to: hints count, and so do checks that found it wrong. Ties go to
/// the bridge with more hints, then the first. `None` if nobody needed help.
pub fn hardest(helps: &[Help]) -> Option<Hardest> {
    let mut counts: Vec<Hardest> = Vec::new();
    for help in helps {
        let hint = match help.kind {
            MarkKind::Hint => true,
            MarkKind::Check => false,
            // A help of a newer server: not counted
            MarkKind::Unknown => continue,
        };
        for &edge in &help.edges {
            let edge = usize::from(edge);
            let i = if let Some(i) = counts.iter().position(|h| h.edge == edge) {
                i
            } else {
                counts.push(Hardest {
                    edge,
                    hints: 0,
                    wrong: 0,
                });
                counts.len() - 1
            };
            if hint {
                counts[i].hints += 1;
            } else {
                counts[i].wrong += 1;
            }
        }
    }
    counts.into_iter().max_by(|a, b| {
        (a.hints + a.wrong, a.hints)
            .cmp(&(b.hints + b.wrong, b.hints))
            .then(b.edge.cmp(&a.edge))
    })
}

/// The middle of some times (the lower one of the two middle ones), `None` for none.
pub fn median(times: &[i64]) -> Option<i64> {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len().saturating_sub(1) / 2).copied()
}

/// Everything the group page shows of its puzzles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPuzzles {
    /// Open ones, the ones closing first first.
    pub open: Vec<PuzzleSummary>,
    /// Closed ones, the newest first, at most [`ARCHIVE_SHOWN`].
    pub closed: Vec<PuzzleSummary>,
    /// The viewer's own puzzles that closed since they last looked: the applause.
    pub kudos: Vec<Kudos>,
    /// How many more the viewer can post to this group now (of [`MAX_OPEN`]).
    pub can_post: usize,
    pub server_now: i64,
}

/// The applause for an author when their puzzle closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kudos {
    pub puzzle: PuzzleSummary,
    pub stats: PuzzleStats,
}

/// Everything the page of a group puzzle shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzlePage {
    pub group_id: i64,
    pub group_name: String,
    /// The viewer.
    pub me: i64,
    pub summary: PuzzleSummary,
    /// The puzzle: once the viewer started it (so nobody studies it before the clock runs), for
    /// the author, and for everyone once it's closed.
    pub puzzle: Option<Puzzle>,
    /// The solution: for the author, and for everyone once it's closed.
    pub solution: Option<Vec<u8>>,
    pub attempt: Option<AttemptInfo>,
    /// The members' results, spoiler safe like the dailies' (`model::spoiler_safe`): times only
    /// once the viewer is done with it, or it's closed. The author isn't on it.
    pub board: Vec<TodayEntry>,
    /// The board has times.
    pub board_open: bool,
    /// How everyone did, for the author and once it's closed.
    pub stats: Option<PuzzleStats>,
    /// Server time when the response was made, to correct the browser's clock.
    pub server_now: i64,
}

impl PuzzlePage {
    /// The viewer's place on the board, if ranked.
    pub fn my_rank(&self) -> Option<i64> {
        match self.summary.mine {
            MyPuzzle::Solved { rank, .. } => rank,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use common::Pos;

    use super::*;

    #[test]
    fn titles_are_trimmed_optional_and_limited() {
        assert_eq!(
            clean_title("  The   Spiral \n"),
            Ok(Some("The Spiral".into()))
        );
        assert_eq!(clean_title(""), Ok(None));
        assert_eq!(clean_title("   "), Ok(None));
        let long = "橋".repeat(TITLE_MAX);
        assert_eq!(clean_title(&long), Ok(Some(long.clone())));
        assert_eq!(
            clean_title(&format!("{long}x")),
            Err(ApiError::PuzzleTitleInvalid)
        );
        assert_eq!(clean_title("a\u{7}b"), Err(ApiError::PuzzleTitleInvalid));
    }

    #[test]
    fn big_or_empty_puzzles_cant_be_posted() {
        let mut puzzle = Puzzle::empty(MAX_SIDE, MAX_SIDE);
        assert_eq!(check_size(&puzzle), Err(ApiError::WrongPuzzle));
        puzzle.set(Pos::new(0, 0), 1);
        assert_eq!(check_size(&puzzle), Err(ApiError::WrongPuzzle));
        puzzle.set(Pos::new(3, 0), 1);
        assert_eq!(check_size(&puzzle), Ok(()));
        let mut wide = Puzzle::empty(MAX_SIDE + 1, 5);
        wide.set(Pos::new(0, 0), 1);
        wide.set(Pos::new(3, 0), 1);
        assert_eq!(check_size(&wide), Err(ApiError::PuzzleTooBig));
        let mut tall = Puzzle::empty(5, MAX_SIDE + 1);
        tall.set(Pos::new(0, 0), 1);
        tall.set(Pos::new(3, 0), 1);
        assert_eq!(check_size(&tall), Err(ApiError::PuzzleTooBig));
    }

    #[test]
    fn a_week_counts_down_in_days_then_hours_then_minutes() {
        let posted = 1_000_000;
        let closes = closes_at(posted);
        assert_eq!(closes - posted, 7 * MS_PER_DAY);
        assert_eq!(time_left(posted, closes), TimeLeft::Days(7));
        assert_eq!(time_left(posted + 1, closes), TimeLeft::Days(6));
        assert_eq!(time_left(closes - MS_PER_DAY, closes), TimeLeft::Days(1));
        assert_eq!(
            time_left(closes - MS_PER_DAY + 1, closes),
            TimeLeft::Hours(23)
        );
        assert_eq!(time_left(closes - 3_600_000, closes), TimeLeft::Hours(1));
        assert_eq!(time_left(closes - 3_599_999, closes), TimeLeft::Minutes(59));
        assert_eq!(time_left(closes - 1, closes), TimeLeft::Minutes(1));
        assert_eq!(time_left(closes, closes), TimeLeft::Closed);
        assert_eq!(time_left(closes + 5, closes), TimeLeft::Closed);
    }

    fn help(kind: MarkKind, edges: &[u16]) -> Help {
        Help {
            t: 0,
            kind,
            edges: edges.to_vec(),
        }
    }

    /// The hardest moment: the bridge hints went to most, checks finding it wrong count too.
    #[test]
    fn the_hardest_bridge_took_the_most_help() {
        assert_eq!(hardest(&[]), None);
        // A check that found nothing wrong is no help to any bridge
        assert_eq!(hardest(&[help(MarkKind::Check, &[])]), None);
        let helps = [
            help(MarkKind::Hint, &[4]),
            help(MarkKind::Check, &[2, 4]),
            help(MarkKind::Hint, &[2]),
            help(MarkKind::Hint, &[7]),
            help(MarkKind::Hint, &[4]),
        ];
        assert_eq!(
            hardest(&helps),
            Some(Hardest {
                edge: 4,
                hints: 2,
                wrong: 1
            })
        );
        // As much help: more hints wins, then the first bridge
        let tie = [help(MarkKind::Check, &[1, 3]), help(MarkKind::Hint, &[3])];
        assert_eq!(hardest(&tie).map(|h| h.edge), Some(3));
        let tie = [help(MarkKind::Hint, &[5]), help(MarkKind::Hint, &[2])];
        assert_eq!(hardest(&tie).map(|h| h.edge), Some(2));
        // A newer server's help counts for no bridge
        assert_eq!(hardest(&[help(MarkKind::Unknown, &[6])]), None);
        let mut newer = helps.to_vec();
        newer.push(help(MarkKind::Unknown, &[2, 2, 2]));
        assert_eq!(hardest(&newer), hardest(&helps));
    }

    #[test]
    fn the_median_is_the_middle_time() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[7]), Some(7));
        assert_eq!(median(&[9, 1, 5]), Some(5));
        assert_eq!(median(&[4, 1, 3, 2]), Some(2));
    }

    #[test]
    fn done_means_the_times_are_no_spoiler() {
        assert!(MyPuzzle::Author.done());
        assert!(MyPuzzle::GaveUp.done());
        assert!(
            MyPuzzle::Solved {
                time_ms: 1,
                rank: None
            }
            .done()
        );
        assert!(!MyPuzzle::Fresh.done());
        assert!(!MyPuzzle::Playing.done());
    }
}
