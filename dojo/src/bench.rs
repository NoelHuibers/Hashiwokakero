//! Benchmark sets, generated from a seed.
//!
//! Every answer is known for certain, backed by a model or by a checked proof:
//!
//! - Hashi: satisfiable, the generator's solution is a model.
//! - Hashi parity: unsatisfiable, the island numbers add up to an odd number.
//! - Pigeonhole: n+1 pigeons in n holes is unsatisfiable, n in n is not.
//! - Queens and random 3-SAT: solved by splr beforehand. SAT comes with a model, which is
//!   checked. UNSAT must be proven: splr runs again writing a DRAT proof, which our own checker
//!   ([`crate::drat`]) verifies. If the proof doesn't check out, a model found by our CDCL makes
//!   the formula SAT, otherwise it is skipped, like instances splr can't solve.
//!   Models are always checked anyway, labels only decide about UNSAT answers.
//!
//! The hidden test set uses a secret seed that changes every season, so its formulas can't be
//! known in advance, and all formulas are scrambled (variables renamed and negated, clauses and
//! literals shuffled) so a family can't be recognised by its layout.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use backend::sat::{SatOutcome, solve_clauses};
use common::{Difficulty, GenerateParams, Puzzle, SolverConfig, SolverKind};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use sha2::{Digest, Sha256};
use splr::{Certificate, Config, SatSolverIF, SolveIF, Solver, SolverError};

use crate::cnf::{Cnf, literals};
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
}

/// What to generate, in order. The numbers are sizes, see [`generate_one`].
pub const SPEC: &[(Family, u32)] = &[
    (Family::Hashi, 10),
    (Family::Hashi, 12),
    (Family::Hashi, 14),
    (Family::Hashi, 16),
    (Family::Hashi, 18),
    (Family::Hashi, 20),
    (Family::Hashi, 24),
    (Family::Hashi, 28),
    (Family::HashiParity, 5),
    (Family::HashiParity, 6),
    (Family::HashiParity, 7),
    (Family::HashiParity, 8),
    (Family::HashiParity, 9),
    (Family::HashiParity, 10),
    (Family::Random3Sat, 100),
    (Family::Random3Sat, 125),
    (Family::Random3Sat, 150),
    (Family::Random3Sat, 175),
    (Family::Random3Sat, 200),
    (Family::Random3Sat, 225),
    (Family::Random3Sat, 250),
    (Family::Random3Sat, 275),
    (Family::Random3Sat, 300),
    (Family::Random3Sat, 325),
    (Family::Pigeonhole, 6),
    (Family::Pigeonhole, 7),
    (Family::Pigeonhole, 8),
    (Family::Pigeonhole, 9),
    (Family::Pigeonhole, 20),
    (Family::Queens, 8),
    (Family::Queens, 16),
    (Family::Queens, 24),
    (Family::Queens, 32),
    (Family::Queens, 48),
];

/// The random generator for one instance: same seed and index, same instance.
fn rng_for(seed: &str, index: usize, attempt: u32) -> StdRng {
    let digest = Sha256::digest(format!("{seed}/{index}/{attempt}"));
    StdRng::from_seed(digest.into())
}

/// The whole set for `seed`, see [`SPEC`].
pub fn generate(seed: &str) -> Vec<Instance> {
    generate_with(seed, SPEC)
}

