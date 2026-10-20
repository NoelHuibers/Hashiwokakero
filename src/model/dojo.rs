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
    /// Solved and PAR-2 by answer, overall and per family; `None` without per-formula results.
    #[serde(default)]
    pub breakdown: Option<dojo::metrics::EntryBreakdown>,
    /// What it adds to the other ranked entries (see [`DojoPortfolio`]), `None` when its results
    /// aren't on the season's formulas.
    #[serde(default)]
    pub contribution: Option<dojo::metrics::Contribution>,
}

/// The ranked entries of a season as a portfolio: the virtual best solver (VBS), which takes
/// the best result of any of them on every formula, and the cactus plot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DojoPortfolio {
    /// Entries in it: the ranked ones with per-formula results.
    pub entries: u32,
    pub vbs: dojo::metrics::Split,
    pub cactus: DojoCactus,
}

/// A cactus plot: per solver the fuel of every formula it solved, sorted (see
/// `dojo::metrics::curve`), so it shows how far a solver gets with how much fuel without telling
/// which formula a point is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoCactus {
    /// Fuel per formula: the top of the plot.
    pub budget: u64,
    /// Formulas in the set: the right end.
    pub formulas: u32,
    /// The virtual best solver's curve.
    pub vbs: Vec<u64>,
    /// Every solver's, in board order.
    pub curves: Vec<DojoCurve>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoCurve {
    /// The submission.
    pub id: i64,
    pub name: String,
    pub house: bool,
    /// Fuel of every solved formula, ascending, rounded to three significant digits.
    pub fuel: Vec<u64>,
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
    /// The community pool that hidden sets draw from (see [`DojoBenchmarks`]).
    #[serde(default)]
    pub community: CommunitySummary,
    /// The VBS and the cactus plot of the ranked entries, `None` while there are none.
    #[serde(default)]
    pub portfolio: Option<DojoPortfolio>,
}

/// The community pool in short, for the credits in the guide.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunitySummary {
    /// Accepted benchmarks.
    pub benchmarks: u32,
    /// Their authors, by how many they contributed (empty names: deleted accounts left out).
    pub authors: Vec<String>,
    /// How many a hidden set draws.
    pub per_set: u32,
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
    /// The community benchmarks drawn for its hidden set, with their authors; only once the
    /// season is over.
    #[serde(default)]
    pub community: Vec<CommunityCredit>,
    /// The final leaderboard. An empty author: the account was deleted.
    pub board: Vec<DojoEntry>,
    /// The VBS and the cactus plot of the final leaderboard.
    #[serde(default)]
    pub portfolio: Option<DojoPortfolio>,
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
    /// Solved and PAR-2 by answer, once it is done.
    #[serde(default)]
    pub breakdown: Option<dojo::metrics::EntryBreakdown>,
    /// What it adds to the ranked entries, if it is one of them.
    #[serde(default)]
    pub contribution: Option<dojo::metrics::Contribution>,
    /// The season's cactus plot, with this submission's curve in it even if it isn't ranked.
    #[serde(default)]
    pub portfolio: Option<DojoPortfolio>,
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

/// Where a community benchmark is: from upload to the pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BenchmarkState {
    /// Waiting for the worker.
    Queued,
    /// The worker is settling its answer.
    Checking,
    /// Answer certain, hard enough: waits for a moderator.
    Verified,
    /// The worker didn't take it (see [`DojoBenchmark::reason`]).
    Refused,
    /// In the pool.
    Accepted,
    /// A moderator didn't take it.
    Rejected,
}

impl BenchmarkState {
    pub const ALL: [Self; 6] = [
        Self::Queued,
        Self::Checking,
        Self::Verified,
        Self::Refused,
        Self::Accepted,
        Self::Rejected,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Checking => "checking",
            Self::Verified => "verified",
            Self::Refused => "refused",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or(Self::Queued)
    }
}

/// One of the player's own community benchmarks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoBenchmark {
    pub id: i64,
    pub family: String,
    pub description: String,
    pub state: BenchmarkState,
    /// Why it was refused or rejected: `dojo::community::Refusal::id`, `wrong_claim`, `failed`
    /// or `moderation`.
    pub reason: Option<String>,
    pub answer: Option<dojo::Answer>,
    pub vars: u32,
    pub clauses: u32,
    /// The GBD instance hash.
    pub hash: String,
    pub created_at: i64,
}

/// An accepted community benchmark and who contributed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityCredit {
    pub id: i64,
    pub family: String,
    /// Empty if the author deleted their account.
    pub author: String,
    pub answer: dojo::Answer,
    pub vars: u32,
    pub clauses: u32,
    pub hash: String,
}

/// The page for community benchmarks: the player's own and the pool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoBenchmarks {
    /// Only registered accounts can submit.
    pub registered: bool,
    pub left_today: u32,
    pub mine: Vec<DojoBenchmark>,
    pub pool: Vec<CommunityCredit>,
    /// How many a hidden set draws from the pool.
    pub per_set: u32,
}
