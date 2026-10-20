//! Benchmark sets, generated from a seed.
//!
//! Every answer is known for certain, backed by a model, a construction argument or a checked
//! proof:
//!
//! - Hashi: satisfiable, the generator's solution is a model.
//! - Hashi parity: unsatisfiable, the island numbers add up to an odd number.
//! - Pigeonhole: n+1 pigeons in n holes is unsatisfiable, n in n is not.
//! - Factoring, miters, colouring, Tseitin, quasigroups and Hamiltonian cycles: see
//!   [`crate::families`], a planted model or a mathematical reason.
//! - Queens and random 3-SAT: solved by splr beforehand. SAT comes with a model, which is
//!   checked. UNSAT must be proven: splr runs again writing a DRAT proof, which drat-trim and
//!   `cake_lpr` verify when they are installed (like on the leaderboard), our own checker
//!   ([`crate::drat`]) otherwise. If the proof doesn't check out, a model found by our CDCL makes
//!   the formula SAT, otherwise it is skipped, like instances splr can't solve.
//!   Models are always checked anyway, labels only decide about UNSAT answers.
//!
//! The development set ([`DEV_SPEC`]) is public. A season's hidden set ([`hidden_spec`]) uses a
//! secret seed, so its formulas can't be known in advance; it has larger sizes than the dev set
//! and one held-out family that no dev set has, so a solver tuned to the dev set has to
//! generalise. All formulas are scrambled (variables renamed and negated, clauses and literals
//! shuffled) so a family can't be recognised by its layout, and solvers read them without a
//! name (see `eval::HEADER`).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use backend::sat::{SatOutcome, solve_clauses};
use common::{Difficulty, GenerateParams, Puzzle, SolverConfig, SolverKind};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use sha2::{Digest, Sha256};
use splr::{Certificate, Config, SatSolverIF, SolveIF, Solver, SolverError};

use crate::cnf::{Cnf, literals};
use crate::community::{PoolInstance, Refusal, Verdict};
use crate::{Answer, Family};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Instance {
    /// e.g. `hashi-03`, unique in a set.
    pub name: String,
    pub family: Family,
    pub expected: Answer,
    pub cnf: Cnf,
    /// A model, for satisfiable instances.
    pub witness: Option<Vec<i32>>,
    /// Who contributed it, for benchmarks from the community pool ([`Family::Community`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<crate::community::Origin>,
}

/// One formula to generate: its family, a size (what it means depends on the family, see
/// [`generate_one`]) and the answer it should have (`None`: whatever comes out).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub family: Family,
    pub size: u32,
    pub want: Option<Answer>,
}

const fn sat(family: Family, size: u32) -> Spec {
    Spec {
        family,
        size,
        want: Some(Answer::Sat),
    }
}

const fn unsat(family: Family, size: u32) -> Spec {
    Spec {
        family,
        size,
        want: Some(Answer::Unsat),
    }
}

#[cfg(test)]
const fn any(family: Family, size: u32) -> Spec {
    Spec {
        family,
        size,
        want: None,
    }
}

/// The development set: every family of [`Family::DEV`], close to as many SAT as UNSAT
/// formulas, sizes from easy for any solver to out of reach for a simple DPLL.
pub const DEV_SPEC: &[Spec] = &[
    sat(Family::Hashi, 12),
    sat(Family::Hashi, 16),
    sat(Family::Hashi, 20),
    sat(Family::Hashi, 24),
    unsat(Family::HashiParity, 8),
    unsat(Family::HashiParity, 12),
    unsat(Family::HashiParity, 16),
    unsat(Family::HashiParity, 20),
    sat(Family::Random3Sat, 150),
    unsat(Family::Random3Sat, 150),
    sat(Family::Random3Sat, 200),
    unsat(Family::Random3Sat, 200),
    unsat(Family::Pigeonhole, 6),
    unsat(Family::Pigeonhole, 7),
    unsat(Family::Pigeonhole, 8),
    sat(Family::Pigeonhole, 20),
    sat(Family::Queens, 16),
    sat(Family::Queens, 32),
    sat(Family::Queens, 48),
    sat(Family::Factoring, 16),
    unsat(Family::Factoring, 18),
    sat(Family::Factoring, 20),
    unsat(Family::Factoring, 22),
    sat(Family::Factoring, 24),
    unsat(Family::Factoring, 24),
    unsat(Family::Miter, 4),
    sat(Family::Miter, 6),
    unsat(Family::Miter, 5),
    sat(Family::Miter, 7),
    unsat(Family::Miter, 6),
    sat(Family::Coloring, 80),
    unsat(Family::Coloring, 120),
    sat(Family::Coloring, 120),
    unsat(Family::Coloring, 200),
    sat(Family::Coloring, 140),
    unsat(Family::Coloring, 240),
];

