//! The SAT Dojo: a gym for SAT solvers.
//!
//! Submissions are WebAssembly programs (WASI preview 1) that read a DIMACS CNF formula on stdin
//! and answer in the SAT competition format. Each one runs in a sandbox without network and with
//! a single writable file, the proof of an UNSAT answer, with a fixed budget of *fuel* (executed
//! WebAssembly instructions) instead of a time limit, so a score is exactly reproducible and
//! doesn't depend on the machine or its load. Like in the SAT Competition, a SAT answer needs a
//! model and an UNSAT answer a DRAT or LRAT proof that a formally verified checker accepts.
//!
//! - always: the result types shared with the website ([`Family`], [`Score`], [`Report`], ...)
//! - `bench`: benchmark generation ([`bench`]), answer checking ([`output`]) and proof checking
//!   ([`proof`], [`drat`])
//! - `runner`: checking ([`validate`]), the sandbox ([`sandbox`]) and evaluating a set ([`eval`])
//! - `worker`: the HTTP service that evaluates submissions for the website ([`worker`])

use serde::{Deserialize, Serialize};

#[cfg(feature = "bench")]
pub mod archive;
#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "bench")]
pub mod cnf;
#[cfg(feature = "bench")]
pub mod drat;
#[cfg(feature = "runner")]
pub mod eval;
#[cfg(feature = "bench")]
pub mod families;
#[cfg(feature = "bench")]
pub mod lrat;
#[cfg(feature = "bench")]
pub mod output;
#[cfg(feature = "bench")]
pub mod proof;
// In a tests/ directory, so coverage doesn't count the tests themselves
#[cfg(all(test, feature = "bench"))]
#[path = "tests/proofs.rs"]
mod proof_tests;
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

/// Formulas in a season's hidden set (see `bench::hidden_spec`).
pub const SET_SIZE: u32 = 48;

/// Formulas in the development set (see `bench::DEV_SPEC`).
pub const DEV_SET_SIZE: u32 = 36;

/// The public development set, the same for everyone and downloadable.
pub const DEV_SEED: &str = "hashi-dojo-dev-2";

/// The version of the benchmark generator: families, sizes, labels, scrambling.
///
/// Bump it whenever the same seed gives other formulas (`bench::tests::sets_are_unchanged` fails), together with
/// [`RULES_VERSION`]. The worker's cache of generated sets is keyed by it, so a new generator
/// never evaluates on a set an older one cached, and every archived set says which one made it.
///
/// - 1: Hashi, Hashi parity, random 3-SAT, pigeonhole and queens
/// - 2: factoring, miters and colouring, held-out families, larger sizes in hidden sets
pub const GENERATOR_VERSION: u32 = 2;

/// The version of the rules a score is computed with.
///
/// That is the sets ([`GENERATOR_VERSION`]), the judging of answers, fuel costs and limits, the
/// seeds of clocks and random numbers, the checking of proofs. Bump it whenever a change makes the
/// same solver score differently. Every result is stored with the
/// version it was computed with; when the website runs a newer one, it evaluates every finished
/// submission of the season again (same season, nothing archived), and the worker refuses jobs
/// meant for another version than its own.
///
/// - 1: the first rules
/// - 2: UNSAT needs a proof the checker accepts ([`Outcome::Unverified`] without one)
/// - 3: new families and a held-out one in hidden sets ([`GENERATOR_VERSION`] 2), formulas
///   reach the solver without a comment that names them
pub const RULES_VERSION: u32 = 3;

/// Kinds of formulas in a benchmark set.
///
/// The development set has the [`Family::DEV`] families. A season's hidden set has them too, at
/// sizes beyond the dev set's, plus one family of [`Family::RESERVE`] that no dev set has (the
/// "mystery family", see [`mystery_family`]): solvers tuned to the families they trained on
/// should still generalise. Public scores of a hidden set show it as [`Family::Mystery`].
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
    /// Is a number the product of two factors greater than one? A multiplier circuit with its
    /// output fixed: satisfiable for a semiprime (the factors are the model), unsatisfiable for
    /// a prime.
    Factoring,
    /// Equivalence checking: do two structurally different multiplier circuits ever disagree?
    /// Unsatisfiable when both are correct, satisfiable when one hides a rarely triggered bug.
    Miter,
    /// Graph colouring with four colours: a random graph with a planted colouring, or one that
    /// contains a Mycielski graph, whose chromatic number is known to be five.
    Coloring,
    /// Tseitin formulas on random regular graphs: one parity constraint per vertex on its edges,
    /// unsatisfiable when the charges add up to an odd number. Hard for resolution.
    Tseitin,
    /// Completing a Latin square with holes (quasigroup with holes), always satisfiable.
    Quasigroup,
    /// A Hamiltonian cycle in a bipartite graph: a planted one, or none since the two sides
    /// differ in size.
    Hamiltonian,
    /// Stands for the season's held-out family in public scores, see [`mystery_family`]. No
    /// formula is of this family.
    Mystery,
}

