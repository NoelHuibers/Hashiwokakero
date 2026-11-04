//! Users and the daily puzzle: attempts, results, leaderboards and statistics.

use common::{Difficulty, Puzzle};
use serde::{Deserialize, Serialize};

use super::moves::Flag;
use super::stamps::{Stamp, StampProgress};

/// Historical daily and group-puzzle price for one hint.
pub const HINT_PENALTY_MS: i64 = 30_000;
/// Historical daily and group-puzzle price for one check.
pub const CHECK_PENALTY_MS: i64 = 10_000;
/// Difficulty-scaled help starts with the November daily season (2026-11-01 UTC).
pub const SCALED_HELP_DAY: i64 = 20_758;

/// Prices for one use of help. Historical dailies and group puzzles keep the old prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpPenalties {
    pub check_ms: i64,
    pub hint_ms: i64,
}

impl Default for HelpPenalties {
    fn default() -> Self {
        Self {
            check_ms: CHECK_PENALTY_MS,
            hint_ms: HINT_PENALTY_MS,
        }
    }
}

impl HelpPenalties {
    pub fn daily(day: i64) -> Self {
        if day < SCALED_HELP_DAY {
            return Self::default();
        }
        let check_ms = match super::daily_difficulty(day) {
            Difficulty::Easy => 10_000,
            Difficulty::Medium => 15_000,
            Difficulty::Hard => 20_000,
            Difficulty::Expert => 25_000,
            Difficulty::Oni => 30_000,
        };
        Self {
            check_ms,
            hint_ms: 3 * check_ms,
        }
    }

    pub fn is_legacy(&self) -> bool {
        *self == Self::default()
    }

    pub fn total(self, hints: i64, checks: i64) -> i64 {
        hints
            .saturating_mul(self.hint_ms)
            .saturating_add(checks.saturating_mul(self.check_ms))
    }
}

/// A username can be changed once in this many days.
pub const RENAME_EVERY_DAYS: i64 = 30;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub name: String,
    /// Guests play without an account, they can claim a username later.
    pub guest: bool,
    pub created_at: i64,
    /// Can open the admin dashboard (`HASHI_ADMINS` on the server).
    #[serde(default)]
    pub admin: bool,
    /// When the username was last changed, `None`: never.
    #[serde(default)]
    pub renamed_at: Option<i64>,
    /// An admin listed by username in `HASHI_ADMINS`, who can't change it.
    #[serde(default)]
    pub name_locked: bool,
}

/// What the account page shows of an account's recovery email (`server::recovery`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEmail {
    /// The confirmed address, the one reset links go to. `None`: none.
    pub address: Option<String>,
    /// A new address whose confirmation link wasn't opened yet. Until it is, `address` stays.
    pub pending: Option<String>,
    /// Whether the server can send mail at all: without SMTP there are no recovery emails.
    pub mail: bool,
}

/// The longest email address there can be (RFC 5321), in bytes.
pub const MAX_EMAIL: usize = 254;