/// Instances are independent (each has its own random generator), so they are generated in
/// parallel.
pub fn generate_with(seed: &str, spec: &[(Family, u32)]) -> Vec<Instance> {
    let mut counters = std::collections::HashMap::new();
    let names: Vec<String> = spec
        .iter()
        .map(|&(family, _)| {
            let n = counters.entry(family).or_insert(0u32);
            *n += 1;
            format!("{}-{:02}", family.id(), n)
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
                    let Some(&(family, size)) = spec.get(index) else {
                        break;
                    };
                    let instance =
                        generate_instance(seed, index, names[index].clone(), family, size);
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

fn generate_instance(
    seed: &str,
    index: usize,
    name: String,
    family: Family,
    size: u32,
) -> Instance {
    let mut attempt = 0;
    loop {
        let mut rng = rng_for(seed, index, attempt);
        if let Some((cnf, expected, witness)) = generate_one(family, size, &mut rng) {
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
                family,
                expected,
                cnf,
                witness,
            };
        }
        attempt += 1;
    }
}

type Generated = (Cnf, Answer, Option<Vec<i32>>);

/// One formula; `None` to try again with another random draw.
pub fn generate_one(family: Family, size: u32, rng: &mut StdRng) -> Option<Generated> {
    match family {
        Family::Hashi => hashi(size, rng, false),
        Family::HashiParity => hashi(size, rng, true),
        Family::Random3Sat => random_3sat(size, rng),
        // Up to 11 holes one pigeon too many, bigger ones fit
        Family::Pigeonhole => Some(pigeonhole(size, size >= 12)),
        Family::Queens => queens(size, rng),
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

/// How long splr may take to write a proof, and the checker to check it.
const PROOF_TIMEOUT: Duration = Duration::from_secs(120);

/// Whether splr, run again with certification, proves the formula unsatisfiable with a DRAT
/// proof that our checker accepts.
fn proven_unsat(cnf: &Cnf) -> bool {
    splr_proof(cnf).is_some_and(|proof| crate::drat::check(cnf, &proof, PROOF_TIMEOUT).is_ok())
}

/// A file that is removed when this goes out of scope (also on a panic).
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The DRAT proof (text) that splr writes when it finds the formula unsatisfiable; `None` if it
/// doesn't (or fails). The proof goes through a temporary file, splr can only write to files.
pub(crate) fn splr_proof(cnf: &Cnf) -> Option<Vec<u8>> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let file = TempFile(std::env::temp_dir().join(format!(
        "dojo-proof-{}-{}.drat",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    // Like `backend::sat`: the clause eliminator stays on (the default), splr only has whole
    // seconds as timeout
    let config = Config {
        quiet_mode: true,
        use_certification: true,
        io_pfile: file.0.clone(),
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
    unsat.then(|| std::fs::read(&file.0).unwrap_or_default())
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
mod tests {
    use super::*;

    #[test]
    fn spec_size() {
        assert_eq!(SPEC.len(), crate::SET_SIZE as usize);
    }

    #[test]
    fn small_set_is_labelled_correctly() {
        let spec = [
            (Family::Hashi, 7),
            (Family::HashiParity, 5),
            (Family::Random3Sat, 40),
            (Family::Pigeonhole, 4),
            (Family::Pigeonhole, 12),
            (Family::Queens, 8),
        ];
        let set = generate_with("test", &spec);
        assert_eq!(set.len(), spec.len());
        for instance in &set {
            if let Some(w) = &instance.witness {
                assert_eq!(instance.cnf.check(w), Some(true), "{}", instance.name);
                assert_eq!(instance.expected, Answer::Sat);
            }
            // Cross-check every label with splr
            let outcome = solve_clauses(
                &instance.cnf.clauses,
                instance.cnf.vars as usize,
                &SolverConfig::splr(),
                None,
            )
            .unwrap()
            .0;
            let answer = if matches!(outcome, SatOutcome::Sat(_)) {
                Answer::Sat
            } else {
                Answer::Unsat
            };
            assert_eq!(answer, instance.expected, "{}", instance.name);
        }
        assert_eq!(set[1].expected, Answer::Unsat);
        assert_eq!(set[3].expected, Answer::Unsat);
        assert_eq!(set[4].expected, Answer::Sat);
        // Same seed, same formulas
        let again = generate_with("test", &spec);
        assert!(set.iter().zip(&again).all(|(a, b)| a.cnf == b.cnf));
        assert_ne!(generate_with("other", &spec[..1])[0].cnf, set[0].cnf);
    }

    /// The formulas of a seed, checked in as digests. Workers generate a season's hidden set
    /// from its seed when they don't have it cached (a new volume, a cleared cache), so the
    /// same seed has to give the same formulas with every version of the worker.
    #[test]
    fn sets_are_unchanged() {
        const SETS_CHANGE: &str = "the same seed gives other formulas now (rand, splr, sha2, \
                                   the generator or the families changed): a worker without \
                                   the set cached would evaluate new submissions on another \
                                   hidden set than the earlier ones of the season. Only change \
                                   it together with a new season (DOJO_SEASON).";
        let mut rng = rng_for("golden", 3, 1);
        assert_eq!(
            rng.random::<u64>(),
            14_952_121_534_598_797_468,
            "{SETS_CHANGE}"
        );
        let spec = [
            (Family::Hashi, 7),
            (Family::HashiParity, 5),
            (Family::Random3Sat, 20),
            (Family::Pigeonhole, 4),
            (Family::Queens, 8),
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
                "random-3sat-01 a285979ad78b96b7",
                "pigeonhole-01 20cb5b50acf516fe",
                "queens-01 a73ac8eebddbb055",
            ],
            "{SETS_CHANGE}"
        );
    }
}
