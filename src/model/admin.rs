//! The admin dashboard: overview, attempts of a day, Dojo submissions and contact messages.

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
    /// Contact messages waiting to be answered (and deleted).
    pub messages: i64,
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

/// A message from the contact form.
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
}
