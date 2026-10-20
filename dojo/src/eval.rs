//! Evaluating a submission on a whole set, several instances at a time. Since fuel doesn't
//! depend on timing, running in parallel changes nothing about the results.
//!
//! Every run gets a fresh proof directory; an UNSAT answer is judged by checking the proof the
//! solver left there (see [`crate::proof`]), after the run, without fuel.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::bench::Instance;
use crate::output::{judge, parse};
use crate::proof::{Checker, Format, PROOF_FILE, TempDir, Verdict};
use crate::sandbox::{End, Limits, Run, Sandbox, Solver};
use crate::{Outcome, RunResult};

#[derive(Clone, Debug)]
pub struct Options {
    pub limits: Limits,
    pub threads: usize,
    /// Pass the solver's stderr through (for local runs).
    pub stderr: bool,
    /// The seed of the set, which the random numbers of every run depend on (see [`rng_seed`]).
    pub set_seed: String,
    /// Checks the proofs of UNSAT answers.
    pub checker: Checker,
    /// Where the proof directories of the runs go.
    pub proof_root: PathBuf,
    /// Keep the proof directories (one per formula, named like it) instead of removing them.
    pub keep_proofs: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            threads: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            stderr: false,
            set_seed: crate::DEV_SEED.to_string(),
            checker: Checker::builtin(),
            proof_root: std::env::temp_dir(),
            keep_proofs: false,
        }
    }
}

/// The verdict on the proof a run left in `dir`.
pub fn check_proof(
    instance: &Instance,
    run: &Run,
    dir: &Path,
    limits: &Limits,
    checker: &Checker,
) -> Verdict {
    let path = dir.join(PROOF_FILE);
    if run.proof_too_large {
        return Verdict::Unverified(format!(
            "the proof is larger than {} MiB",
            limits.proof_bytes >> 20
        ));
    }
    match std::fs::metadata(&path) {
        Err(_) => Verdict::Unverified("no proof".into()),
        // A sparse file can be larger than what was written
        Ok(meta) if meta.len() > limits.proof_bytes => Verdict::Unverified(format!(
            "the proof is larger than {} MiB",
            limits.proof_bytes >> 20
        )),
        Ok(_) => checker.check(&instance.cnf, &path, Format::declared(&run.stdout)),
    }
}

/// The seed of the random numbers (`random_get`) of a run.
///
/// From the set's seed and the formula's name, so the same name gets other random numbers in
/// every season and in the dev and hidden sets, and still the same ones on every run.
pub fn rng_seed(set_seed: &str, instance: &str) -> u64 {
    let digest = Sha256::digest(format!("{set_seed}/{instance}").as_bytes());
    u64::from_le_bytes(digest[..8].try_into().unwrap_or_default())
}

pub fn run_one(
    sandbox: &Sandbox,
    solver: &Solver,
    instance: &Instance,
    options: &Options,
) -> RunResult {
    let input = instance
        .cnf
        .dimacs(&format!("{} from the Hashi SAT Dojo", instance.name));
    let dir = if options.keep_proofs {
        TempDir::named(&options.proof_root, &instance.name)
    } else {
        TempDir::new(&options.proof_root, "dojo-proof")
    };
    let run = sandbox.run(
        solver,
        &input,
        rng_seed(&options.set_seed, &instance.name),
        &options.limits,
        options.stderr,
        dir.as_ref().ok().map(TempDir::path),
    );
    let outcome = match run.end {
        End::Exited => judge(
            &instance.cnf,
            instance.expected,
            &parse(&run.stdout),
            || match &dir {
                Ok(dir) => check_proof(
                    instance,
                    &run,
                    dir.path(),
                    &options.limits,
                    &options.checker,
                ),
                Err(e) => Verdict::Unverified(format!("no proof directory: {e}")),
            },
        ),
        End::OutOfFuel | End::WallClock => Outcome::Timeout,
        End::Crashed(_) => Outcome::Crashed,
    };
    if options.keep_proofs
        && let Ok(dir) = dir
    {
        dir.keep();
    }
    RunResult {
        name: instance.name.clone(),
        family: instance.family,
        expected: instance.expected,
        outcome,
        fuel: run.fuel,
        wall_ms: u64::try_from(run.wall.as_millis()).unwrap_or(u64::MAX),
    }
}

