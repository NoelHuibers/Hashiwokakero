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
#[cfg(feature = "bench")]
pub mod drat;
#[cfg(feature = "runner")]
pub mod eval;
#[cfg(feature = "bench")]
pub mod output;
// In a tests/ directory, so coverage doesn't count the tests themselves
#[cfg(all(test, feature = "bench"))]
#[path = "tests/props.rs"]
mod props;
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
    pub const ALL: [Self; 5] = [
        Self::Hashi,
        Self::HashiParity,
        Self::Random3Sat,
        Self::Pigeonhole,
        Self::Queens,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Hashi => "hashi",
            Self::HashiParity => "hashi-parity",
            Self::Random3Sat => "random-3sat",
            Self::Pigeonhole => "pigeonhole",
            Self::Queens => "queens",
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
        matches!(self, Self::Solved(_))
    }
}

/// One instance of an evaluation. Only shown for the public development set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// A number of instances as `u32` (a set has a few dozen), saturating.
pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Score {
    pub fn from_results(results: &[RunResult], budget: u64) -> Self {
        const G: f64 = 1e9;
        #[expect(
            clippy::cast_precision_loss,
            reason = "fuel (at most a few budgets of 10^10) and instance counts are far below \
                      2^53, so the conversions are exact"
        )]
        let par2 = |rs: &[&RunResult]| {
            if rs.is_empty() {
                return 0.0;
            }
            let sum: f64 = rs
                .iter()
                .map(|r| {
                    if r.outcome.solved() {
                        r.fuel as f64
                    } else {
                        2.0 * budget as f64
                    }
                })
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
                    solved: count(rs.iter().filter(|r| r.outcome.solved()).count()),
                    total: count(rs.len()),
                    par2: par2(&rs),
                })
            })
            .collect();
        #[expect(
            clippy::cast_precision_loss,
            reason = "fuel per instance is at most the budget (10^10 by default), far below \
                      2^53, so the conversion is exact"
        )]
        let fuel = results
            .iter()
            .filter(|r| r.outcome.solved())
            .map(|r| r.fuel as f64)
            .sum::<f64>()
            / G;
        Self {
            par2: par2(&all),
            solved: count(results.iter().filter(|r| r.outcome.solved()).count()),
            total: count(results.len()),
            wrong: count(
                results
                    .iter()
                    .filter(|r| r.outcome == Outcome::Wrong)
                    .count(),
            ),
            budget,
            fuel,
            families,
        }
    }
}

/// Why a submission was refused before running it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rejection {
    TooLarge,
    /// Not a valid WebAssembly module, or one using features the Dojo doesn't allow.
    Invalid(String),
    /// Imports from anything but `wasi_snapshot_preview1`.
    Import(String),
    /// No `_start` function: build a command (a binary), not a library.
    NoStart,
}

/// One number per formula, in set order.
///
/// The fuel of a solved formula, otherwise a marker for what happened. Scores are
/// deterministic, so two submissions with (nearly) the same fingerprint are (nearly) the same
/// solver.
pub fn fingerprint(results: &[RunResult]) -> Vec<u64> {
    results
        .iter()
        .map(|r| match r.outcome {
            Outcome::Solved(_) => r.fuel,
            Outcome::Timeout => FINGERPRINT_TIMEOUT,
            Outcome::Unknown | Outcome::Crashed => FINGERPRINT_GAVE_UP,
            Outcome::Wrong => FINGERPRINT_WRONG,
        })
        .collect()
}

pub const FINGERPRINT_TIMEOUT: u64 = u64::MAX;
pub const FINGERPRINT_GAVE_UP: u64 = u64::MAX - 1;
pub const FINGERPRINT_WRONG: u64 = u64::MAX - 2;

/// Whether `newer` looks like a copy of `older`.
///
/// That is: the same outcome on every formula and, on at least `MIN_SHARED` solved formulas,
/// fuel within `TOLERANCE` of each other. Independent solvers never come that close; a copy
/// changed just enough to escape this has to behave differently, and padding it with extra
/// work only makes it rank below the original.
pub fn similar(newer: &[u64], older: &[u64]) -> bool {
    const TOLERANCE: f64 = 0.02;
    const MIN_SHARED: usize = 8;
    if newer.len() != older.len() || newer.is_empty() {
        return false;
    }
    let mut shared = 0;
    for (&a, &b) in newer.iter().zip(older) {
        let solved_a = a < FINGERPRINT_WRONG;
        let solved_b = b < FINGERPRINT_WRONG;
        if solved_a != solved_b || (!solved_a && a != b) {
            return false;
        }
        if solved_a {
            #[expect(clippy::cast_precision_loss, reason = "fuel is far below 2^53")]
            let (a, b) = (a as f64, b as f64);
            if (a - b).abs() > TOLERANCE * a.max(b) {
                return false;
            }
            shared += 1;
        }
    }
    shared >= MIN_SHARED
}

/// Messages from the worker to the website about one evaluation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Report {
    Progress {
        id: i64,
        season: u32,
        done: u32,
        total: u32,
    },
    Finished {
        id: i64,
        season: u32,
        score: Score,
        /// How the solver did on each formula, see [`fingerprint`]. Kept private by the
        /// website, it only compares submissions to find copies.
        #[serde(default)]
        fingerprint: Vec<u64>,
    },
    Rejected {
        id: i64,
        season: u32,
        reason: Rejection,
    },
    /// Our fault, not the submission's: the website tries again later.
    Failed {
        id: i64,
        season: u32,
        message: String,
    },
}

impl Report {
    pub fn id(&self) -> i64 {
        match self {
            Self::Progress { id, .. }
            | Self::Finished { id, .. }
            | Self::Rejected { id, .. }
            | Self::Failed { id, .. } => *id,
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
