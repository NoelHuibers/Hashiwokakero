//! Metrics beyond the official PAR-2 ranking, the way the SAT Competition and the portfolio
//! literature look at results.
//!
//! - [`breakdown`]: solved and PAR-2 split into SAT, UNSAT and ALL, overall and per family, as
//!   the SAT Competition reports them.
//! - [`Portfolio`]: what an entry adds to the others of its season. The virtual best solver
//!   (VBS) takes the best result of any entry on every formula. Standalone scores reward
//!   near-clones (two copies of a good solver both rank high), the marginal contribution
//!   punishes them (each adds nothing as long as the other is there); the Shapley value is fair
//!   to both (Fréchette, Kotthoff, Michalak, Rahwan, Hoos, Leyton-Brown: "Using the Shapley
//!   Value to Analyze Algorithm Portfolios", AAAI 2016). Per entry: the marginal VBS
//!   contribution, its relative form as on the Sparkle SAT Challenge 2018's leaderboard, the
//!   CASC-style state-of-the-art contribution and the exact Shapley value.
//!   Sparkle: <https://ada.liacs.nl/events/sparkle-sat-18/>, CASC: <https://tptp.org/CASC/>.
//! - [`curve`]: the points of a cactus plot, fuel of every solved formula sorted, so nothing in
//!   it says which formula a point is.
//!
//! Every function here works on results in set order and needs no feature, so the website and
//! the command line compute the same numbers.

use serde::{Deserialize, Serialize};

use crate::{Answer, Family, InstanceResult, count};

/// Billions of fuel, the unit of PAR-2.
const G: f64 = 1e9;

/// Solved formulas and PAR-2 of a group of formulas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Split {
    pub solved: u32,
    pub total: u32,
    /// Mean PAR-2 in billions of fuel units, 0 for no formulas.
    pub par2: f64,
}

impl Split {
    fn of<'a>(results: impl IntoIterator<Item = &'a InstanceResult>, budget: u64) -> Self {
        let mut split = Self::default();
        let mut sum = 0u128;
        for r in results {
            split.total += 1;
            if r.outcome.solved() {
                split.solved += 1;
            }
            sum += u128::from(cost(r, budget));
        }
        split.par2 = mean_g(sum, split.total);
        split
    }
}

/// A sum of fuel over `n` formulas as their mean in billions.
#[expect(
    clippy::cast_precision_loss,
    reason = "sums of at most a few hundred costs of 2·10^10 are far below 2^53"
)]
fn mean_g(sum: u128, n: u32) -> f64 {
    if n == 0 {
        0.0
    } else {
        sum as f64 / f64::from(n) / G
    }
}

/// The mean of PAR-2 costs, in billions.
fn par2(costs: &[u64]) -> f64 {
    mean_g(
        costs.iter().map(|&c| u128::from(c)).sum(),
        count(costs.len()),
    )
}

/// The PAR-2 cost of one result: its fuel when solved, twice the budget otherwise.
fn cost(r: &InstanceResult, budget: u64) -> u64 {
    if r.outcome.solved() {
        r.fuel
    } else {
        budget.saturating_mul(2)
    }
}

/// Results of a group of formulas split by their answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breakdown {
    pub all: Split,
    /// `None` when the answers have to stay secret: the held-out family while its season runs
    /// (how many of its formulas are satisfiable would give away which family it is).
    pub sat: Option<Split>,
    pub unsat: Option<Split>,
    /// UNSAT answers without an accepted proof, they count as unsolved.
    pub unverified: u32,
}

/// [`Breakdown`] of one family.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyBreakdown {
    /// [`Family::Mystery`] for the held-out family while its season runs.
    pub family: Family,
    pub breakdown: Breakdown,
}

/// SAT, UNSAT and ALL of a whole set and of each family.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryBreakdown {
    /// SAT and UNSAT leave out the `held_out` formulas, ALL has every one.
    pub total: Breakdown,
    /// Formulas whose answers stay secret: in ALL, not in SAT or UNSAT.
    pub held_out: u32,
    pub families: Vec<FamilyBreakdown>,
}

