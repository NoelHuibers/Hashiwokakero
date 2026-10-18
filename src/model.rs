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

/* ------------------------------------------------------------------ */
/*  SAT Dojo                                                           */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// An earlier submission by someone else that this one behaves like: not ranked then.
    #[serde(default)]
    pub similar_to: Option<i64>,
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
/*  Fair play                                                          */
/* ------------------------------------------------------------------ */

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
}

impl Flag {
    pub const ALL: [Self; 5] = [
        Self::Log,
        Self::TooFast,
        Self::Pace,
        Self::SecondAccount,
        Self::Review,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::TooFast => "too_fast",
            Self::Pace => "pace",
            Self::SecondAccount => "second_account",
            Self::Review => "review",
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

/* ------------------------------------------------------------------ */
/*  Admin                                                              */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminOverview {
    pub users: i64,
    pub registered: i64,
    /// Today, and its daily so far.
    pub day: i64,
    pub started: i64,
    pub finished: i64,
    pub flagged: i64,
    pub dojo_pending: i64,
    pub dojo_total: i64,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAttempt {
    pub user_id: i64,
    pub name: String,
    pub username: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// With penalties.
    pub time_ms: Option<i64>,
    pub hints: i64,
    pub checks: i64,
    pub moves: Option<i64>,
    pub revealed: bool,
    pub flag: Option<Flag>,
    pub log: LogSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdminSubmission {
    pub id: i64,
    pub name: String,
    pub author: String,
    pub house: bool,
    pub state: DojoState,
    pub season: u32,
    pub solved: Option<u32>,
    pub total: u32,
    pub par2: Option<f64>,
    pub wrong: u32,
    pub similar_to: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionAction {
    /// Evaluate again.
    Requeue,
    /// Not a copy after all: rank it.
    ClearCopy,
    Delete,
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
    /// Our solvers don't touch today's daily before you finished it.
    DailyLocked,
    /// Only for admins.
    Forbidden,
}

impl ApiError {
    pub const ALL: [Self; 16] = [
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
        Self::DailyLocked,
        Self::Forbidden,
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
            Self::DailyLocked => "daily_locked",
            Self::Forbidden => "forbidden",
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