/// What a hidden set has beyond the dev set's sizes: one larger formula of every dev family.
pub const BEYOND: &[Spec] = &[
    sat(Family::Hashi, 28),
    unsat(Family::HashiParity, 24),
    unsat(Family::Random3Sat, 225),
    unsat(Family::Pigeonhole, 9),
    sat(Family::Queens, 64),
    unsat(Family::Factoring, 26),
    sat(Family::Miter, 8),
    sat(Family::Coloring, 160),
];

const TSEITIN: &[Spec] = &[
    sat(Family::Tseitin, 400),
    unsat(Family::Tseitin, 16),
    unsat(Family::Tseitin, 20),
    unsat(Family::Tseitin, 22),
];

const QUASIGROUP: &[Spec] = &[
    sat(Family::Quasigroup, 20),
    sat(Family::Quasigroup, 25),
    sat(Family::Quasigroup, 28),
    sat(Family::Quasigroup, 30),
];

const HAMILTONIAN: &[Spec] = &[
    sat(Family::Hamiltonian, 30),
    unsat(Family::Hamiltonian, 24),
    sat(Family::Hamiltonian, 40),
    unsat(Family::Hamiltonian, 28),
];

/// The formulas of a held-out family in a hidden set.
pub fn mystery_spec(family: Family) -> &'static [Spec] {
    match family {
        Family::Tseitin => TSEITIN,
        Family::Quasigroup => QUASIGROUP,
        Family::Hamiltonian => HAMILTONIAN,
        _ => &[],
    }
}

/// A season's hidden set: the dev families at the dev set's sizes and [`BEYOND`] them, then the
/// formulas of the mystery family that the seed picks (see [`crate::mystery_family`]).
pub fn hidden_spec(seed: &str) -> Vec<Spec> {
    Family::DEV
        .into_iter()
        .flat_map(|family| {
            DEV_SPEC
                .iter()
                .chain(BEYOND)
                .filter(move |s| s.family == family)
                .copied()
        })
        .chain(mystery_spec(crate::mystery_family(seed)).iter().copied())
        .collect()
}

/// The random generator for one instance: same seed and index, same instance.
fn rng_for(seed: &str, index: usize, attempt: u32) -> StdRng {
    let digest = Sha256::digest(format!("{seed}/{index}/{attempt}"));
    StdRng::from_seed(digest.into())
}

/// The development set's formulas for `seed` (see [`DEV_SPEC`]), any seed but a season's.
pub fn generate(seed: &str) -> Vec<Instance> {
    generate_with(seed, DEV_SPEC)
}

/// The hidden set of a season with this seed (see [`hidden_spec`]).
pub fn generate_hidden(seed: &str) -> Vec<Instance> {
    generate_with(seed, &hidden_spec(seed))
}

/// A season's hidden set with the benchmarks drawn from the community pool for it (see
/// [`community`]).
pub fn generate_hidden_with(seed: &str, pool: &[PoolInstance]) -> Vec<Instance> {
    let mut set = generate_hidden(seed);
    set.extend(community(seed, pool));
    set
}

/// The community benchmarks of a hidden set, `community-01` and on.
///
/// Each is scrambled with a random generator of the set's seed and its hash: in every season a
/// known benchmark looks different, so a solver can't recognise it by its layout or carry a
/// model over.
pub fn community(seed: &str, pool: &[PoolInstance]) -> Vec<Instance> {
    pool.iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let cnf = match Cnf::parse(&p.dimacs) {
                Ok(cnf) => cnf,
                Err(e) => {
                    eprintln!("community benchmark #{}: {e}", p.origin.benchmark);
                    return None;
                }
            };
            let digest = Sha256::digest(format!("{seed}/community/{}", p.origin.hash));
            let mut rng = StdRng::from_seed(digest.into());
            let (cnf, map) = cnf.scramble(&mut rng);
            let witness = p.model.as_ref().map(|w| {
                w.iter()
                    .map(|&l| {
                        let renamed = map[l.unsigned_abs() as usize];
                        if l > 0 { renamed } else { -renamed }
                    })
                    .collect()
            });
            Some(Instance {
                name: format!("{}-{:02}", Family::Community.id(), i + 1),
                family: Family::Community,
                expected: p.answer,
                cnf,
                witness,
                origin: Some(p.origin.clone()),
            })
        })
        .collect()
}