/// Runs every instance; `progress` is called after each one with the number done so far.
pub fn evaluate(
    sandbox: &Sandbox,
    solver: &Solver,
    set: &[Instance],
    options: &Options,
    progress: impl Fn(usize, &RunResult) + Sync,
) -> Vec<RunResult> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<RunResult>>> = Mutex::new(vec![None; set.len()]);
    std::thread::scope(|scope| {
        for _ in 0..options.threads.clamp(1, set.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(instance) = set.get(i) else { break };
                    let result = run_one(sandbox, solver, instance, options);
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(n, &result);
                    if let Ok(mut results) = results.lock() {
                        results[i] = Some(result);
                    }
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changing these changes the random numbers of every run, so the score of every solver that
    /// uses them: bump `RULES_VERSION` with it.
    #[test]
    fn rng_seeds_depend_on_the_set() {
        assert_eq!(
            rng_seed("hashi-dojo-dev-1", "queens-01"),
            9_993_982_788_544_393_026
        );
        assert_ne!(
            rng_seed("hashi-dojo-dev-1", "queens-01"),
            rng_seed("another seed", "queens-01")
        );
        assert_ne!(
            rng_seed("hashi-dojo-dev-1", "queens-01"),
            rng_seed("hashi-dojo-dev-1", "queens-02")
        );
        assert_eq!(Options::default().set_seed, crate::DEV_SEED);
    }

    use crate::cnf::Cnf;
    use crate::proof::{CheckLimits, Tools};
    use crate::sandbox::testing::unsat_solver;
    use crate::{Answer, Family};

    /// (1 ∨ 2), (¬1 ∨ 2), (1 ∨ ¬2), (¬1 ∨ ¬2): unsatisfiable, but not by unit propagation.
    fn instance(expected: Answer) -> Instance {
        Instance {
            name: "pigeonhole-01".into(),
            family: Family::Pigeonhole,
            expected,
            cnf: Cnf::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]]),
            witness: None,
        }
    }

    fn outcome(wasm: &[u8], instance: &Instance, options: &Options) -> Outcome {
        let sandbox = Sandbox::new().unwrap();
        let solver = sandbox.load(wasm).unwrap();
        run_one(&sandbox, &solver, instance, options).outcome
    }

    /// Every way an UNSAT answer can be judged, through real runs.
    #[test]
    fn judging_matrix() {
        let options = Options::default();
        let unsat = instance(Answer::Unsat);
        let solved = Outcome::Solved(Answer::Unsat);
        let cases: [(Option<&[u8]>, &str, Outcome); 7] = [
            (None, "", Outcome::Unverified),
            (Some(b"-1 0\n0\n"), "", solved),
            // Binary DRAT: a -1 0, a 0
            (Some(b"a\x03\x00a\x00"), "", solved),
            (
                Some(b"5 -1 0 2 4 0\n6 0 5 1 3 0\n"),
                "c proof lrat\n",
                solved,
            ),
            (Some(b"0\n"), "", Outcome::Wrong),
            (
                Some(b"5 -1 0 2 0\n6 0 5 1 3 0\n"),
                "c proof lrat\n",
                Outcome::Wrong,
            ),
            // An empty proof doesn't refute this formula
            (Some(b""), "", Outcome::Wrong),
        ];
        for (proof, stdout, expected) in cases {
            assert_eq!(
                outcome(&unsat_solver(proof, stdout), &unsat, &options),
                expected,
                "{proof:?} {stdout:?}"
            );
        }
        // UNSAT on a satisfiable formula is wrong, even with a proof
        assert_eq!(
            outcome(
                &unsat_solver(Some(b"-1 0\n0\n"), ""),
                &instance(Answer::Sat),
                &options
            ),
            Outcome::Wrong
        );
        // Larger than the limit: no proof
        let mut capped = options.clone();
        capped.limits.proof_bytes = 3;
        assert_eq!(
            outcome(&unsat_solver(Some(b"-1 0\n0\n"), ""), &unsat, &capped),
            Outcome::Unverified
        );
        // The checker out of time: no verdict
        let mut hasty = options;
        hasty.checker = Checker::new(
            Tools::Builtin,
            CheckLimits {
                time: Duration::ZERO,
                memory_mb: 64,
            },
            1,
        );
        assert_eq!(
            outcome(&unsat_solver(Some(b"-1 0\n0\n"), ""), &unsat, &hasty),
            Outcome::Unverified
        );
    }

    /// `dojo run --proofs DIR` keeps a directory per formula, others are removed.
    #[test]
    fn proof_directories() {
        let root = TempDir::new(&std::env::temp_dir(), "dojo-eval-test").unwrap();
        let mut options = Options {
            proof_root: root.path().to_path_buf(),
            ..Options::default()
        };
        let wasm = unsat_solver(Some(b"-1 0\n0\n"), "");
        outcome(&wasm, &instance(Answer::Unsat), &options);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        options.keep_proofs = true;
        outcome(&wasm, &instance(Answer::Unsat), &options);
        let kept = root.path().join("pigeonhole-01").join(PROOF_FILE);
        assert_eq!(std::fs::read(kept).unwrap(), b"-1 0\n0\n");
    }
}
