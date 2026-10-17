//! Data exchanged between the server functions and the pages: users, daily puzzles, results.
//!
//! Days are counted in UTC since 1970-01-01, the daily puzzle changes at midnight UTC for
//! everyone.

use common::{Board, Difficulty, Puzzle};
use serde::{Deserialize, Serialize};

/// The first daily puzzle, #1.
pub const LAUNCH_DAY: i64 = 20_743; // 2026-10-17
/// Added to the time of a daily run for every hint.
pub const HINT_PENALTY_MS: i64 = 30_000;
/// Added to the time of a daily run for every check.
pub const CHECK_PENALTY_MS: i64 = 10_000;
pub const MS_PER_DAY: i64 = 86_400_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub name: String,
    /// Guests play without an account, they can claim a username later.
    pub guest: bool,
    pub created_at: i64,
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
    pub rank: i64,
    pub finishers: i64,
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

/* ------------------------------------------------------------------ */
/*  SAT Dojo                                                           */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DojoState {
    /// Waiting for the worker.
    Queued,
    Running,
    Done,
    /// The module can't be run (see [`DojoSubmission::rejection`]).
    Rejected,
    /// Evaluation kept failing on our side.
    Failed,
}

impl DojoState {
    pub fn id(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "running" => Self::Running,
            "done" => Self::Done,
            "rejected" => Self::Rejected,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }

