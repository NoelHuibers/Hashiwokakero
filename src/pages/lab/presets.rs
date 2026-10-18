//! The solver configurations the lab offers to race.

use common::{Heuristic, Restarts, SolverConfig, SolverKind};

fn cdcl(heuristic: Heuristic, restarts: Restarts, subsumption: bool) -> SolverConfig {
    SolverConfig {
        kind: SolverKind::Cdcl,
        heuristic,
        restarts,
        subsumption,
        ..Default::default()
    }
}

fn dpll(heuristic: Heuristic) -> SolverConfig {
    SolverConfig {
        kind: SolverKind::Dpll,
        heuristic,
        restarts: Restarts::None,
        ..Default::default()
    }
}

pub(super) fn presets() -> Vec<SolverConfig> {
    let luby = Restarts::Luby { unit: 32 };
    let mut list = vec![
        cdcl(Heuristic::Vsids, luby, false),
        cdcl(Heuristic::Vsids, Restarts::None, false),
        cdcl(
            Heuristic::Vsids,
            Restarts::from_id("geometric").unwrap_or_default(),
            false,
        ),
        cdcl(
            Heuristic::Vsids,
            Restarts::from_id("fixed").unwrap_or_default(),
            false,
        ),
        cdcl(Heuristic::Vsids, luby, true),
        cdcl(Heuristic::Arbitrary, luby, false),
    ];
    list.extend(SolverKind::Dpll.heuristics().iter().map(|&h| dpll(h)));
    list.push(SolverConfig::splr());
    list
}