impl Family {
    /// Every family that formulas are generated in.
    pub const ALL: [Self; 11] = [
        Self::Hashi,
        Self::HashiParity,
        Self::Random3Sat,
        Self::Pigeonhole,
        Self::Queens,
        Self::Factoring,
        Self::Miter,
        Self::Coloring,
        Self::Tseitin,
        Self::Quasigroup,
        Self::Hamiltonian,
    ];

    /// The families of the development set, and of every hidden set.
    pub const DEV: [Self; 8] = [
        Self::Hashi,
        Self::HashiParity,
        Self::Random3Sat,
        Self::Pigeonhole,
        Self::Queens,
        Self::Factoring,
        Self::Miter,
        Self::Coloring,
    ];

    /// The held-out families: never in the dev set, one of them in each hidden set.
    pub const RESERVE: [Self; 3] = [Self::Tseitin, Self::Quasigroup, Self::Hamiltonian];

    pub fn id(self) -> &'static str {
        match self {
            Self::Hashi => "hashi",
            Self::HashiParity => "hashi-parity",
            Self::Random3Sat => "random-3sat",
            Self::Pigeonhole => "pigeonhole",
            Self::Queens => "queens",
            Self::Factoring => "factoring",
            Self::Miter => "miter",
            Self::Coloring => "coloring",
            Self::Tseitin => "tseitin",
            Self::Quasigroup => "quasigroup",
            Self::Hamiltonian => "hamiltonian",
            Self::Mystery => "mystery",
        }
    }

    /// A generated family by its [`Family::id`] (not [`Family::Mystery`]).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    /// The family's name in the Global Benchmark Database (<https://benchmark-database.de>),
    /// whose families the SAT Competition's instances are sorted into, `None` for ours.
    pub fn gbd(self) -> Option<&'static str> {
        match self {
            Self::Hashi | Self::HashiParity | Self::Queens => Some("puzzle"),
            Self::Random3Sat => Some("random"),
            Self::Pigeonhole => Some("pigeon-hole"),
            Self::Factoring => Some("prime-factoring"),
            Self::Miter => Some("miter"),
            Self::Coloring => Some("coloring"),
            Self::Tseitin => Some("tseitin-formulas"),
            Self::Quasigroup => Some("quasigroup-completion"),
            Self::Hamiltonian => Some("hamiltonian"),
            Self::Mystery => None,
        }
    }

    /// Whether this is a held-out family (see [`Family::RESERVE`]).
    pub fn reserve(self) -> bool {
        Self::RESERVE.contains(&self)
    }
}

/// The held-out family of the hidden set with this seed: one of [`Family::RESERVE`], picked by
/// the seed, so it is as secret as the seed until the season is over.
pub fn mystery_family(seed: &str) -> Family {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("mystery:{seed}").as_bytes());
    let pick = u64::from_le_bytes(digest[..8].try_into().unwrap_or_default());
    let len = Family::RESERVE.len() as u64;
    Family::RESERVE[usize::try_from(pick % len).unwrap_or_default()]
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
    /// A wrong answer: UNSAT for a satisfiable formula, a model that doesn't satisfy it, or an
    /// UNSAT proof the checker rejects.
    Wrong,
    /// UNSAT without a proof the checker accepted: none was written, it was larger than the
    /// limit, or checking it ran out of time or memory. Counts as unsolved, like in the SAT
    /// Competition, but doesn't disqualify.
    Unverified,
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

impl RunResult {
    /// What is published of this run once the season is over (nothing that depends on the
    /// machine, like `wall_ms`).
    pub fn published(&self) -> InstanceResult {
        InstanceResult {
            name: self.name.clone(),
            family: self.family,
            expected: self.expected,
            outcome: self.outcome,
            fuel: self.fuel,
        }
    }
}

/// How a submission did on one formula of a set. Kept private by the website while the season
/// runs (the board shows families only), published per formula once it is archived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceResult {
    pub name: String,
    pub family: Family,
    pub expected: Answer,
    pub outcome: Outcome,
    /// Fuel used, the whole budget for a timeout.
    pub fuel: u64,
}

impl Outcome {
    /// A short lowercase name: `solved`, `unknown`, `timeout`, `crashed`, `wrong`,
    /// `unverified`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Solved(_) => "solved",
            Self::Unknown => "unknown",
            Self::Timeout => "timeout",
            Self::Crashed => "crashed",
            Self::Wrong => "wrong",
            Self::Unverified => "unverified",
        }
    }
}