    pub fn busy(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoEntry {
    pub rank: u32,
    pub id: i64,
    pub name: String,
    pub author: String,
    /// One of our own solvers, for comparison.
    pub house: bool,
    pub score: dojo::Score,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoBoard {
    pub season: u32,
    /// Formulas in the hidden set and fuel per formula.
    pub formulas: u32,
    pub budget: u64,
    /// Best submission of each author, best first.
    pub entries: Vec<DojoEntry>,
    /// Submissions waiting or being evaluated.
    pub queued: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoSubmission {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub source_url: Option<String>,
    pub author: String,
    pub house: bool,
    pub created_at: i64,
    pub state: DojoState,
    pub season: u32,
    pub progress: u32,
    pub total: u32,
    pub score: Option<dojo::Score>,
    pub rejection: Option<dojo::Rejection>,
    /// Position on the leaderboard, if it is the author's best.
    pub rank: Option<u32>,
    pub size: i64,
    pub sha256: String,
    pub mine: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoRow {
    pub id: i64,
    pub name: String,
    pub state: DojoState,
    pub created_at: i64,
    pub score: Option<dojo::Score>,
}

/// What the current user can do in the Dojo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoMe {
    /// Only registered accounts can submit.
    pub registered: bool,
    pub left_today: u32,
    pub submissions: Vec<DojoRow>,
}

/* ------------------------------------------------------------------ */
/*  Errors                                                             */
/* ------------------------------------------------------------------ */

/// Errors the server reports to players. They travel as `E:<code>` in the server function error
/// and are translated on the client (`common.errors.<code>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiError {
    Database,
    Internal,
    ArchiveOnlyPast,
    PasswordTooShort,
    PasswordTooLong,
    UsernameInvalid,
    UsernameTaken,
    WrongLogin,
    TooManyAttempts,
    SessionExpired,
    NotStarted,
    NotSolved,
    WrongPuzzle,
    AlreadyFinished,
}

impl ApiError {
    pub const ALL: [Self; 14] = [
        Self::Database,
        Self::Internal,
        Self::ArchiveOnlyPast,
        Self::PasswordTooShort,
        Self::PasswordTooLong,
        Self::UsernameInvalid,
        Self::UsernameTaken,
        Self::WrongLogin,
        Self::TooManyAttempts,
        Self::SessionExpired,
        Self::NotStarted,
        Self::NotSolved,
        Self::WrongPuzzle,
        Self::AlreadyFinished,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Internal => "internal",
            Self::ArchiveOnlyPast => "archive_only_past",
            Self::PasswordTooShort => "password_too_short",
            Self::PasswordTooLong => "password_too_long",
            Self::UsernameInvalid => "username_invalid",
            Self::UsernameTaken => "username_taken",
            Self::WrongLogin => "wrong_login",
            Self::TooManyAttempts => "too_many_attempts",
            Self::SessionExpired => "session_expired",
            Self::NotStarted => "not_started",
            Self::NotSolved => "not_solved",
            Self::WrongPuzzle => "wrong_puzzle",
            Self::AlreadyFinished => "already_finished",
        }
    }

    /// Finds the `E:<code>` marker in an error message.
    pub fn from_message(message: &str) -> Option<Self> {
        let code = message.split("E:").nth(1)?;
        let code = code
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()?;
        Self::ALL.into_iter().find(|e| e.code() == code)
    }
}

#[cfg(feature = "ssr")]
impl From<ApiError> for leptos::prelude::ServerFnError {
    fn from(e: ApiError) -> Self {
        Self::new(format!("E:{}", e.code()))
    }
}

#[cfg(feature = "ssr")]
impl ApiError {
    /// The error as a server function error, where `.into()` can't infer the type.
    pub fn fail(self) -> leptos::prelude::ServerFnError {
        self.into()
    }
}

/* ------------------------------------------------------------------ */
/*  Hints                                                              */
/* ------------------------------------------------------------------ */

/// The bridge a hint fixes: wrong bridges first, then a missing bridge at the most constrained
/// island. Returns the edge and its value in the solution.
pub fn pick_hint(board: &Board, current: &[u8], solution: &[u8]) -> Option<(usize, u8)> {
    let n = board.edges.len();
    if current.len() != n || solution.len() != n {
        return None;
    }
    let wrong = (0..n).find(|&e| current[e] > solution[e]);
    let missing_at = |island: usize| {
        board.incident[island]
            .iter()
            .filter(|&&e| current[e] < solution[e])
            .count()
    };
    let edge = wrong.or_else(|| {
        (0..n)
            .filter(|&e| current[e] < solution[e])
            .min_by_key(|&e| missing_at(board.edges[e].a).min(missing_at(board.edges[e].b)))
    })?;
    Some((edge, solution[edge]))
}

/// Edges with more bridges than in the solution.
pub fn wrong_edges(current: &[u8], solution: &[u8]) -> Vec<usize> {
    current
        .iter()
        .zip(solution)
        .enumerate()
        .filter(|(_, (c, s))| c > s)
        .map(|(e, _)| e)
        .collect()
}

/* ------------------------------------------------------------------ */
/*  Dates                                                              */
/* ------------------------------------------------------------------ */

/// Day number of a unix timestamp in ms.
pub fn day_of(ms: i64) -> i64 {
    ms.div_euclid(MS_PER_DAY)
}

/// (year, month, day) of a day number, Howard Hinnant's `civil_from_days`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "day of month (1..=31) and month (1..=12) are in range by construction"
)]
pub fn civil(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Day number of a date, inverse of [`civil`].
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-10-17`
pub fn date_string(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_date(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let day = days_from_civil(y, m, d);
    (civil(day) == (y, m, d)).then_some(day)
}

/// 0 = Monday … 6 = Sunday
#[expect(
    clippy::cast_possible_truncation,
    reason = "`rem_euclid(7)` is in 0..7"
)]
pub fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

pub const WEEKDAY_KANJI: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];
pub const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Like a newspaper puzzle the dailies get harder during the week.
pub fn daily_difficulty(day: i64) -> Difficulty {
    match weekday(day) {
        0 => Difficulty::Easy,
        1 | 2 => Difficulty::Medium,
        3 | 4 => Difficulty::Hard,
        5 => Difficulty::Expert,
        _ => Difficulty::Oni,
    }
}

/// Daily puzzle #1 was on the launch day.
pub fn daily_number(day: i64) -> i64 {
    day - LAUNCH_DAY + 1
}

/// `Sat 17 Oct 2026`
pub fn pretty_date(day: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d) = civil(day);
    format!(
        "{} {d} {} {y}",
        &WEEKDAY_NAMES[weekday(day)][..3],
        MONTHS[m as usize - 1]
    )
}

/// `1:05.3` / `12:30` / `1:02:03`
pub fn format_duration(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else if total < 600 {
        format!("{m}:{s:02}.{}", ms.max(0) % 1000 / 100)
    } else {
        format!("{m}:{s:02}")
    }
}

/// Current streak (ending today or yesterday) and best streak from sorted solved days.
pub fn streaks(solved_days: &[i64], today: i64) -> (i64, i64) {
    let (mut best, mut run, mut prev) = (0, 0, None::<i64>);
    for &day in solved_days {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(day);
    }
    let current = match prev {
        Some(last) if last >= today - 1 => run,
        _ => 0,
    };
    (current, best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_string(0), "1970-01-01");
        assert_eq!(date_string(LAUNCH_DAY), "2026-10-17");
        assert_eq!(parse_date("2026-10-17"), Some(LAUNCH_DAY));
        assert_eq!(
            parse_date("2024-02-29").map(date_string).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("nope"), None);
        for day in [-1000, 0, 1, 59, 60, 11_016, LAUNCH_DAY, 100_000] {
            assert_eq!(parse_date(&date_string(day)), Some(day));
        }
    }

    #[test]
    fn weekdays() {
        // 1970-01-01 was a Thursday, 2026-10-17 is a Saturday
        assert_eq!(weekday(0), 3);
        assert_eq!(weekday(LAUNCH_DAY), 5);
        assert_eq!(pretty_date(LAUNCH_DAY), "Sat 17 Oct 2026");
        assert_eq!(daily_difficulty(LAUNCH_DAY), Difficulty::Expert);
        assert_eq!(daily_difficulty(LAUNCH_DAY + 1), Difficulty::Oni);
        assert_eq!(daily_difficulty(LAUNCH_DAY + 2), Difficulty::Easy);
        assert_eq!(daily_number(LAUNCH_DAY), 1);
    }

    #[test]
    fn streak_counting() {
        assert_eq!(streaks(&[], 10), (0, 0));
        assert_eq!(streaks(&[8, 9, 10], 10), (3, 3));
        assert_eq!(streaks(&[8, 9], 10), (2, 2));
        assert_eq!(streaks(&[7, 8], 10), (0, 2));
        assert_eq!(streaks(&[1, 2, 3, 4, 9, 10], 10), (2, 4));
    }

    #[test]
    fn error_codes_round_trip() {
        for e in ApiError::ALL {
            let message = format!("error running server function: E:{}", e.code());
            assert_eq!(ApiError::from_message(&message), Some(e));
        }
        assert_eq!(ApiError::from_message("something else"), None);
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(65_300), "1:05.3");
        assert_eq!(format_duration(750_000), "12:30");
        assert_eq!(format_duration(3_723_000), "1:02:03");
    }
}