/// Solved and PAR-2 by answer, overall and per family, like the SAT Competition reports them.
///
/// `reveal`: the season is over. Until then the held-out family is [`Family::Mystery`] and its
/// formulas stay out of SAT and UNSAT, otherwise the number of satisfiable ones would tell
/// which family it is (each has its own, see `bench::mystery_spec`). Afterwards every family
/// shows under its own name.
pub fn breakdown(results: &[InstanceResult], budget: u64, reveal: bool) -> EntryBreakdown {
    let hidden = |r: &InstanceResult| !reveal && r.family.reserve();
    let split = |rs: &[&InstanceResult], with_answers: bool| {
        let by = |answer: Answer| {
            with_answers.then(|| {
                Split::of(
                    rs.iter()
                        .copied()
                        .filter(|r| r.expected == answer && !hidden(r)),
                    budget,
                )
            })
        };
        Breakdown {
            all: Split::of(rs.iter().copied(), budget),
            sat: by(Answer::Sat),
            unsat: by(Answer::Unsat),
            unverified: count(
                rs.iter()
                    .filter(|r| r.outcome == crate::Outcome::Unverified)
                    .count(),
            ),
        }
    };
    let shown = |r: &InstanceResult| {
        if hidden(r) { Family::Mystery } else { r.family }
    };
    let all: Vec<&InstanceResult> = results.iter().collect();
    // In the order of `Family::ALL`, then the mystery and the community
    let families = Family::ALL
        .into_iter()
        .chain([Family::Mystery, Family::Community])
        .filter_map(|family| {
            let rs: Vec<&InstanceResult> = results.iter().filter(|r| shown(r) == family).collect();
            (!rs.is_empty()).then(|| FamilyBreakdown {
                family,
                breakdown: split(&rs, family != Family::Mystery),
            })
        })
        .collect();
    EntryBreakdown {
        total: split(&all, true),
        held_out: count(results.iter().filter(|r| hidden(r)).count()),
        families,
    }
}

/// Fuel of every solved formula, sorted: the points of an entry in a cactus plot (the k-th
/// point is the fuel the k-th fastest solved formula took), in [`round`] precision.
pub fn curve(results: &[InstanceResult]) -> Vec<u64> {
    let mut fuel: Vec<u64> = results
        .iter()
        .filter(|r| r.outcome.solved())
        .map(|r| round(r.fuel))
        .collect();
    fuel.sort_unstable();
    fuel
}

/// Fuel rounded to three significant digits.
///
/// More than a plot on a log scale can show, and it keeps the plot from being a channel: a
/// solver could encode anything it reads in a formula in the exact fuel it burns on it, and
/// the sorted curve would publish one such number per formula. Three digits leave about ten
/// bits per point instead of thirty. The exact numbers are published with the archive.
pub fn round(fuel: u64) -> u64 {
    let mut unit = 1u64;
    while fuel / unit >= 1000 {
        unit *= 10;
    }
    (fuel + unit / 2) / unit * unit
}

/// What every entry of a season adds to the others: the portfolio of their results.
///
/// The cost of an entry on a formula is its PAR-2 cost (its fuel if solved, twice the budget
/// if not), the virtual best solver (VBS) of a set of entries takes the cheapest one on every
/// formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Portfolio {
    budget: u64,
    /// Per entry, per formula: the fuel of a solved formula, `None` for anything else.
    solved: Vec<Vec<Option<u64>>>,
    formulas: usize,
}

/// How much an entry contributes to the portfolio of its season, see [`Portfolio`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    /// Marginal VBS contribution: the VBS's PAR-2 without the entry minus with it, billions of
    /// fuel per formula. 0 for an entry the others always match or beat.
    pub marginal: f64,
    /// The Sparkle SAT Challenge 2018's absolute marginal contribution: log10 of the VBS's
    /// PAR-2 without the entry over the one with it.
    pub log_ratio: f64,
    /// Sparkle's relative marginal contribution, which its leaderboard ranked by: the entry's
    /// `log_ratio` as a share of all entries' (0 to 1, 0 when nobody contributes anything).
    pub relative: f64,
    /// State-of-the-art contribution (CASC): for every formula it solved, 1 over the number of
    /// entries that solved it, summed. Sums of all entries add up to the formulas the VBS
    /// solves.
    pub sota: f64,
    /// Shapley value of the portfolio game, billions of fuel per formula. Shapley values of
    /// all entries add up to twice the budget minus the VBS's PAR-2.
    pub shapley: f64,
}

