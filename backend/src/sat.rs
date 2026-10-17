//! Runs one of the SAT solvers on clauses in memory (no files, safe to call from many threads).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use common::{Heuristic, Restarts, SolverConfig, SolverKind};
use sat_solver::{cdcl::CDCL, cli, dpll::DIMACSOutput, dpll::DPLL};
use splr::{
    Certificate, Config, SolveIF, Solver, SolverError, assign::property::Tusize,
    types::PropertyDereference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SatOutcome {
    /// `model[v]` is the value of variable `v` (index 0 is unused).
    Sat(Vec<bool>),
    Unsat,
    Timeout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SatStats {
    pub decisions: Option<u64>,
    pub restarts: Option<u64>,
}

impl SatStats {
    /// Adds the numbers of another run.
    pub fn add(&mut self, other: SatStats) {
        fn sum(a: Option<u64>, b: Option<u64>) -> Option<u64> {
            match (a, b) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            }
        }
        self.decisions = sum(self.decisions, other.decisions);
        self.restarts = sum(self.restarts, other.restarts);
    }
}

// Own CDCL: clause deletion parameters (k-bounded / m-size relevance based learning)
const CDCL_K: usize = 10;
const CDCL_M: usize = 10;

/// Solves `clauses` over the variables `1..=num_vars` with the configured solver.
///
/// Returns `Err` only if the solver failed (e.g. a bug in our own solvers); a model that doesn't
/// satisfy the clauses is reported as an error as well.
pub fn solve_clauses(
    clauses: &[Vec<i32>],
    num_vars: usize,
    config: &SolverConfig,
    deadline: Option<Instant>,
) -> Result<(SatOutcome, SatStats), String> {
    let config = config.normalized();
    if clauses.iter().any(|c| c.is_empty()) {
        return Ok((SatOutcome::Unsat, SatStats::default()));
    }
    if clauses.is_empty() {
        return Ok((
            SatOutcome::Sat(vec![false; num_vars + 1]),
            SatStats::default(),
        ));
    }
    if deadline.is_some_and(|d| Instant::now() >= d) {
        return Ok((SatOutcome::Timeout, SatStats::default()));
    }
    let num_vars = clauses
        .iter()
        .flatten()
        .map(|l| l.unsigned_abs() as usize)
        .max()
        .unwrap_or(0)
        .max(num_vars);

    let (outcome, stats) = match config.kind {
        // splr 0.19 trips debug assertions in its eliminator on some formulas (debug builds
        // only), fall back to our CDCL then
        SolverKind::Splr => {
            match catch_unwind(AssertUnwindSafe(|| solve_splr(clauses, deadline))) {
                Ok(result) => result?,
                Err(_) => {
                    let cdcl = SolverConfig {
                        kind: SolverKind::Cdcl,
                        ..SolverConfig::default()
                    };
                    solve_own(clauses, num_vars, &cdcl, deadline)?
                }
            }
        }
        SolverKind::Cdcl | SolverKind::Dpll => catch_unwind(AssertUnwindSafe(|| {
            solve_own(clauses, num_vars, &config, deadline)
        }))
        .map_err(|e| {
            let msg = e
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            format!("{} crashed: {msg}", config.kind.label())
        })??,
    };

    let outcome = match outcome {
        Raw::Model(lits) => {
            let mut model = vec![false; num_vars + 1];
            for l in lits {
                if let Some(v) = model.get_mut(l.unsigned_abs() as usize) {
                    *v = l > 0;
                }
            }
            let satisfied = clauses.iter().all(|c| {
                c.iter()
                    .any(|&l| model[l.unsigned_abs() as usize] == (l > 0))
            });
            if !satisfied {
                return Err(format!(
                    "{} returned a model that doesn't satisfy the formula",
                    config.kind.label()
                ));
            }
            SatOutcome::Sat(model)
        }
        Raw::Unsat => SatOutcome::Unsat,
        Raw::Timeout => SatOutcome::Timeout,
    };
    Ok((outcome, stats))
}

/// Result of a solver before the model is checked: SAT models are lists of literals.
enum Raw {
    Model(Vec<i32>),
    Unsat,
    Timeout,
}

fn solve_splr(clauses: &[Vec<i32>], deadline: Option<Instant>) -> Result<(Raw, SatStats), String> {
    // Keep the clause eliminator enabled (the default): splr 0.19 without it reports wrong
    // UNSAT results at the root level on our formulas
    let mut config = Config {
        quiet_mode: true,
        ..Default::default()
    };
    if let Some(deadline) = deadline {
        // splr only supports whole seconds; we check the real deadline afterwards
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64();
        config.c_timeout = remaining.ceil().max(1.0);
    }
    let mut solver = match Solver::try_from((config, clauses)) {
        Ok(solver) => solver,
        Err(Ok(Certificate::UNSAT)) => return Ok((Raw::Unsat, SatStats::default())),
        Err(Ok(Certificate::SAT(model))) => {
            return Ok((Raw::Model(model), SatStats::default()));
        }
        Err(Err(SolverError::EmptyClause | SolverError::RootLevelConflict(_))) => {
            return Ok((Raw::Unsat, SatStats::default()));
        }
        Err(Err(e)) => return Err(format!("splr failed: {e}")),
    };
    let result = solver.solve();
    let stats = SatStats {
        decisions: Some(solver.asg.derefer(Tusize::NumDecision) as u64),
        restarts: Some(solver.asg.derefer(Tusize::NumRestart) as u64),
    };
    let timed_out = deadline.is_some_and(|d| Instant::now() >= d);
    match result {
        Ok(Certificate::SAT(model)) => Ok((Raw::Model(model), stats)),
        Ok(Certificate::UNSAT) => Ok((Raw::Unsat, stats)),
        Err(SolverError::TimeOut) => Ok((Raw::Timeout, stats)),
        Err(SolverError::EmptyClause | SolverError::RootLevelConflict(_)) => {
            Ok((Raw::Unsat, stats))
        }
        Err(_) if timed_out => Ok((Raw::Timeout, stats)),
        Err(e) => Err(format!("splr failed: {e}")),
    }
}

fn own_heuristic(h: Heuristic) -> cli::Heuristic {
    match h {
        Heuristic::Arbitrary => cli::Heuristic::Arbitrary,
        Heuristic::Dlis => cli::Heuristic::DLIS,
        Heuristic::Dlcs => cli::Heuristic::DLCS,
        Heuristic::Mom => cli::Heuristic::MOM,
        Heuristic::Boehm => cli::Heuristic::Boehm,
        Heuristic::JeroslawWang => cli::Heuristic::JeroslawWang,
        Heuristic::Vsids => cli::Heuristic::VSIDS,
        Heuristic::Custom => cli::Heuristic::Custom,
    }
}

/// Restart parameters of our CDCL: (threshold, use luby, factor in percent).
pub fn restart_params(restarts: Restarts) -> (Option<u32>, bool, Option<u32>) {
    match restarts {
        Restarts::None => (None, false, None),
        Restarts::Fixed { threshold } => (Some(threshold.max(1)), false, None),
        // A factor below 100% would shrink the interval to 0 (restart on every conflict)
        Restarts::Geometric {
            threshold,
            factor_percent,
        } => (Some(threshold.max(1)), false, Some(factor_percent.max(100))),
        Restarts::Luby { unit } => (Some(unit.max(1)), true, None),
    }
}

fn solve_own(
    clauses: &[Vec<i32>],
    num_vars: usize,
    config: &SolverConfig,
    deadline: Option<Instant>,
) -> Result<(Raw, SatStats), String> {
    let heuristic = own_heuristic(config.heuristic);
    let (result, stats) = match config.kind {
        SolverKind::Dpll => {
            let mut dpll = DPLL::new(clauses.to_vec(), num_vars, clauses.len(), heuristic, false)
                .with_deadline(deadline);
            let result = dpll.solve();
            let stats = SatStats {
                decisions: Some(dpll.decisions()),
                restarts: None,
            };
            (result, stats)
        }
        _ => {
            let (threshold, luby, factor) = restart_params(config.restarts);
            let mut cdcl = CDCL::new(
                clauses.to_vec(),
                num_vars,
                clauses.len(),
                heuristic,
                config.subsumption,
                threshold,
                luby,
                factor,
                CDCL_K,
                CDCL_M,
                None,
            )
            .with_deadline(deadline);
            let result = cdcl.solve();
            let stats = SatStats {
                decisions: Some(cdcl.decisions()),
                restarts: Some(cdcl.restarts()),
            };
            (result, stats)
        }
    };
    match result {
        Ok(DIMACSOutput::Sat(model)) => Ok((Raw::Model(model), stats)),
        Ok(DIMACSOutput::Unsat) => Ok((Raw::Unsat, stats)),
        Ok(DIMACSOutput::Timeout) => Ok((Raw::Timeout, stats)),
        Err(e) => Err(format!("{} failed: {}", config.kind.label(), e.message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configs() -> Vec<SolverConfig> {
        vec![
            SolverConfig::splr(),
            SolverConfig::default(),
            SolverConfig {
                kind: SolverKind::Dpll,
                heuristic: Heuristic::Dlis,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn small_formulas() {
        for config in configs() {
            let (out, _) =
                solve_clauses(&[vec![1, -2], vec![-1, 2], vec![1, 2]], 2, &config, None).unwrap();
            assert_eq!(out, SatOutcome::Sat(vec![false, true, true]), "{config:?}");
            let (out, _) =
                solve_clauses(&[vec![1, 2], vec![-1], vec![-2]], 2, &config, None).unwrap();
            assert_eq!(out, SatOutcome::Unsat, "{config:?}");
            let (out, _) = solve_clauses(&[vec![1], vec![]], 2, &config, None).unwrap();
            assert_eq!(out, SatOutcome::Unsat);
            let (out, _) = solve_clauses(&[], 2, &config, None).unwrap();
            assert_eq!(out, SatOutcome::Sat(vec![false; 3]));
        }
    }

    #[test]
    fn expired_deadline_times_out() {
        for config in configs() {
            let (out, _) = solve_clauses(&[vec![1, 2]], 2, &config, Some(Instant::now())).unwrap();
            assert_eq!(out, SatOutcome::Timeout);
        }
    }
}
