//! Users and the daily puzzle: attempts, results, leaderboards and statistics.

use common::{Difficulty, Puzzle};
use serde::{Deserialize, Serialize};

use super::moves::Flag;

/// Added to the time of a daily run for every hint.
pub const HINT_PENALTY_MS: i64 = 30_000;
/// Added to the time of a daily run for every check.
pub const CHECK_PENALTY_MS: i64 = 10_000;

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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptInfo {
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
        self.hints * HINT_PENALTY_MS + self.checks * CHECK_PENALTY_MS
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitResult {
    pub time_ms: i64,
    /// 0 when the result isn't ranked, see `flag`.
    pub rank: i64,
    pub finishers: i64,
    #[serde(default)]
    pub flag: Option<Flag>,
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
pub struct AllTimeEntry {
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub solved: i64,
    pub best_streak: i64,
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
}
