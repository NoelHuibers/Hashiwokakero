//! The metrics beyond PAR-2 for a board: SAT/UNSAT/ALL per entry, the virtual best solver,
//! every entry's contribution to it and the cactus plot (see [`dojo::metrics`]).
//!
//! All of it comes from the per-formula results the site keeps of every finished submission.
//! While a season runs they stay private, and so does everything here that could tell one
//! formula from another:
//!
//! - Cactus curves are sorted: a point is the k-th smallest fuel of an entry, not a formula.
//!   With one entry the VBS curve is that entry's; with a few it says how many formulas each
//!   one was the fastest on, but not which, and nothing about formulas nobody solved.
//! - Fuel in curves is rounded to three digits (see `dojo::metrics::round`), so a solver can't
//!   publish what it read in each formula through the exact fuel it burns.
//! - Contributions are sums over all formulas, like PAR-2 itself. A probe that solves only some
//!   formulas learns how the field did on those in total, never an answer: to solve a formula it
//!   has to know the answer already.
//! - The held-out family stays out of SAT and UNSAT until the season is over: its share of
//!   satisfiable formulas would tell which family it is.

use dojo::InstanceResult;
use dojo::metrics::{Portfolio, breakdown, curve};

use crate::model::{DojoCactus, DojoCurve, DojoEntry, DojoPortfolio};

/// Fills in the breakdown and contribution of every entry of a board, each with its results,
/// and returns the board's portfolio, `None` if no entry has results. `reveal`: the season is
/// over, see [`dojo::metrics::breakdown`].
pub(super) fn apply<'a>(
    board: impl IntoIterator<Item = (&'a mut DojoEntry, &'a [InstanceResult])>,
    budget: u64,
    reveal: bool,
) -> Option<DojoPortfolio> {
    let board: Vec<(&mut DojoEntry, &[InstanceResult])> = board.into_iter().collect();
    let results: Vec<&[InstanceResult]> = board.iter().map(|(_, rs)| *rs).collect();
    let (portfolio, member) = Portfolio::new(&results, budget);
    let mut contributions = portfolio.contributions().into_iter();
    let mut curves = Vec::new();
    for ((entry, rs), member) in board.into_iter().zip(member) {
        entry.breakdown = (!rs.is_empty()).then(|| breakdown(rs, budget, reveal));
        entry.contribution = None;
        if member {
            entry.contribution = contributions.next();
            curves.push(curve_of(entry, rs));
        }
    }
    (!portfolio.is_empty()).then(|| DojoPortfolio {
        entries: u32::try_from(portfolio.len()).unwrap_or(u32::MAX),
        vbs: portfolio.vbs(),
        cactus: DojoCactus {
            budget,
            formulas: u32::try_from(portfolio.formulas()).unwrap_or(u32::MAX),
            vbs: portfolio.vbs_curve(),
            curves,
        },
    })
}

pub(super) fn curve_of(entry: &DojoEntry, results: &[InstanceResult]) -> DojoCurve {
    DojoCurve {
        id: entry.id,
        name: entry.name.clone(),
        house: entry.house,
        fuel: curve(results),
    }
}

/// The results of a row, empty when there are none (or they don't load).
pub(super) fn results_of(json: Option<String>) -> Vec<InstanceResult> {
    json.and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default()
}