impl UserInfo {
    /// From when the username can be changed again, `None`: right away.
    pub fn rename_from(&self, now: i64) -> Option<i64> {
        self.renamed_at
            .map(|at| at + RENAME_EVERY_DAYS * super::MS_PER_DAY)
            .filter(|&from| from > now)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptInfo {
    /// Server-selected prices, retained when the client updates help counts.
    #[serde(default, skip_serializing_if = "HelpPenalties::is_legacy")]
    pub penalties: HelpPenalties,
    /// Stable identity across guest transfer, independent of user IDs and wall-clock time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub attempt_id: String,
    /// Server time (ms) when the player pressed start.
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub hints: i64,
    pub checks: i64,
    /// The SAT solver showed the solution, the run isn't ranked.
    pub revealed: bool,
    /// Final time including penalties.
    pub time_ms: Option<i64>,
    /// Bridges of the finished board.
    pub bridges: Option<Vec<u8>>,
    /// Why the result isn't ranked, if it isn't.
    #[serde(default)]
    pub flag: Option<Flag>,
}

impl AttemptInfo {
    pub fn penalty_ms(&self) -> i64 {
        self.penalties.total(self.hints, self.checks)
    }

    pub fn finished(&self) -> bool {
        self.finished_at.is_some() || self.revealed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyInfo {
    pub day: i64,
    pub difficulty: Difficulty,
    /// Only sent once the attempt has started, so nobody can study it before the clock runs.
    pub puzzle: Option<Puzzle>,
    pub attempt: Option<AttemptInfo>,
    /// Server time when the response was created, to correct the client clock.
    pub server_now: i64,
    /// Players who finished this daily (ranked).
    pub finishers: i64,
    /// Stamps the player earned since the last visit (yesterday's medal, say), to announce.
    #[serde(default)]
    pub stamps: Vec<Stamp>,
}

/// What the start of the daily sends: the started daily and who plays it, a guest the start just
/// made too, so the page can show the board at once instead of asking who it is first.
///
/// The daily's fields are at the top level: a page or app from before reads the reply as the
/// [`DailyInfo`] it was, and the reply of an older server loads without a player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyStart {
    #[serde(flatten)]
    pub info: DailyInfo,
    #[serde(default)]
    pub user: Option<UserInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitResult {
    pub time_ms: i64,
    /// 0 when the result isn't ranked, see `flag`.
    pub rank: i64,
    pub finishers: i64,
    #[serde(default)]
    pub flag: Option<Flag>,
    /// Stamps earned with this result, to announce.
    #[serde(default)]
    pub stamps: Vec<Stamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub time_ms: i64,
    pub hints: i64,
    pub checks: i64,
    /// The stamp the player chose to show next to their name.
    #[serde(default)]
    pub stamp: Option<Stamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leaderboard {
    pub day: i64,
    pub difficulty: Difficulty,
    pub entries: Vec<LeaderboardEntry>,
    /// The current user's entry, also when it isn't in `entries`.
    pub me: Option<LeaderboardEntry>,
    pub finishers: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayResult {
    pub day: i64,
    /// `None` if the day wasn't finished (or the solution was revealed).
    pub time_ms: Option<i64>,
    pub rank: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserStats {
    pub solved: i64,
    pub current_streak: i64,
    pub best_streak: i64,
    pub best_time_ms: Option<i64>,
    pub average_time_ms: Option<i64>,
    /// Finished dailies of the last weeks, oldest first.
    pub recent: Vec<DayResult>,
    /// Medals of days that are over.
    pub gold: i64,
    pub silver: i64,
    pub bronze: i64,
    /// Place and points on this month's board, `None` without points this month.
    pub month_rank: Option<i64>,
    pub month_points: i64,
    /// Every stamp and how far along the player is.
    pub stamps: Vec<StampProgress>,
    /// Everyone with a stamp, for the share of players who have each one.
    #[serde(default)]
    pub players: i64,
    /// The stamp shown next to the player's name on the leaderboards.
    #[serde(default)]
    pub shown: Option<Stamp>,
}

/// What everybody can see of a player: their profile, linked from the leaderboards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub id: i64,
    pub name: String,
    pub guest: bool,
    pub created_at: i64,
    pub stats: UserStats,
}

#[cfg(test)]
mod penalty_tests {
    use super::*;

    #[test]
    fn november_prices_follow_puzzle_date_and_difficulty() {
        assert_eq!(
            super::super::parse_date("2026-11-01"),
            Some(SCALED_HELP_DAY)
        );
        assert_eq!(
            HelpPenalties::daily(SCALED_HELP_DAY - 1),
            HelpPenalties::default()
        );
        // Sunday through Saturday, including both Medium and Hard days.
        for (offset, seconds) in [30, 10, 15, 15, 20, 20, 25].into_iter().enumerate() {
            let prices = HelpPenalties::daily(SCALED_HELP_DAY + i64::try_from(offset).unwrap());
            assert_eq!(prices.check_ms, seconds * 1000);
            assert_eq!(prices.hint_ms, seconds * 3000);
            assert_eq!(prices.total(2, 3), seconds * 9000);
        }
    }

    #[test]
    fn old_attempts_default_to_historical_prices_and_new_prices_round_trip() {
        let old = r#"{"started_at":0,"finished_at":null,"hints":1,"checks":2,"revealed":false,"time_ms":null,"bridges":null}"#;
        let mut attempt: AttemptInfo = serde_json::from_str(old).unwrap();
        assert_eq!(attempt.penalty_ms(), 50_000);
        assert!(
            !serde_json::to_string(&attempt)
                .unwrap()
                .contains("penalties")
        );
        attempt.penalties = HelpPenalties::daily(SCALED_HELP_DAY);
        let restored: AttemptInfo =
            serde_json::from_str(&serde_json::to_string(&attempt).unwrap()).unwrap();
        assert_eq!(restored.penalty_ms(), 150_000);
        assert_eq!(restored, attempt);
    }
}
