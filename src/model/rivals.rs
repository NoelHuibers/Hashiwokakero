//! Rivals ライバル: players someone chose to measure themselves against. One-sided and private,
//! nobody is asked or told: what they compare is on the public leaderboards anyway.

use serde::{Deserialize, Serialize};

use super::{AllTimeEntry, Stamp, format_duration};

/// A player can mark at most this many rivals.
pub const MAX_RIVALS: usize = 20;

/// How many of the last shared days a [`HeadToHead`] lists.
pub const RECENT_DUELS: usize = 7;

/// How a day went against a rival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Win,
    Loss,
    /// The same time to the millisecond.
    Draw,
}

impl Outcome {
    /// Faster wins: `mine` and `theirs` are ranked times of the same day.
    pub fn of(mine: i64, theirs: i64) -> Self {
        match mine.cmp(&theirs) {
            std::cmp::Ordering::Less => Self::Win,
            std::cmp::Ordering::Greater => Self::Loss,
            std::cmp::Ordering::Equal => Self::Draw,
        }
    }

    /// 勝 win, 負 loss, 分 draw (hikiwake), as on a sumo scoreboard.
    pub fn kanji(self) -> &'static str {
        match self {
            Self::Win => "勝",
            Self::Loss => "負",
            Self::Draw => "分",
        }
    }
}

/// One day both solved, ranked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duel {
    pub day: i64,
    pub mine: i64,
    pub theirs: i64,
}

impl Duel {
    pub fn outcome(&self) -> Outcome {
        Outcome::of(self.mine, self.theirs)
    }
}

/// The record against a rival over every day both have a ranked time, today too once both
/// finished it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadToHead {
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
    /// The last [`RECENT_DUELS`] of them, oldest first.
    pub recent: Vec<Duel>,
}

impl HeadToHead {
    /// The record of `duels`, in the order of their days.
    pub fn of(duels: &[Duel]) -> Self {
        let count = |o| {
            i64::try_from(duels.iter().filter(|d| d.outcome() == o).count()).unwrap_or(i64::MAX)
        };
        Self {
            wins: count(Outcome::Win),
            losses: count(Outcome::Loss),
            draws: count(Outcome::Draw),
            recent: duels[duels.len().saturating_sub(RECENT_DUELS)..].to_vec(),
        }
    }

    pub fn played(&self) -> i64 {
        self.wins + self.losses + self.draws
    }

    /// Who leads: a win if I won more, a loss if they did, a draw when even.
    pub fn lead(&self) -> Outcome {
        Outcome::of(self.losses, self.wins)
    }

    /// "5–3", draws only when there were any: "5–3–1".
    pub fn score(&self) -> String {
        if self.draws > 0 {
            format!("{}–{}–{}", self.wins, self.losses, self.draws)
        } else {
            format!("{}–{}", self.wins, self.losses)
        }
    }
}

/// A rival on the daily page, once the player finished today's daily.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivalDay {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub stamp: Option<Stamp>,
    /// Their ranked time today, `None` while they have none (not played, still playing, given
    /// up or not ranked: only what the public leaderboard shows).
    pub time_ms: Option<i64>,
    pub record: HeadToHead,
}

/// Today's duels against every rival. Only sent once the player finished today's daily, so
/// nothing gives away how fast the others were before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivalsToday {
    /// The player's own ranked time today, `None` if it isn't ranked (given up, flagged).
    pub mine: Option<i64>,
    pub rivals: Vec<RivalDay>,
}

/// What the line of a rival on the daily page says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// "You beat Keiko by 12 s"
    Beat { gap_ms: i64 },
    /// "Tobias was 40 s faster"
    Lost { gap_ms: i64 },
    /// The same time to the millisecond.
    Tie,
    /// "Lena hasn't played yet": no ranked time today.
    Waiting,
    /// My own time isn't ranked: only theirs, without a duel.
    Solved { time_ms: i64 },
}

impl Verdict {
    pub fn of(mine: Option<i64>, theirs: Option<i64>) -> Self {
        match (mine, theirs) {
            (_, None) => Self::Waiting,
            (None, Some(time_ms)) => Self::Solved { time_ms },
            (Some(m), Some(t)) => match Outcome::of(m, t) {
                Outcome::Win => Self::Beat { gap_ms: t - m },
                Outcome::Loss => Self::Lost { gap_ms: m - t },
                Outcome::Draw => Self::Tie,
            },
        }
    }

    /// Duels first, the closest at the top of each kind: wins, ties, losses, solved, waiting.
    pub fn order(self) -> (u8, i64) {
        match self {
            Self::Beat { gap_ms } => (0, gap_ms),
            Self::Tie => (1, 0),
            Self::Lost { gap_ms } => (2, gap_ms),
            Self::Solved { time_ms } => (3, time_ms),
            Self::Waiting => (4, 0),
        }
    }
}

