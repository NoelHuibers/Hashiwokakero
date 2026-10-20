//! The SAT Dojo: submissions, their state and the leaderboard of a season.

use serde::{Deserialize, Serialize};

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
    /// The commitment to this season's hidden set: the SHA-256 of its seed, which is revealed
    /// once the season is over (see [`DojoSeason`]).
    #[serde(default)]
    pub commitment: String,
    /// `dojo::GENERATOR_VERSION` and `dojo::RULES_VERSION` of the site.
    #[serde(default)]
    pub generator_version: u32,
    #[serde(default)]
    pub rules_version: u32,
}

/// One season of the Dojo: its commitment, and once it is over its seed, set and final board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoSeason {
    pub season: u32,
    /// `None` for the current season before the site recorded its start.
    pub started_at: Option<i64>,
    /// `None` while it runs.
    pub ended_at: Option<i64>,
    /// SHA-256 (hex) of the seed of the hidden set, published from the start.
    pub commitment: String,
    /// The seed, only once the season is over.
    pub seed: Option<String>,
    pub generator_version: u32,
    pub rules_version: u32,
    /// The archived set (`/dojo/seasons/<n>/set.tar.gz`): its size and SHA-256, `None` when
    /// there is none (the season is running, or nothing was evaluated on it).
    pub set_size: Option<i64>,
    pub set_sha256: Option<String>,
    /// Finished submissions in the archive, ranked or not.
    pub entries: u32,
    /// The held-out family of its hidden set, revealed with the seed (`dojo::mystery_family`),
    /// `None` while it runs and for sets from before held-out families.
    #[serde(default)]
    pub mystery: Option<dojo::Family>,
    /// The final leaderboard. An empty author: the account was deleted.
    pub board: Vec<DojoEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoSeasons {
    pub current: DojoSeason,
    /// Finished seasons, the latest first.
    pub past: Vec<DojoSeason>,
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
