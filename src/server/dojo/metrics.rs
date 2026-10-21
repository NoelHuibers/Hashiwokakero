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
//! - Scores at other budgets (the runs go on past the ranking budget) are sums like PAR-2.
//!   Their PAR-2 is rounded to the three decimals the board shows and crossovers to three
//!   digits: at many budgets the exact sums would tell the exact fuel of single formulas,
//!   which the cactus plot rounds for the same reason.

use dojo::InstanceResult;
use dojo::metrics::{
    Crossover, Portfolio, breakdown, budget_curve, budget_grid, crossovers, curve, round,
};

use crate::model::{
    DojoBudgetCurve, DojoBudgets, DojoCactus, DojoCrossover, DojoCurve, DojoEntry, DojoPortfolio,
};

/// Entries among the best at the ranking budget or at the run budget whose crossovers are
/// named: with every pair of a long board the sentences would say nothing.
const CROSSOVER_ENTRIES: usize = 5;

/// Fills in the breakdown and contribution of every entry of a board, each with its results,
/// and returns the board's portfolio, `None` if no entry has results. `budget` is the ranking
/// budget, `run_budget` how far the runs went. `reveal`: the season is over, see
/// [`dojo::metrics::breakdown`].
pub(super) fn apply<'a>(
    board: impl IntoIterator<Item = (&'a mut DojoEntry, &'a [InstanceResult])>,
    budget: u64,
    run_budget: u64,
    reveal: bool,
) -> Option<DojoPortfolio> {
    let board: Vec<(&mut DojoEntry, &[InstanceResult])> = board.into_iter().collect();
    let results: Vec<&[InstanceResult]> = board.iter().map(|(_, rs)| *rs).collect();
    let (portfolio, member) = Portfolio::new(&results, budget);
    let mut contributions = portfolio.contributions().into_iter();
    let mut curves = Vec::new();
    let mut members = Vec::new();
    for ((entry, rs), member) in board.into_iter().zip(member) {
        entry.breakdown = (!rs.is_empty()).then(|| breakdown(rs, budget, reveal));
        entry.contribution = None;
        if member {
            entry.contribution = contributions.next();
            let entry: &DojoEntry = entry;
            curves.push(curve_of(entry, rs));
            members.push((entry, rs));
        }
    }
    if portfolio.is_empty() {
        return None;
    }
    let budgets = budgets(&members, budget, run_budget, reveal);
    // The cactus plot goes as far as the runs did, its VBS too
    let (whole, _) = Portfolio::new(&results, run_budget.max(budget));
    Some(DojoPortfolio {
        entries: u32::try_from(portfolio.len()).unwrap_or(u32::MAX),
        vbs: portfolio.vbs(),
        cactus: DojoCactus {
            budget,
            run_budget,
            formulas: u32::try_from(portfolio.formulas()).unwrap_or(u32::MAX),
            vbs: whole.vbs_curve(),
            curves,
        },
        budgets: Some(budgets),
    })
}

/// The entries' scores at every budget of the grid and the crossovers of the leading ones.
fn budgets(
    members: &[(&DojoEntry, &[InstanceResult])],
    budget: u64,
    run_budget: u64,
    reveal: bool,
) -> DojoBudgets {
    let run_budget = run_budget.max(budget);
    let grid = budget_grid(budget, run_budget);
    let curves: Vec<DojoBudgetCurve> = members
        .iter()
        .map(|(entry, rs)| DojoBudgetCurve {
            id: entry.id,
            name: entry.name.clone(),
            house: entry.house,
            points: budget_curve(rs, &grid, reveal)
                .into_iter()
                .map(|mut point| {
                    point.par2 = (point.par2 * 1000.0).round() / 1000.0;
                    point
                })
                .collect(),
        })
        .collect();
    // The best few at either end, in board order
    let best_at = |at: usize| {
        let mut order: Vec<usize> = (0..curves.len()).collect();
        order.sort_by(|&a, &b| {
            let (pa, pb) = (&curves[a].points[at], &curves[b].points[at]);
            pa.par2.total_cmp(&pb.par2).then(a.cmp(&b))
        });
        order.truncate(CROSSOVER_ENTRIES);
        order
    };
    let official = grid.iter().position(|&b| b == budget).unwrap_or(0);
    let mut leading = best_at(official);
    leading.extend(best_at(grid.len() - 1));
    leading.sort_unstable();
    leading.dedup();
    let mut found = Vec::new();
    for (i, &a) in leading.iter().enumerate() {
        for &b in &leading[i + 1..] {
            let (first, second) = (members[a], members[b]);
            if let Some(c) = decisive(crossovers(first.1, second.1, grid[0], run_budget), budget) {
                let (ahead, behind) = if c.first {
                    (first.0.id, second.0.id)
                } else {
                    (second.0.id, first.0.id)
                };
                found.push(DojoCrossover {
                    budget: round(c.budget),
                    ahead,
                    behind,
                });
            }
        }
    }
    found.sort_by_key(|c| (c.budget, c.ahead, c.behind));
    DojoBudgets {
        budget,
        run_budget,
        curves,
        crossovers: found,
    }
}

/// The crossover of two entries worth a sentence: the last, if it comes after the ranking
/// `budget`. Lead changes back and forth are common where two solvers are close, the last one
/// says who leads from there up to the run budget.
fn decisive(mut found: Vec<Crossover>, budget: u64) -> Option<Crossover> {
    found.pop().filter(|c| c.budget > budget)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_crossover_past_the_budget_counts() {
        let at = |budget, first| Crossover { budget, first };
        assert_eq!(
            decisive(vec![at(5, true), at(8, false), at(12, true)], 7),
            Some(at(12, true))
        );
        // The last lead change comes before the budget: the official order holds beyond it
        assert_eq!(decisive(vec![at(3, true), at(5, false)], 7), None);
        assert_eq!(decisive(vec![at(7, true)], 7), None);
        assert_eq!(decisive(Vec::new(), 7), None);
    }
}