impl Portfolio {
    /// The portfolio of `entries`, each with its results on the same formulas.
    ///
    /// Only entries with results on the very formulas of the first one with any take part
    /// (others were evaluated on another set, or kept no results); the second value says which.
    pub fn new(entries: &[&[InstanceResult]], budget: u64) -> (Self, Vec<bool>) {
        fn names(rs: &[InstanceResult]) -> Vec<&str> {
            rs.iter().map(|r| r.name.as_str()).collect()
        }
        let reference = entries.iter().find(|rs| !rs.is_empty()).map(|rs| names(rs));
        let member: Vec<bool> = entries
            .iter()
            .map(|rs| !rs.is_empty() && Some(names(rs)) == reference)
            .collect();
        let solved = entries
            .iter()
            .zip(&member)
            .filter(|(_, member)| **member)
            .map(|(rs, _)| {
                rs.iter()
                    .map(|r| r.outcome.solved().then_some(r.fuel))
                    .collect()
            })
            .collect();
        let formulas = reference.map_or(0, |r| r.len());
        (
            Self {
                budget,
                solved,
                formulas,
            },
            member,
        )
    }

    /// Entries in the portfolio.
    pub fn len(&self) -> usize {
        self.solved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.solved.is_empty()
    }

    /// Formulas of the set.
    pub fn formulas(&self) -> usize {
        self.formulas
    }

    fn unsolved(&self) -> u64 {
        self.budget.saturating_mul(2)
    }

    fn cost(&self, entry: usize, formula: usize) -> u64 {
        self.solved[entry][formula].unwrap_or_else(|| self.unsolved())
    }

    /// The VBS's cost on every formula, leaving out the entry `without`. Without any entry,
    /// nothing is solved.
    fn vbs_costs(&self, without: Option<usize>) -> Vec<u64> {
        (0..self.formulas)
            .map(|f| {
                (0..self.len())
                    .filter(|&e| Some(e) != without)
                    .map(|e| self.cost(e, f))
                    .min()
                    .unwrap_or_else(|| self.unsolved())
            })
            .collect()
    }

    /// The virtual best solver: formulas solved by any entry and its PAR-2.
    pub fn vbs(&self) -> Split {
        let costs = self.vbs_costs(None);
        Split {
            solved: count(costs.iter().filter(|&&c| c < self.unsolved()).count()),
            total: count(self.formulas),
            par2: par2(&costs),
        }
    }

    /// The VBS's points in a cactus plot, like [`curve`]: the best fuel of any entry on each
    /// formula solved by one, sorted.
    pub fn vbs_curve(&self) -> Vec<u64> {
        let mut fuel: Vec<u64> = (0..self.formulas)
            .filter_map(|f| (0..self.len()).filter_map(|e| self.solved[e][f]).min())
            .map(round)
            .collect();
        fuel.sort_unstable();
        fuel
    }

    /// Marginal VBS contribution of every entry, in PAR-2 and as Sparkle's log ratio (see
    /// [`Contribution`]).
    pub fn marginal(&self) -> Vec<(f64, f64)> {
        let with = par2(&self.vbs_costs(None));
        (0..self.len())
            .map(|e| {
                let without = par2(&self.vbs_costs(Some(e)));
                let log_ratio = if with > 0.0 {
                    (without / with).log10()
                } else {
                    0.0
                };
                (without - with, log_ratio)
            })
            .collect()
    }