/// A gap between two times, without the unit: whole seconds below a minute ("12", tenths below
/// ten seconds: "4.2"), minutes and seconds above ("1:05"). `true` if it is in minutes.
pub fn format_gap(ms: i64) -> (String, bool) {
    let ms = ms.max(0);
    if ms < 10_000 {
        (format!("{}.{}", ms / 1000, ms % 1000 / 100), false)
    } else if ms < 60_000 {
        ((ms / 1000).to_string(), false)
    } else {
        let s = ms / 1000;
        if s >= 3600 {
            (format_duration(ms), true)
        } else {
            (format!("{}:{:02}", s / 60, s % 60), true)
        }
    }
}

/// A row of the rivals card on the ranks page: the player or one of their rivals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivalRow {
    /// Solved dailies, medals and streaks; `rank` is the place among the rows.
    pub entry: AllTimeEntry,
    /// The player's own row.
    pub me: bool,
    /// Against the player, `None` on their own row.
    pub record: Option<HeadToHead>,
}

/// The player and their rivals compared, most solved first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivalsBoard {
    pub rows: Vec<RivalRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duel(day: i64, mine: i64, theirs: i64) -> Duel {
        Duel { day, mine, theirs }
    }

    #[test]
    fn faster_wins_the_day() {
        assert_eq!(Outcome::of(60_000, 72_000), Outcome::Win);
        assert_eq!(Outcome::of(72_000, 60_000), Outcome::Loss);
        assert_eq!(Outcome::of(60_000, 60_000), Outcome::Draw);
        assert_eq!(duel(1, 1, 2).outcome(), Outcome::Win);
    }

    #[test]
    fn the_record_counts_every_shared_day_and_keeps_the_last_ones() {
        let duels: Vec<Duel> = (0..10)
            .map(|d| match d % 3 {
                0 => duel(d, 50, 60),
                1 => duel(d, 70, 60),
                _ => duel(d, 60, 60),
            })
            .collect();
        let record = HeadToHead::of(&duels);
        assert_eq!((record.wins, record.losses, record.draws), (4, 3, 3));
        assert_eq!(record.played(), 10);
        assert_eq!(record.recent.len(), RECENT_DUELS);
        assert_eq!(record.recent.first().map(|d| d.day), Some(3));
        assert_eq!(record.recent.last().map(|d| d.day), Some(9));
        assert_eq!(record.score(), "4–3–3");
        assert_eq!(record.lead(), Outcome::Win);

        let none = HeadToHead::of(&[]);
        assert_eq!(none, HeadToHead::default());
        assert_eq!(
            (none.score(), none.lead()),
            ("0–0".to_string(), Outcome::Draw)
        );
        let behind = HeadToHead::of(&[duel(1, 90, 60), duel(2, 50, 60), duel(3, 90, 80)]);
        assert_eq!(
            (behind.score(), behind.lead()),
            ("1–2".to_string(), Outcome::Loss)
        );
    }

    #[test]
    fn the_daily_line_compares_ranked_times_only() {
        assert_eq!(
            Verdict::of(Some(60_000), Some(72_000)),
            Verdict::Beat { gap_ms: 12_000 }
        );
        assert_eq!(
            Verdict::of(Some(100_000), Some(60_000)),
            Verdict::Lost { gap_ms: 40_000 }
        );
        assert_eq!(Verdict::of(Some(60_000), Some(60_000)), Verdict::Tie);
        // No time of theirs: not played, still at it, given up or not ranked, all the same
        assert_eq!(Verdict::of(Some(60_000), None), Verdict::Waiting);
        assert_eq!(Verdict::of(None, None), Verdict::Waiting);
        // Mine isn't ranked: their time, no duel
        assert_eq!(
            Verdict::of(None, Some(72_000)),
            Verdict::Solved { time_ms: 72_000 }
        );
    }

    #[test]
    fn duels_come_first_the_closest_on_top() {
        let mut verdicts = vec![
            Verdict::Waiting,
            Verdict::Lost { gap_ms: 40_000 },
            Verdict::Solved { time_ms: 1 },
            Verdict::Beat { gap_ms: 30_000 },
            Verdict::Lost { gap_ms: 2_000 },
            Verdict::Tie,
            Verdict::Beat { gap_ms: 1_000 },
        ];
        verdicts.sort_by_key(|v| v.order());
        assert_eq!(
            verdicts,
            [
                Verdict::Beat { gap_ms: 1_000 },
                Verdict::Beat { gap_ms: 30_000 },
                Verdict::Tie,
                Verdict::Lost { gap_ms: 2_000 },
                Verdict::Lost { gap_ms: 40_000 },
                Verdict::Solved { time_ms: 1 },
                Verdict::Waiting,
            ]
        );
    }

    #[test]
    fn gaps_read_naturally() {
        assert_eq!(format_gap(400), ("0.4".into(), false));
        assert_eq!(format_gap(4_250), ("4.2".into(), false));
        assert_eq!(format_gap(12_900), ("12".into(), false));
        assert_eq!(format_gap(59_999), ("59".into(), false));
        assert_eq!(format_gap(65_000), ("1:05".into(), true));
        assert_eq!(format_gap(3_725_000), ("1:02:05".into(), true));
        assert_eq!(format_gap(-5), ("0.0".into(), false));
    }
}
