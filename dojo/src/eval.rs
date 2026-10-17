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
}

impl Default for Options {
    fn default() -> Self {
        Options {
            limits: Limits::default(),
            threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
            stderr: false,
        }
    }
}

/// The seed for an instance's random numbers.
fn seed(instance: &Instance) -> u64 {
    let digest = Sha256::digest(instance.name.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().unwrap_or_default())
}

pub fn run_one(sandbox: &Sandbox, solver: &Solver, instance: &Instance, options: &Options) -> RunResult {
    let input = instance.cnf.dimacs(&format!("{} from the Hashi SAT Dojo", instance.name));
    let run = sandbox.run(solver, &input, seed(instance), &options.limits, options.stderr);
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
        wall_ms: run.wall.as_millis() as u64,
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
    results.into_inner().unwrap_or_default().into_iter().flatten().collect()
}