impl Answer {
    /// `SAT` or `UNSAT`, as in `answers.txt`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sat => "SAT",
            Self::Unsat => "UNSAT",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "SAT" => Some(Self::Sat),
            "UNSAT" => Some(Self::Unsat),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyScore {
    /// [`Family::Mystery`] for the held-out family of a hidden set.
    pub family: Family,
    pub solved: u32,
    pub total: u32,
    /// Mean PAR-2 in billions of fuel units.
    pub par2: f64,
    /// UNSAT answers without an accepted proof (see [`Outcome::Unverified`]), 0 in scores
    /// from before proofs were needed.
    #[serde(default)]
    pub unverified: u32,
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
    /// UNSAT answers without an accepted proof, they count as unsolved.
    #[serde(default)]
    pub unverified: u32,
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
        let unverified = |rs: &mut dyn Iterator<Item = &RunResult>| {
            count(rs.filter(|r| r.outcome == Outcome::Unverified).count())
        };
        // The held-out family stays a mystery: its formulas count as `Mystery`
        let public = |r: &RunResult| {
            if r.family.reserve() {
                Family::Mystery
            } else {
                r.family
            }
        };
        let families = Family::DEV
            .into_iter()
            .chain([Family::Mystery])
            .filter_map(|family| {
                let rs: Vec<&RunResult> = results.iter().filter(|r| public(r) == family).collect();
                (!rs.is_empty()).then(|| FamilyScore {
                    family,
                    solved: count(rs.iter().filter(|r| r.outcome.solved()).count()),
                    total: count(rs.len()),
                    par2: par2(&rs),
                    unverified: unverified(&mut rs.iter().copied()),
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
            unverified: unverified(&mut results.iter()),
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
            Outcome::Unverified => FINGERPRINT_UNVERIFIED,
        })
        .collect()
}

pub const FINGERPRINT_TIMEOUT: u64 = u64::MAX;
pub const FINGERPRINT_GAVE_UP: u64 = u64::MAX - 1;
pub const FINGERPRINT_WRONG: u64 = u64::MAX - 2;
pub const FINGERPRINT_UNVERIFIED: u64 = u64::MAX - 3;

/// Whether a fingerprint entry is the fuel of a solved formula rather than a marker.
const fn fingerprint_solved(entry: u64) -> bool {
    entry < FINGERPRINT_UNVERIFIED
}

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
        let solved_a = fingerprint_solved(a);
        let solved_b = fingerprint_solved(b);
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
        /// The run of every formula, in set order. Private like the fingerprint until the
        /// season is archived. Empty from workers that predate it.
        #[serde(default)]
        results: Vec<InstanceResult>,
        /// The [`RULES_VERSION`] of the worker that computed the score, 0 from workers that
        /// predate it.
        #[serde(default)]
        rules_version: u32,
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
            run(Family::Queens, Outcome::Unverified, 7),
        ];
        let score = Score::from_results(&results, budget);
        assert_eq!(
            (score.solved, score.total, score.wrong, score.unverified),
            (1, 4, 1, 1)
        );
        // An UNSAT answer without an accepted proof counts like an unsolved formula
        assert!((score.par2 - (1.0 + 20.0 + 20.0 + 20.0) / 4.0).abs() < 1e-9);
        assert_eq!(score.families.len(), 2);
        assert!((score.families[0].par2 - 10.5).abs() < 1e-9);
        assert_eq!(
            score
                .families
                .iter()
                .map(|f| f.unverified)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    /// Scores stored before proofs were needed have no unverified counts.
    #[test]
    fn old_scores_load() {
        let old = r#"{"par2":1.0,"solved":1,"total":1,"wrong":0,"budget":10,"fuel":0.5,"families":[{"family":"Hashi","solved":1,"total":1,"par2":1.0}]}"#;
        let score: Score = serde_json::from_str(old).unwrap();
        assert_eq!((score.unverified, score.families[0].unverified), (0, 0));
    }

    #[test]
    fn unverified_answers_leave_their_own_fingerprint() {
        let results = [
            run(Family::Hashi, Outcome::Unverified, 7),
            run(Family::Hashi, Outcome::Unknown, 7),
        ];
        let print = fingerprint(&results);
        assert_eq!(print, [FINGERPRINT_UNVERIFIED, FINGERPRINT_GAVE_UP]);
        // Markers aren't fuel: the same markers alone don't make a copy, other ones rule it out
        let solved: Vec<u64> = (1..=8).map(|i| i * 1000).collect();
        let a = [solved.clone(), vec![FINGERPRINT_UNVERIFIED]].concat();
        let b = [solved, vec![FINGERPRINT_GAVE_UP]].concat();
        assert!(similar(&a, &a));
        assert!(!similar(&a, &b));
    }

    #[test]
    fn ids() {
        let outcomes = [
            Outcome::Solved(Answer::Unsat),
            Outcome::Unknown,
            Outcome::Timeout,
            Outcome::Crashed,
            Outcome::Wrong,
            Outcome::Unverified,
        ];
        assert_eq!(
            outcomes.map(Outcome::id),
            [
                "solved",
                "unknown",
                "timeout",
                "crashed",
                "wrong",
                "unverified"
            ]
        );
        for answer in [Answer::Sat, Answer::Unsat] {
            assert_eq!(Answer::from_id(answer.id()), Some(answer));
        }
        assert_eq!(Answer::from_id("sat"), None);
        let run = run(Family::Queens, Outcome::Timeout, 7);
        assert_eq!(
            run.published(),
            InstanceResult {
                name: String::new(),
                family: Family::Queens,
                expected: Answer::Sat,
                outcome: Outcome::Timeout,
                fuel: 7
            }
        );
    }
}
