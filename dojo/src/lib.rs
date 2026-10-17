//! The SAT Dojo: a gym for SAT solvers.
//!
//! Submissions are WebAssembly programs (WASI preview 1) that read a DIMACS CNF formula on stdin
//! and answer in the SAT competition format. Each one runs in a sandbox without file system or
//! network, with a fixed budget of *fuel* (executed WebAssembly instructions) instead of a time
//! limit, so a score is exactly reproducible and doesn't depend on the machine or its load.
//!
//! - always: the result types shared with the website ([`Family`], [`Score`], [`Report`], ...)
//! - `bench`: benchmark generation ([`bench`]) and answer checking ([`output`])
//! - `runner`: checking ([`validate`]), the sandbox ([`sandbox`]) and evaluating a set ([`eval`])
//! - `worker`: the HTTP service that evaluates submissions for the website ([`worker`])

use serde::{Deserialize, Serialize};

#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "bench")]
pub mod cnf;
#[cfg(feature = "runner")]
pub mod eval;
#[cfg(feature = "bench")]
pub mod output;
#[cfg(feature = "runner")]
pub mod sandbox;
#[cfg(feature = "runner")]
pub mod validate;
#[cfg(feature = "worker")]
pub mod worker;

/// Fuel for one instance: about 10 billion WebAssembly instructions.
pub const DEFAULT_BUDGET: u64 = 10_000_000_000;

/// Largest accepted submission.
pub const MAX_WASM_BYTES: usize = 8 << 20;

/// Formulas in a set (see `bench::SPEC`).
pub const SET_SIZE: u32 = 34;

/// The public development set, the same for everyone and downloadable.
pub const DEV_SEED: &str = "hashi-dojo-dev-1";

/// Kinds of formulas in a benchmark set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Family {
    /// Hashiwokakero puzzles (without the connectivity rule), always satisfiable.
    Hashi,
    /// Hashi puzzles with one number changed so the sum is odd: unsatisfiable, since every
    /// bridge adds two to the sum. A parity argument, which is hard for resolution.
    HashiParity,
    /// Uniform random 3-SAT at the phase transition (4.26 clauses per variable).
    Random3Sat,
    /// Pigeons into holes, unsatisfiable with one pigeon too many.
    Pigeonhole,
    /// N queens on an N×N board without attacks.
    Queens,
}

impl Family {
    pub const ALL: [Family; 5] = [
        Family::Hashi,
        Family::HashiParity,
        Family::Random3Sat,
        Family::Pigeonhole,
        Family::Queens,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Family::Hashi => "hashi",
            Family::HashiParity => "hashi-parity",
            Family::Random3Sat => "random-3sat",
            Family::Pigeonhole => "pigeonhole",
            Family::Queens => "queens",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Sat,
    Unsat,
}

/// What became of one instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Correct answer; satisfiable answers come with a model that was checked.
    Solved(Answer),
    /// The solver finished without an answer (`s UNKNOWN` or nothing).
    Unknown,
    /// Out of fuel, or the wall clock safety limit was hit.
    Timeout,
    /// The solver trapped or ran out of memory.
    Crashed,
    /// A wrong answer: UNSAT for a satisfiable formula or a model that doesn't satisfy it.
    Wrong,
}

impl Outcome {
    pub fn solved(self) -> bool {
        matches!(self, Outcome::Solved(_))
    }
}

/// One instance of an evaluation. Only shown for the public development set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub name: String,
    pub family: Family,
    pub expected: Answer,
    pub outcome: Outcome,
    /// Fuel used, including the charge for bulk memory operations.
    pub fuel: u64,
    pub wall_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyScore {
    pub family: Family,
    pub solved: u32,
    pub total: u32,
    /// Mean PAR-2 in billions of fuel units.
    pub par2: f64,
}

/// The result of a whole set: what the leaderboard shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Mean PAR-2 in billions of fuel: solved instances count their fuel, others twice the budget.
    pub par2: f64,
    pub solved: u32,
    pub total: u32,
    /// Wrong answers disqualify a submission.
    pub wrong: u32,
    pub budget: u64,
    /// Total fuel of the solved instances, in billions.
    pub fuel: f64,
    pub families: Vec<FamilyScore>,
}

impl Score {
    pub fn from_results(results: &[RunResult], budget: u64) -> Self {
        const G: f64 = 1e9;
        let par2 = |rs: &[&RunResult]| {
            if rs.is_empty() {
                return 0.0;
            }
            let sum: f64 = rs
                .iter()
                .map(|r| if r.outcome.solved() { r.fuel as f64 } else { 2.0 * budget as f64 })
                .sum();
            sum / rs.len() as f64 / G
        };
        let all: Vec<&RunResult> = results.iter().collect();
        let families = Family::ALL
            .into_iter()
            .filter_map(|family| {
                let rs: Vec<&RunResult> = results.iter().filter(|r| r.family == family).collect();
                (!rs.is_empty()).then(|| FamilyScore {
                    family,
                    solved: rs.iter().filter(|r| r.outcome.solved()).count() as u32,
                    total: rs.len() as u32,
                    par2: par2(&rs),
                })
            })
            .collect();
        Score {
            par2: par2(&all),
            solved: results.iter().filter(|r| r.outcome.solved()).count() as u32,
            total: results.len() as u32,
            wrong: results.iter().filter(|r| r.outcome == Outcome::Wrong).count() as u32,
            budget,
            fuel: results
                .iter()
                .filter(|r| r.outcome.solved())
                .map(|r| r.fuel as f64)
                .sum::<f64>()
                / G,
            families,
        }
    }
}

/// Why a submission was refused before running it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Rejection {
    TooLarge,
    /// Not a valid WebAssembly module, or one using features the Dojo doesn't allow.
    Invalid(String),
    /// Imports from anything but `wasi_snapshot_preview1`.
    Import(String),
    /// No `_start` function: build a command (a binary), not a library.
    NoStart,
}

/// Messages from the worker to the website about one evaluation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Report {
    Progress { id: i64, season: u32, done: u32, total: u32 },
    Finished { id: i64, season: u32, score: Score },
    Rejected { id: i64, season: u32, reason: Rejection },
    /// Our fault, not the submission's: the website tries again later.
    Failed { id: i64, season: u32, message: String },
}

impl Report {
    pub fn id(&self) -> i64 {
        match self {
            Report::Progress { id, .. }
            | Report::Finished { id, .. }
            | Report::Rejected { id, .. }
            | Report::Failed { id, .. } => *id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(family: Family, outcome: Outcome, fuel: u64) -> RunResult {
        RunResult {
            name: String::new(),
            family,
            expected: Answer::Sat,
            outcome,
            fuel,
            wall_ms: 0,
        }
    }

    #[test]
    fn par2() {
        let budget = 10_000_000_000;
        let results = [
            run(Family::Hashi, Outcome::Solved(Answer::Sat), 1_000_000_000),
            run(Family::Hashi, Outcome::Timeout, budget),
            run(Family::Queens, Outcome::Wrong, 5),
        ];
        let score = Score::from_results(&results, budget);
        assert_eq!((score.solved, score.total, score.wrong), (1, 3, 1));
        assert!((score.par2 - (1.0 + 20.0 + 20.0) / 3.0).abs() < 1e-9);
        assert_eq!(score.families.len(), 2);
        assert!((score.families[0].par2 - 10.5).abs() < 1e-9);
    }
}
