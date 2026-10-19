//! Achievements: hanko stamps for the stamp book on the account page, worked out from a
//! player's dailies (see `server::stamps`).

use common::Difficulty;
use serde::{Deserialize, Serialize};

use super::{daily_difficulty, weekday};

/// A stamp in the stamp book. Listed in the order of the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stamp {
    First,
    Solved10,
    Solved50,
    Solved100,
    Solved365,
    Streak7,
    Streak30,
    Streak100,
    FullWeek,
    Oni,
    Clean,
    EarlyBird,
    Swift,
    Podium,
    Gold,
    Gold10,
    Champion,
    Dojo,
}

impl Stamp {
    pub const ALL: [Self; 18] = [
        Self::First,
        Self::Solved10,
        Self::Solved50,
        Self::Solved100,
        Self::Solved365,
        Self::Streak7,
        Self::Streak30,
        Self::Streak100,
        Self::FullWeek,
        Self::Oni,
        Self::Clean,
        Self::EarlyBird,
        Self::Swift,
        Self::Podium,
        Self::Gold,
        Self::Gold10,
        Self::Champion,
        Self::Dojo,
    ];

    /// Shown next to a name on the leaderboards, the most impressive stamp first. Stamps
    /// everybody on a leaderboard has aren't worth showing.
    pub const PRESTIGE: [Self; 15] = [
        Self::Champion,
        Self::Gold10,
        Self::Streak100,
        Self::Solved365,
        Self::Gold,
        Self::Streak30,
        Self::Solved100,
        Self::Swift,
        Self::Podium,
        Self::FullWeek,
        Self::Oni,
        Self::Streak7,
        Self::Solved50,
        Self::EarlyBird,
        Self::Clean,
    ];

    /// Stored in the `stamps` table.
    pub const fn id(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Solved10 => "solved_10",
            Self::Solved50 => "solved_50",
            Self::Solved100 => "solved_100",
            Self::Solved365 => "solved_365",
            Self::Streak7 => "streak_7",
            Self::Streak30 => "streak_30",
            Self::Streak100 => "streak_100",
            Self::FullWeek => "full_week",
            Self::Oni => "oni",
            Self::Clean => "clean",
            Self::EarlyBird => "early_bird",
            Self::Swift => "swift",
            Self::Podium => "podium",
            Self::Gold => "gold",
            Self::Gold10 => "gold_10",
            Self::Champion => "champion",
            Self::Dojo => "dojo",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// The kanji on the stamp.
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::First => "初",
            Self::Solved10 | Self::Solved50 | Self::Solved100 | Self::Solved365 => "百",
            Self::Streak7 | Self::Streak30 | Self::Streak100 => "連",
            Self::FullWeek => "週",
            Self::Oni => "鬼",
            Self::Clean => "清",
            Self::EarlyBird => "早",
            Self::Swift => "速",
            Self::Podium => "銅",
            Self::Gold | Self::Gold10 => "金",
            Self::Champion => "王",
            Self::Dojo => "道",
        }
    }

    /// The number of the tier (`10` solved, `7` days), for stamps that share a kanji.
    pub const fn tier(self) -> Option<i64> {
        match self {
            Self::Solved10 | Self::Gold10 => Some(10),
            Self::Solved50 => Some(50),
            Self::Solved100 | Self::Streak100 => Some(100),
            Self::Solved365 => Some(365),
            Self::Streak7 => Some(7),
            Self::Streak30 => Some(30),
            _ => None,
        }
    }
}

/// How many ranked finishers a daily needs before its fastest tenth earns [`Stamp::Swift`].
pub const SWIFT_MIN_FINISHERS: i64 = 10;
/// Dailies without hints or checks for [`Stamp::Clean`].
pub const CLEAN_TARGET: i64 = 10;

/// One ranked daily of a player, as the stamps see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Solve {
    pub day: i64,
    pub hints: i64,
    pub checks: i64,
    /// Place among the day's ranked finishers, 1 = fastest.
    pub place: i64,
    pub finishers: i64,
    /// The first of the day to finish (not necessarily the fastest).
    pub first_finish: bool,
}