/// The worker's check of a community benchmark.
///
/// Refused if a plain DPLL solver settles it within [`crate::community::EASY_SECS`] (too
/// easy, like the SAT Competition's "`MiniSat` solves it in a minute"), verified if splr finds a
/// model that checks out or proves it unsatisfiable with a proof the labelling checker accepts
/// (see [`label`]), refused otherwise.
pub fn verify_benchmark(cnf: &Cnf) -> Verdict {
    // The house DPLL entry's solver
    let dpll = SolverConfig {
        kind: SolverKind::Dpll,
        ..Default::default()
    }
    .normalized();
    let easy = Instant::now() + Duration::from_secs(crate::community::EASY_SECS);
    if let Ok((outcome, _)) = solve_clauses(&cnf.clauses, cnf.vars as usize, &dpll, Some(easy))
        && !matches!(outcome, SatOutcome::Timeout)
    {
        return Verdict::Refused {
            reason: Refusal::TooEasy,
        };
    }
    let started = Instant::now();
    match label(cnf) {
        Some((answer, model)) => Verdict::Verified {
            answer,
            model,
            solve_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        None => Verdict::Refused {
            reason: Refusal::NoAnswer,
        },
    }
}

/// Instances are independent (each has its own random generator), so they are generated in
/// parallel.
pub fn generate_with(seed: &str, spec: &[Spec]) -> Vec<Instance> {
    let mut counters = std::collections::HashMap::new();
    let names: Vec<String> = spec
        .iter()
        .map(|s| {
            let n = counters.entry(s.family).or_insert(0u32);
            *n += 1;
            format!("{}-{:02}", s.family.id(), n)
        })
        .collect();
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Instance>>> =
        spec.iter().map(|_| std::sync::Mutex::default()).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads.min(spec.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&one) = spec.get(index) else {
                        break;
                    };
                    let instance = generate_instance(seed, index, names[index].clone(), one);
                    *slots[index]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(instance);
                }
            });
        }
    });
    slots
        .into_iter()
        .filter_map(|slot| {
            slot.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .collect()
}

fn generate_instance(seed: &str, index: usize, name: String, spec: Spec) -> Instance {
    assert!(
        !matches!(spec.family, Family::Mystery | Family::Community),
        "only generated families"
    );
    let mut attempt = 0;
    loop {
        let mut rng = rng_for(seed, index, attempt);
        if let Some((cnf, expected, witness)) = generate_one(spec, &mut rng)
            && spec.want.is_none_or(|want| want == expected)
        {
            let (cnf, map) = cnf.scramble(&mut rng);
            let witness = witness.map(|w| {
                w.iter()
                    .map(|&l| {
                        let renamed = map[l.unsigned_abs() as usize];
                        if l > 0 { renamed } else { -renamed }
                    })
                    .collect()
            });
            return Instance {
                name,
                family: spec.family,
                expected,
                cnf,
                witness,
                origin: None,
            };
        }
        attempt += 1;
    }
}

pub type Generated = (Cnf, Answer, Option<Vec<i32>>);

/// One formula; `None` to try again with another random draw. Sizes:
///
/// - Hashi, Hashi parity: width and height of the puzzle
/// - random 3-SAT: variables
/// - pigeonhole: holes (for SAT as many pigeons, otherwise one more; without a wanted answer
///   holes up to 11 get one more)
/// - queens: the board's width
/// - factoring: bits of the number, miter: bits of the multipliers' operands
/// - colouring, Tseitin, Hamiltonian: vertices; quasigroup: the square's order
///
/// Families whose answer comes out of a solver (random 3-SAT, queens) may give another answer
/// than `spec.want`, the caller draws again then.
pub fn generate_one(spec: Spec, rng: &mut StdRng) -> Option<Generated> {
    use crate::families::{coloring, factoring, hamiltonian, miter, quasigroup, tseitin};
    let size = spec.size;
    let want = spec.want.unwrap_or_else(|| {
        if rng.random_bool(0.5) {
            Answer::Sat
        } else {
            Answer::Unsat
        }
    });
    match spec.family {
        Family::Hashi => hashi(size, rng, false),
        Family::HashiParity => hashi(size, rng, true),
        Family::Random3Sat => random_3sat(size, rng),
        Family::Pigeonhole => Some(pigeonhole(
            size,
            spec.want.map_or(size >= 12, |w| w == Answer::Sat),
        )),
        Family::Queens => queens(size, rng),
        Family::Factoring => factoring::generate(size, want, rng),
        Family::Miter => miter::generate(size, want, rng),
        Family::Coloring => coloring::generate(size, want, rng),
        Family::Tseitin => tseitin::generate(size, want, rng),
        Family::Quasigroup => quasigroup::generate(size, rng),
        Family::Hamiltonian => hamiltonian::generate(size, want, rng),
        // Not generated: the mystery family stands for another, community benchmarks are given
        Family::Mystery | Family::Community => None,
    }
}

fn difficulty_for(size: u32) -> Difficulty {
    match size {
        0..=7 => Difficulty::Easy,
        8..=11 => Difficulty::Medium,
        12..=15 => Difficulty::Hard,
        16..=21 => Difficulty::Expert,
        _ => Difficulty::Oni,
    }
}

/// A generated puzzle as CNF (the static encoding, without the connectivity rule). With
/// `parity`, one island's number moves by one: the sum turns odd, which no set of bridges can
/// produce since each bridge adds two.
fn hashi(size: u32, rng: &mut StdRng, parity: bool) -> Option<Generated> {
    let size = size.clamp(5, 30) as u8;
    let generated = backend::api::generate(&GenerateParams {
        width: size,
        height: size,
        difficulty: difficulty_for(u32::from(size)),
        seed: rng.random(),
    });
    let mut puzzle: Puzzle = generated.puzzle;
    let (board, encoding) = backend::encoding::encode_puzzle(&puzzle);
    if !parity {
        let mut model = vec![false; encoding.num_vars + 1];
        for (edge, &count) in generated.solution.iter().enumerate() {
            for k in 1..=count {
                let v = usize::try_from(backend::encoding::var(edge, k))
                    .expect("variables are positive");
                model[v] = true;
            }
        }
        let vars = u32::try_from(encoding.num_vars).expect("a puzzle has far fewer variables");
        let cnf = Cnf::new(vars, encoding.clauses);
        let witness = literals(&model);
        return (cnf.check(&witness) == Some(true)).then_some((cnf, Answer::Sat, Some(witness)));
    }
    // Islands whose number can move without becoming trivially impossible
    let candidates: Vec<(usize, u8)> = puzzle
        .islands
        .iter()
        .enumerate()
        .flat_map(|(i, island)| {
            // At most four neighbours, so this always fits
            let most = u8::try_from((2 * board.incident[i].len()).min(8)).unwrap_or(8);
            let up = (island.required < most).then_some((i, island.required + 1));
            let down = (island.required > 1).then_some((i, island.required - 1));
            [up, down].into_iter().flatten()
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let (i, required) = candidates[rng.random_range(0..candidates.len())];
    let pos = puzzle.islands[i].pos;
    puzzle.set(pos, required);
    let sum: u32 = puzzle.islands.iter().map(|i| u32::from(i.required)).sum();
    debug_assert_eq!(sum % 2, 1);
    let (_, encoding) = backend::encoding::encode_puzzle(&puzzle);
    let vars = u32::try_from(encoding.num_vars).expect("a puzzle has far fewer variables");
    Some((Cnf::new(vars, encoding.clauses), Answer::Unsat, None))
}

/// Solved by splr (with its clause eliminator, which keeps it sound on our formulas). SAT needs a
/// model that checks out, UNSAT a DRAT proof that does (see [`proven_unsat`]). Should splr's
/// UNSAT not be provable, our CDCL gets a few seconds to find a model.
fn label(cnf: &Cnf) -> Option<(Answer, Option<Vec<i32>>)> {
    let deadline = |secs| Some(Instant::now() + Duration::from_secs(secs));
    let checked = |model: Vec<bool>| {
        let witness = literals(&model);
        (cnf.check(&witness) == Some(true)).then_some((Answer::Sat, Some(witness)))
    };
    let splr = SolverConfig::splr();
    match solve_clauses(&cnf.clauses, cnf.vars as usize, &splr, deadline(60))
        .ok()?
        .0
    {
        SatOutcome::Sat(model) => checked(model),
        SatOutcome::Unsat if proven_unsat(cnf) => Some((Answer::Unsat, None)),
        SatOutcome::Unsat => {
            let cdcl = SolverConfig {
                kind: SolverKind::Cdcl,
                ..Default::default()
            };
            match solve_clauses(&cnf.clauses, cnf.vars as usize, &cdcl, deadline(10))
                .ok()?
                .0
            {
                SatOutcome::Sat(model) => checked(model),
                SatOutcome::Unsat | SatOutcome::Timeout => None,
            }
        }
        SatOutcome::Timeout => None,
    }
}

/// How long splr may take to write a proof.
const PROOF_TIMEOUT: Duration = Duration::from_secs(120);

/// Whether splr, run again with certification, proves the formula unsatisfiable with a DRAT
/// proof that the checker accepts (see [`crate::proof::labelling`]).
fn proven_unsat(cnf: &Cnf) -> bool {
    let Ok(dir) = crate::proof::TempDir::new(&std::env::temp_dir(), "dojo-label") else {
        return false;
    };
    let path = dir.path().join(crate::proof::PROOF_FILE);
    write_splr_proof(cnf, &path)
        && crate::proof::labelling().check(cnf, &path, crate::proof::Format::Drat)
            == crate::proof::Verdict::Verified
}

/// The DRAT proof (text) that splr writes when it finds the formula unsatisfiable; `None` if it
/// doesn't (or fails). For tests.
#[cfg(test)]
pub(crate) fn splr_proof(cnf: &Cnf) -> Option<Vec<u8>> {
    let dir = crate::proof::TempDir::new(&std::env::temp_dir(), "dojo-splr").ok()?;
    let path = dir.path().join("proof.drat");
    write_splr_proof(cnf, &path).then(|| std::fs::read(&path).unwrap_or_default())
}

/// Runs splr with certification into the file `path` (splr can only write proofs to files):
/// whether it found the formula unsatisfiable. The file is created even when splr writes
/// nothing (an empty proof).
pub(crate) fn write_splr_proof(cnf: &Cnf, path: &Path) -> bool {
    // Like `backend::sat`: the clause eliminator stays on (the default), splr only has whole
    // seconds as timeout
    let config = Config {
        quiet_mode: true,
        use_certification: true,
        io_pfile: path.to_path_buf(),
        c_timeout: PROOF_TIMEOUT.as_secs_f64(),
        ..Default::default()
    };
    // splr trips debug assertions on some formulas (debug builds only), see `backend::sat`
    let unsat = catch_unwind(AssertUnwindSafe(|| {
        match Solver::try_from((config, cnf.clauses.as_slice())) {
            Ok(mut solver) => match solver.solve() {
                Ok(Certificate::UNSAT)
                | Err(SolverError::EmptyClause | SolverError::RootLevelConflict(_)) => {
                    // Writes what is left of the proof and the final empty clause
                    solver.save_certification();
                    true
                }
                _ => false,
            },
            // A conflict while loading the clauses, splr writes no proof then: the checker
            // accepts the (empty or partial) file only if unit propagation refutes the formula.
            // splr reports most of these as errors (found by tests/props.rs).
            Err(
                Ok(Certificate::UNSAT)
                | Err(SolverError::EmptyClause | SolverError::RootLevelConflict(_)),
            ) => true,
            Err(_) => false,
        }
    }))
    .unwrap_or(false);
    // No file is an empty proof
    unsat && (path.exists() || std::fs::File::create(path).is_ok())
}

/// `n` variables, 4.26·n clauses of three distinct variables: about half are satisfiable.
fn random_3sat(n: u32, rng: &mut StdRng) -> Option<Generated> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "4.26 times a u32 is non-negative and far below usize::MAX"
    )]
    let m = (f64::from(n) * 4.26).round() as usize;
    let max = i32::try_from(n).expect("the size is a small variable count");
    let clauses = (0..m)
        .map(|_| {
            let mut vars: Vec<i32> = Vec::with_capacity(3);
            while vars.len() < 3 {
                let v = rng.random_range(1..=max);
                if !vars.contains(&v) {
                    vars.push(v);
                }
            }
            vars.into_iter()
                .map(|v| if rng.random_bool(0.5) { v } else { -v })
                .collect()
        })
        .collect();
    let cnf = Cnf::new(n, clauses);
    let (answer, witness) = label(&cnf)?;
    Some((cnf, answer, witness))
}

