//! Evaluating a submission on a whole set, several instances at a time. Since fuel doesn't
//! depend on timing, running in parallel changes nothing about the results.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest, Sha256};

use crate::bench::Instance;
use crate::output::{judge, parse};
use crate::sandbox::{End, Limits, Sandbox, Solver};
use crate::{Outcome, RunResult};

#[derive(Clone, Debug)]
pub struct Options {
    pub limits: Limits,
    pub threads: usize,
    /// Pass the solver's stderr through (for local runs).
    pub stderr: bool,
    /// The seed of the set, which the random numbers of every run depend on (see [`rng_seed`]).
    pub set_seed: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            threads: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            stderr: false,
            set_seed: crate::DEV_SEED.to_string(),
        }
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
    // No family, label or public index reaches a submission, including through its random
    // stream. Names remain only in the host-side report.
    let input = instance.cnf.dimacs("");
    let run = sandbox.run(
        solver,
        &input,
        rng_seed(&options.set_seed, &instance.name),
        &options.limits,
        options.stderr,
    );
    let outcome = match run.end {
        End::Exited => judge(&instance.cnf, instance.expected, &parse(&run.stdout)),
        End::OutOfFuel | End::WallClock => Outcome::Timeout,
        End::Crashed(_) => Outcome::Crashed,
    };
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
}
