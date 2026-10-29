//! The admin dashboard: overview, attempts of a day, Dojo submissions, names, bans, reports and
//! feedback.

use serde::{Deserialize, Serialize};

use super::dojo::DojoState;
use super::moves::{Flag, LogSummary};

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
    /// Feedback nobody looked at yet (`FeedbackStatus::New`).
    pub messages: i64,
    /// Names on the boards the list of blocked names flags, see [`FlaggedName`].
    #[serde(default)]
    pub names: i64,
    /// Reports of players nobody looked at yet, see `AdminReport`.
    #[serde(default)]
    pub reports: i64,
    /// The repository feedback becomes issues in (`owner/name`), `None` without
    /// `GITHUB_FEEDBACK_TOKEN`.
    #[serde(default)]
    pub issues_repo: Option<String>,
}

/// A name on the boards that the list of blocked names flags: chosen before a word was on the
/// list, or (players) one that looks official.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlaggedName {
    pub kind: NameKind,
    /// The user or the Dojo submission.
    pub id: i64,
    pub name: String,
    /// The entry of the list it matches.
    pub word: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameKind {
    Player,
    Solver,
}

/// The name an admin gives a player whose name isn't welcome. They can choose another one right
/// away, it's no username (it has a space), so it signs nobody in.
pub fn neutral_name(user: i64) -> String {
    format!("Player {user}")
}

/// A banned player, see `server::admin::ban`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannedPlayer {
    pub id: i64,
    pub name: String,
    /// `None` for guests.
    pub username: Option<String>,
    pub banned_at: i64,
    /// Why, for the admins. Empty if none was given.
    pub reason: String,
}

/// What banning a player did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BanOutcome {
    /// Banned now (or already), with their name.
    Banned(String),
    NoSuchPlayer,
    /// Admins can't be banned, not even by themselves.
    Admin,
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

/// A community benchmark as moderators see it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminBenchmark {
    pub id: i64,
    pub family: String,
    pub description: String,
    pub author: String,
    pub state: crate::model::BenchmarkState,
    pub reason: Option<String>,
    pub claimed: Option<dojo::Answer>,
    pub answer: Option<dojo::Answer>,
    pub vars: u32,
    pub clauses: u32,
    pub solve_ms: Option<u64>,
    pub from_gbd: bool,
    pub hash: String,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkAction {
    /// Into the pool (verified ones only).
    Accept,
    /// Not into the pool, or out of it for later seasons.
    Reject,
    /// Verify it again.
    Requeue,
    /// Gone for good, unless a season drew it.
    Delete,
}

/// A message of the feedback form (`/kontakt`).
///
/// The fields after `mailed` came with the feedback inbox: a page from before reads them as
/// unknown fields, a page reading a server from before gets their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactMessage {
    pub id: i64,
    /// Empty if not given.
    pub name: String,
    pub email: String,
    pub message: String,
    pub created_at: i64,
    /// Whether it was mailed to us.
    pub mailed: bool,
    #[serde(default)]
    pub kind: super::FeedbackKind,
    /// What the page sent along, `None` for messages from before (and from apps that don't send
    /// it).
    #[serde(default)]
    pub context: Option<super::FeedbackContext>,
    /// The language to answer in (`de`), if the page said.
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub status: super::FeedbackStatus,
    /// The GitHub issue made of it.
    #[serde(default)]
    pub github_issue: Option<i64>,
    /// When it was answered, closed or made an issue.
    #[serde(default)]
    pub handled_at: Option<i64>,
}