/// What a player has done, everything a stamp depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Sorted by day.
    pub solves: Vec<Solve>,
    /// Months the player finished first, see [`super::month_of`].
    pub champion_months: i64,
    /// Has a scored solver on the SAT Dojo board.
    pub dojo: bool,
}

/// A stamp and how far along the player is, for the stamp book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampProgress {
    pub stamp: Stamp,
    /// When it was earned (ms), `None` while it's still to get.
    pub earned_at: Option<i64>,
    pub progress: i64,
    pub target: i64,
    /// Players who have it, see [`share_percent`].
    #[serde(default)]
    pub holders: i64,
}

impl Record {
    /// Progress towards every stamp, in the order of [`Stamp::ALL`]: `(stamp, progress, target)`,
    /// earned when `progress >= target`. Medals and the fastest tenth only count once the day is
    /// over (`today` is still running), the ranking can change until then.
    pub fn progress(&self, today: i64) -> Vec<(Stamp, i64, i64)> {
        let over = || self.solves.iter().filter(|s| s.day < today);
        let solved = count(self.solves.len());
        let best_streak = best_streak(&self.solves);
        let golds = count(over().filter(|s| s.place == 1).count());
        let podiums = count(over().filter(|s| s.place <= 3).count());
        let swift =
            over().any(|s| s.finishers >= SWIFT_MIN_FINISHERS && s.place * 10 <= s.finishers);
        let clean = count(
            self.solves
                .iter()
                .filter(|s| s.hints == 0 && s.checks == 0)
                .count(),
        );
        let oni = self
            .solves
            .iter()
            .any(|s| daily_difficulty(s.day) == Difficulty::Oni);
        let early = self.solves.iter().any(|s| s.first_finish);
        Stamp::ALL
            .into_iter()
            .map(|stamp| {
                let (progress, target) = match stamp {
                    Stamp::First => (solved, 1),
                    Stamp::Solved10 | Stamp::Solved50 | Stamp::Solved100 | Stamp::Solved365 => {
                        (solved, stamp.tier().unwrap_or_default())
                    }
                    Stamp::Streak7 | Stamp::Streak30 | Stamp::Streak100 => {
                        (best_streak, stamp.tier().unwrap_or_default())
                    }
                    Stamp::FullWeek => (best_week(&self.solves), 7),
                    Stamp::Oni => (i64::from(oni), 1),
                    Stamp::Clean => (clean, CLEAN_TARGET),
                    Stamp::EarlyBird => (i64::from(early), 1),
                    Stamp::Swift => (i64::from(swift), 1),
                    Stamp::Podium => (podiums, 1),
                    Stamp::Gold => (golds, 1),
                    Stamp::Gold10 => (golds, 10),
                    Stamp::Champion => (self.champion_months, 1),
                    Stamp::Dojo => (i64::from(self.dojo), 1),
                };
                (stamp, progress.min(target), target)
            })
            .collect()
    }

    /// The stamps earned.
    pub fn earned(&self, today: i64) -> Vec<Stamp> {
        self.progress(today)
            .into_iter()
            .filter(|(_, progress, target)| progress >= target)
            .map(|(stamp, ..)| stamp)
            .collect()
    }
}

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn best_streak(solves: &[Solve]) -> i64 {
    let days: Vec<i64> = solves.iter().map(|s| s.day).collect();
    super::streaks(&days, 0).1
}

/// Most dailies solved in one Monday to Sunday week, 7 means every difficulty in a row.
fn best_week(solves: &[Solve]) -> i64 {
    let mut best = 0;
    let mut week = None;
    let mut run = 0;
    for s in solves {
        let monday = s.day - count(weekday(s.day));
        run = if week == Some(monday) { run + 1 } else { 1 };
        week = Some(monday);
        best = best.max(run);
    }
    best
}