/// `holes + 1` pigeons in `holes` holes (unsatisfiable), or as many pigeons as holes.
pub(crate) fn pigeonhole(holes: u32, satisfiable: bool) -> Generated {
    let pigeons = if satisfiable { holes } else { holes + 1 };
    let var = |p: u32, h: u32| {
        i32::try_from(p * holes + h + 1).expect("the size is a small variable count")
    };
    let mut clauses: Vec<Vec<i32>> = (0..pigeons)
        .map(|p| (0..holes).map(|h| var(p, h)).collect())
        .collect();
    for h in 0..holes {
        for p in 0..pigeons {
            for q in p + 1..pigeons {
                clauses.push(vec![-var(p, h), -var(q, h)]);
            }
        }
    }
    let cnf = Cnf::new(pigeons * holes, clauses);
    if satisfiable {
        let witness = (0..pigeons)
            .flat_map(|p| (0..holes).map(move |h| if p == h { var(p, h) } else { -var(p, h) }))
            .collect();
        (cnf, Answer::Sat, Some(witness))
    } else {
        (cnf, Answer::Unsat, None)
    }
}

/// N queens: one per row, at most one per column and diagonal. A few random cells are
/// forbidden to vary the instances.
fn queens(n: u32, rng: &mut StdRng) -> Option<Generated> {
    let var =
        |r: u32, c: u32| i32::try_from(r * n + c + 1).expect("the size is a small variable count");
    let mut clauses: Vec<Vec<i32>> = (0..n)
        .map(|r| (0..n).map(|c| var(r, c)).collect())
        .collect();
    let mut at_most_one = |cells: Vec<(u32, u32)>| {
        for i in 0..cells.len() {
            for j in i + 1..cells.len() {
                clauses.push(vec![
                    -var(cells[i].0, cells[i].1),
                    -var(cells[j].0, cells[j].1),
                ]);
            }
        }
    };
    for r in 0..n {
        at_most_one((0..n).map(|c| (r, c)).collect());
    }
    for c in 0..n {
        at_most_one((0..n).map(|r| (r, c)).collect());
    }
    for d in 0..2 * n - 1 {
        at_most_one(
            (0..n)
                .filter_map(|r| d.checked_sub(r).filter(|&c| c < n).map(|c| (r, c)))
                .collect(),
        );
        at_most_one(
            (0..n)
                .filter_map(|r| {
                    (d + r)
                        .checked_sub(n - 1)
                        .filter(|&c| c < n)
                        .map(|c| (r, c))
                })
                .collect(),
        );
    }
    for _ in 0..n / 4 {
        clauses.push(vec![-var(rng.random_range(0..n), rng.random_range(0..n))]);
    }
    let cnf = Cnf::new(n * n, clauses);
    let (answer, witness) = label(&cnf)?;
    Some((cnf, answer, witness))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Checks a label with splr: SAT if it finds a model, UNSAT only with a DRAT proof that the
    /// labelling checker accepts.
    pub(crate) fn assert_label(cnf: &Cnf, expected: Answer, what: &str) {
        let outcome = solve_clauses(&cnf.clauses, cnf.vars as usize, &SolverConfig::splr(), None)
            .unwrap()
            .0;
        match outcome {
            SatOutcome::Sat(model) => {
                assert_eq!(cnf.check(&literals(&model)), Some(true), "{what}");
                assert_eq!(expected, Answer::Sat, "{what}: splr found a model");
            }
            SatOutcome::Unsat => {
                assert_eq!(expected, Answer::Unsat, "{what}: splr says UNSAT");
                assert!(proven_unsat(cnf), "{what}: no checked proof");
            }
            SatOutcome::Timeout => panic!("{what}: splr timed out"),
        }
    }

    #[test]
    fn spec_sizes() {
        assert_eq!(DEV_SPEC.len(), crate::DEV_SET_SIZE as usize);
        for family in Family::RESERVE {
            let spec = hidden_spec(&format!("seed for {}", family.id()));
            assert_eq!(spec.len(), crate::SET_SIZE as usize);
            assert_eq!(mystery_spec(family).len(), 4);
        }
        let none: &[Spec] = &[];
        assert_eq!(mystery_spec(Family::Hashi), none);
        // Every dev family, nothing else, in the dev set
        let families: std::collections::BTreeSet<Family> =
            DEV_SPEC.iter().map(|s| s.family).collect();
        assert_eq!(families.into_iter().collect::<Vec<_>>(), Family::DEV);
        let wants = |answer| DEV_SPEC.iter().filter(|s| s.want == Some(answer)).count();
        // As many SAT as UNSAT formulas
        assert_eq!(wants(Answer::Sat), wants(Answer::Unsat));
        assert!(DEV_SPEC.iter().all(|s| s.want.is_some()));
    }

    /// The hidden set holds the dev families at the dev sizes and beyond, plus one held-out
    /// family that the seed picks, which the dev set never has.
    #[test]
    fn hidden_sets_hold_a_mystery_family() {
        let mut mysteries = std::collections::BTreeSet::new();
        for i in 0..40 {
            let seed = format!("season seed {i}");
            let spec = hidden_spec(&seed);
            let mystery = crate::mystery_family(&seed);
            assert!(mystery.reserve());
            mysteries.insert(mystery);
            let held_out: Vec<&Spec> = spec.iter().filter(|s| s.family.reserve()).collect();
            assert_eq!(held_out.len(), 4);
            assert!(held_out.iter().all(|s| s.family == mystery));
            assert!(!DEV_SPEC.iter().any(|s| s.family.reserve()));
            for family in Family::DEV {
                assert!(spec.iter().any(|s| s.family == family), "{family:?}");
            }
            assert_eq!(spec.len(), DEV_SPEC.len() + BEYOND.len() + 4);
        }
        // Each formula beyond the dev set is larger than any of the dev set's of its family and
        // answer (colourings with a Mycielski graph are easier than planted ones of their size)
        for beyond in BEYOND {
            let largest = DEV_SPEC
                .iter()
                .filter(|s| s.family == beyond.family && s.want == beyond.want)
                .map(|s| s.size)
                .max()
                .unwrap_or_default();
            assert!(beyond.size > largest, "{beyond:?}");
        }
        assert_eq!(BEYOND.len(), Family::DEV.len());
        // Every held-out family gets its turn
        assert_eq!(mysteries.len(), Family::RESERVE.len());
        assert_eq!(crate::mystery_family("x"), crate::mystery_family("x"));
    }

    #[test]
    fn small_set_is_labelled_correctly() {
        let spec = [
            sat(Family::Hashi, 7),
            unsat(Family::HashiParity, 5),
            sat(Family::Random3Sat, 40),
            unsat(Family::Random3Sat, 40),
            unsat(Family::Pigeonhole, 4),
            any(Family::Pigeonhole, 12),
            any(Family::Queens, 8),
            sat(Family::Factoring, 10),
            unsat(Family::Factoring, 10),
            unsat(Family::Miter, 3),
            sat(Family::Miter, 3),
            sat(Family::Coloring, 30),
            unsat(Family::Coloring, 30),
            sat(Family::Tseitin, 8),
            unsat(Family::Tseitin, 8),
            sat(Family::Quasigroup, 6),
            sat(Family::Hamiltonian, 8),
            unsat(Family::Hamiltonian, 8),
        ];
        let set = generate_with("test", &spec);
        assert_eq!(set.len(), spec.len());
        for (instance, spec) in set.iter().zip(&spec) {
            assert_eq!(instance.family, spec.family);
            assert!(spec.want.is_none_or(|w| w == instance.expected));
            assert_eq!(instance.witness.is_some(), instance.expected == Answer::Sat);
            if let Some(w) = &instance.witness {
                assert_eq!(instance.cnf.check(w), Some(true), "{}", instance.name);
            }
            // Cross-check every label with splr, and its proofs
            assert_label(&instance.cnf, instance.expected, &instance.name);
        }
        assert_eq!(set[4].expected, Answer::Unsat);
        assert_eq!(set[5].expected, Answer::Sat);
        // Same seed, same formulas
        let again = generate_with("test", &spec);
        assert!(set.iter().zip(&again).all(|(a, b)| a.cnf == b.cnf));
        assert_ne!(generate_with("other", &spec[..1])[0].cnf, set[0].cnf);
    }

    /// Community benchmarks join a hidden set scrambled, credited and with their models carried
    /// over; another season scrambles them differently.
    #[test]
    fn community_benchmarks_join_hidden_sets() {
        use crate::community::{Formula, Origin, gbd_hash};
        let (cnf, answer, model) = pigeonhole(4, true);
        let dimacs = Formula {
            vars: cnf.vars,
            clauses: cnf.clauses.clone(),
        }
        .dimacs();
        let pool = vec![PoolInstance {
            origin: Origin {
                benchmark: 5,
                hash: gbd_hash(&Formula::parse(&dimacs).unwrap()),
                family: "pigeons".into(),
                author: "ada".into(),
            },
            answer,
            dimacs,
            model,
        }];
        let set = community("season", &pool);
        assert_eq!(set.len(), 1);
        let i = &set[0];
        assert_eq!(
            (i.name.as_str(), i.family),
            ("community-01", Family::Community)
        );
        assert_eq!(i.origin.as_ref().map(|o| o.benchmark), Some(5));
        assert_ne!(i.cnf, cnf, "scrambled");
        assert_eq!(i.cnf.check(i.witness.as_ref().unwrap()), Some(true));
        assert_ne!(community("another season", &pool)[0].cnf, i.cnf);
        // A broken one is left out
        let mut broken = pool;
        broken[0].dimacs = "nonsense".into();
        assert!(community("season", &broken).is_empty());
    }

    /// The "interesting" filter: what plain DPLL settles at once is too easy; otherwise splr
    /// settles it with a checked model or proof.
    #[test]
    fn benchmarks_are_verified() {
        let easy = Cnf::new(2, vec![vec![1, 2], vec![-1]]);
        assert_eq!(
            verify_benchmark(&easy),
            Verdict::Refused {
                reason: Refusal::TooEasy
            }
        );
        let spec = [unsat(Family::Factoring, 22), sat(Family::Factoring, 22)];
        for instance in generate_with("verify", &spec) {
            match verify_benchmark(&instance.cnf) {
                Verdict::Verified { answer, model, .. } => {
                    assert_eq!(answer, instance.expected);
                    assert_eq!(model.is_some(), answer == Answer::Sat);
                    if let Some(m) = model {
                        assert_eq!(instance.cnf.check(&m), Some(true));
                    }
                }
                other => panic!("{}: {other:?}", instance.name),
            }
        }
    }

    /// The formulas of a seed, checked in as digests. Workers generate a season's hidden set
    /// from its seed when they don't have it cached (a new volume, a cleared cache), so the
    /// same seed has to give the same formulas with every version of the worker.
    #[test]
    fn sets_are_unchanged() {
        const SETS_CHANGE: &str = "the same seed gives other formulas now (rand, splr, sha2, \
                                   the generator or the families changed): a worker without \
                                   the set cached would evaluate new submissions on another \
                                   hidden set than the earlier ones of the season. Bump \
                                   GENERATOR_VERSION and RULES_VERSION with it.";
        let mut rng = rng_for("golden", 3, 1);
        assert_eq!(
            rng.random::<u64>(),
            14_952_121_534_598_797_468,
            "{SETS_CHANGE}"
        );
        let spec = [
            sat(Family::Hashi, 7),
            unsat(Family::HashiParity, 5),
            any(Family::Random3Sat, 20),
            unsat(Family::Pigeonhole, 4),
            any(Family::Queens, 8),
            sat(Family::Factoring, 12),
            unsat(Family::Factoring, 12),
            sat(Family::Miter, 4),
            unsat(Family::Miter, 4),
            sat(Family::Coloring, 30),
            unsat(Family::Coloring, 30),
            sat(Family::Tseitin, 10),
            unsat(Family::Tseitin, 10),
            sat(Family::Quasigroup, 8),
            sat(Family::Hamiltonian, 10),
            unsat(Family::Hamiltonian, 10),
        ];
        let digests: Vec<String> = generate_with("golden", &spec)
            .iter()
            .map(|i| {
                let digest = Sha256::new()
                    .chain_update(i.family.id())
                    .chain_update(format!("{:?} {:?}", i.expected, i.witness))
                    .chain_update(i.cnf.dimacs(""))
                    .finalize();
                let first = u64::from_be_bytes(digest[..8].try_into().unwrap());
                format!("{} {first:016x}", i.name)
            })
            .collect();
        assert_eq!(
            digests,
            [
                "hashi-01 cc0dba5ae2ccc1ce",
                "hashi-parity-01 27ec251a8ceb744b",
                "random-3sat-01 00f005fc63ee3235",
                "pigeonhole-01 20cb5b50acf516fe",
                "queens-01 c50d33316a0e0e32",
                "factoring-01 bed852b6710c5ef7",
                "factoring-02 21af9886b01b435a",
                "miter-01 e241be0f89d49f51",
                "miter-02 892dc275d1310581",
                "coloring-01 ea5dc9de5d443a4a",
                "coloring-02 a549083e26a45424",
                "tseitin-01 db32d375fbc93dd3",
                "tseitin-02 dea0f305f461e2ca",
                "quasigroup-01 fe07cd9748f8e6d0",
                "hamiltonian-01 706b3fdcc7bb368d",
                "hamiltonian-02 e01937be30e7af1d",
            ],
            "{SETS_CHANGE}"
        );
        assert_eq!(
            crate::mystery_family("golden"),
            Family::Quasigroup,
            "{SETS_CHANGE}"
        );
    }
}