    /// CASC's state-of-the-art contribution of every entry, summed over the formulas.
    ///
    /// CASC averages 1/k over the problems a system solved, which ranks a solver of a single
    /// formula nobody else solves (1.0) above one that solves that formula and every other one
    /// too. The sum (Kaliszyk and Urban's Σ-SOTAC, "Learning-assisted automated reasoning with
    /// Flyspeck", 2014) doesn't: it gives every formula solved by anyone one point, shared equally
    /// by the k entries that solved it, so the sums add up to the formulas the VBS solves. It
    /// is the Shapley value of the game "formulas solved by any member of the coalition".
    pub fn sota(&self) -> Vec<f64> {
        let mut sota = vec![0.0; self.len()];
        for f in 0..self.formulas {
            let solvers: Vec<usize> = (0..self.len())
                .filter(|&e| self.solved[e][f].is_some())
                .collect();
            for &e in &solvers {
                sota[e] += 1.0 / f64::from(count(solvers.len()));
            }
        }
        sota
    }

    /// The exact Shapley value of every entry in the portfolio game, in billions of fuel per
    /// formula.
    ///
    /// The game: a coalition S of entries is worth what its VBS saves over solving nothing,
    /// `v(S) = Σ_f (2·budget − min_{s∈S} c_{s,f})`, with `v(∅) = 0` (`c_{s,f}` is the PAR-2
    /// cost of entry s on formula f). The Shapley value is linear in the game, so it is the
    /// sum of the Shapley values of the one-formula games `v_f`.
    ///
    /// For one formula write `a_s = 2·budget − c_{s,f} ≥ 0` (0 if unsolved), so
    /// `v_f(S) = max_{s∈S} a_s`. Sort the n entries so `a_(1) ≤ … ≤ a_(n)`, with `a_(0) = 0`.
    /// Then `max_{s∈S} a_s = Σ_{k=1..n} (a_(k) − a_(k−1)) · [S ∩ {(k), …, (n)} ≠ ∅]`: the
    /// coalition earns the k-th layer as soon as any entry at least as good as the k-th one is
    /// in it. Each layer is a game of its own: worth `d_k = a_(k) − a_(k−1)` to every
    /// coalition that meets `T_k = {(k), …, (n)}`, nothing otherwise. Entries outside `T_k`
    /// never change its worth (null players, Shapley value 0), those in `T_k` are symmetric,
    /// and together they get `d_k` (efficiency), so each gets `d_k / (n − k + 1)`. Summing
    /// the layers an entry belongs to:
    ///
    /// `φ_(j) = Σ_{k=1..j} (a_(k) − a_(k−1)) / (n − k + 1)`
    ///
    /// This is the airport game of Littlechild and Owen (1973) with savings instead of costs.
    /// Ties need no care: equal values give empty layers. Sorting makes it O(n log n) per
    /// formula instead of the 2^n coalitions of the definition, which
    /// `tests::shapley_matches_the_definition` checks it against.
    pub fn shapley(&self) -> Vec<f64> {
        let n = self.len();
        let mut phi = vec![0.0; n];
        let unsolved = self.unsolved();
        for f in 0..self.formulas {
            let mut savings: Vec<(u64, usize)> = (0..n)
                .map(|e| (unsolved - self.cost(e, f).min(unsolved), e))
                .collect();
            savings.sort_unstable();
            let (mut below, mut share) = (0u64, 0.0);
            for (k, &(a, e)) in savings.iter().enumerate() {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "savings are at most 2·10^10, far below 2^53"
                )]
                let layer = (a - below) as f64;
                share += layer / f64::from(count(n - k));
                below = a;
                phi[e] += share;
            }
        }
        let formulas = f64::from(count(self.formulas)).max(1.0);
        phi.into_iter().map(|p| p / formulas / G).collect()
    }

    /// Every metric of every entry, in the order of the portfolio's entries.
    pub fn contributions(&self) -> Vec<Contribution> {
        let marginal = self.marginal();
        let total: f64 = marginal.iter().map(|(_, log_ratio)| log_ratio).sum();
        marginal
            .into_iter()
            .zip(self.sota())
            .zip(self.shapley())
            .map(|(((marginal, log_ratio), sota), shapley)| Contribution {
                marginal,
                log_ratio,
                relative: if total > 0.0 { log_ratio / total } else { 0.0 },
                sota,
                shapley,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/metrics.rs"]
mod tests;