/// The share of players who have a stamp, as a whole percent: `"12"`, `"<1"` when only a few do.
/// `None` without players.
pub fn share_percent(holders: i64, players: i64) -> Option<String> {
    if players <= 0 {
        return None;
    }
    let holders = holders.clamp(0, players);
    // Rounded to the nearest percent
    let percent = (holders * 200 + players) / (players * 2);
    Some(if percent == 0 && holders > 0 {
        "<1".into()
    } else {
        percent.to_string()
    })
}

/// The most impressive of some stamps, for a name on a leaderboard.
pub fn top_stamp(stamps: &[Stamp]) -> Option<Stamp> {
    Stamp::PRESTIGE.into_iter().find(|s| stamps.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LAUNCH_DAY, weekday};

    fn solve(day: i64, place: i64, finishers: i64) -> Solve {
        Solve {
            day,
            hints: 0,
            checks: 1,
            place,
            finishers,
            first_finish: false,
        }
    }

    #[test]
    fn ids_round_trip() {
        for stamp in Stamp::ALL {
            assert_eq!(Stamp::from_id(stamp.id()), Some(stamp));
        }
        assert_eq!(Stamp::from_id("nope"), None);
    }

    #[test]
    fn nothing_without_solves() {
        assert_eq!(Record::default().earned(LAUNCH_DAY), []);
    }

    #[test]
    fn streaks_weeks_and_counts() {
        let monday = (LAUNCH_DAY..LAUNCH_DAY + 7)
            .find(|&d| weekday(d) == 0)
            .unwrap();
        // Monday to Sunday of one week, then a gap and three more days
        let mut solves: Vec<Solve> = (monday..monday + 7).map(|d| solve(d, 5, 9)).collect();
        solves.extend((monday + 9..monday + 12).map(|d| solve(d, 5, 9)));
        let record = Record {
            solves,
            ..Record::default()
        };
        let today = monday + 30;
        let earned = record.earned(today);
        for stamp in [
            Stamp::First,
            Stamp::Solved10,
            Stamp::Streak7,
            Stamp::FullWeek,
            Stamp::Oni,
        ] {
            assert!(earned.contains(&stamp), "{stamp:?}");
        }
        for stamp in [Stamp::Solved50, Stamp::Streak30, Stamp::Clean, Stamp::Gold] {
            assert!(!earned.contains(&stamp), "{stamp:?}");
        }
        let progress = record.progress(today);
        let streak30 = progress.iter().find(|p| p.0 == Stamp::Streak30).unwrap();
        assert_eq!((streak30.1, streak30.2), (7, 30));
    }

    #[test]
    fn medals_only_count_once_the_day_is_over() {
        let day = LAUNCH_DAY + 10;
        let record = Record {
            solves: vec![solve(day, 1, 12)],
            ..Record::default()
        };
        let running = record.earned(day);
        assert!(!running.contains(&Stamp::Gold) && !running.contains(&Stamp::Swift));
        let over = record.earned(day + 1);
        for stamp in [Stamp::Gold, Stamp::Podium, Stamp::Swift] {
            assert!(over.contains(&stamp), "{stamp:?}");
        }
        // The fastest tenth needs enough players
        let small = Record {
            solves: vec![solve(day, 1, 9)],
            ..Record::default()
        };
        assert!(!small.earned(day + 1).contains(&Stamp::Swift));
    }

    #[test]
    fn shares_of_players() {
        assert_eq!(share_percent(1, 0), None);
        assert_eq!(share_percent(0, 7).as_deref(), Some("0"));
        assert_eq!(share_percent(1, 3).as_deref(), Some("33"));
        assert_eq!(share_percent(1, 8).as_deref(), Some("13"));
        assert_eq!(share_percent(1, 500).as_deref(), Some("<1"));
        assert_eq!(share_percent(5, 5).as_deref(), Some("100"));
    }

    #[test]
    fn top_stamp_skips_the_basics() {
        assert_eq!(top_stamp(&[Stamp::First, Stamp::Solved10]), None);
        assert_eq!(
            top_stamp(&[Stamp::First, Stamp::Podium, Stamp::Champion]),
            Some(Stamp::Champion)
        );
    }
}
